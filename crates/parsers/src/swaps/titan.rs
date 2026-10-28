use anyhow::{bail, Result};
use async_trait::async_trait;
use rust_decimal::Decimal;
use step_ingestooor_sdk::{
    dooot::{Dooot, SwapEventDooot},
    trait_defs::transaction::{FlattenedIxWithIndex, TransactionLike},
    utils::{
        spl_ixs::TokenParsedInstruction, step_account_reader::StepAccountReader,
        step_utils::StepUtils, util_fns::get_partially_decoded_variant,
    },
};

use crate::TransactionParserTrait;

pub const TITAN_SWAPS_PROGRAM_ID: &str = "T1TANpTeScyeqVzzgNViGDNrkQ6qHz9KrSBS4aNXvGT";
pub const TITAN_SWAP_DISC: &[u8] = &[86, 183, 163, 144, 0, 50, 173, 28];
pub const TITAN_SWAP_DISC_2: &[u8] = &[0xf9, 0x5b, 0x54, 0x21, 0x45, 0x16, 0x00, 0x87];
pub struct TitanSwaps;

#[async_trait]
impl TransactionParserTrait for TitanSwaps {
    fn get_name(&self) -> &'static str {
        "TitanSwaps"
    }

    fn get_filters(&self) -> Vec<crate::TransactionParserFilter> {
        vec![crate::TransactionParserFilter::ProgramId(
            TITAN_SWAPS_PROGRAM_ID.into(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let grouped_ixs = txn.get_grouped_instructions(|ix| {
            let pid = ix.get_ix_program_id(txn);
            if pid != TITAN_SWAPS_PROGRAM_ID {
                return false;
            }

            let Some(ix) = get_partially_decoded_variant(ix.instruction) else {
                return false;
            };

            let Ok(dec) = bs58::decode(&ix.data).into_vec() else {
                return false;
            };

            let Some(disc) = dec.get(0..8) else {
                return false;
            };

            matches!(disc, TITAN_SWAP_DISC | TITAN_SWAP_DISC_2)
        })?;

        for g_ix in grouped_ixs {
            let Some(parent_ix) =
                get_partially_decoded_variant(g_ix.parent_instruction.instruction)
            else {
                continue;
            };

            let auth = &parent_ix.accounts[0];
            let in_mint = &parent_ix.accounts[2];
            let out_mint = &parent_ix.accounts[4];

            if in_mint == out_mint {
                // Arb, we don't care rn
                continue;
            }

            let transfer_amounts = txn.get_total_transfer_amounts_for_pubkey_flattened_ixs(
                g_ix.involved_instructions.iter(),
                &[auth],
                &[in_mint],
                &[out_mint],
            )?;

            let in_amount = transfer_amounts.get_in_amount_i128(in_mint, auth);
            let out_amount = transfer_amounts.get_out_amount_i128(out_mint, auth);

            if in_amount < 0 || out_amount < 0 {
                continue;
            }

            let decoded_ix_data = bs58::decode(&parent_ix.data).into_vec()?;

            let fee_rates = Self::get_fee_rates(&decoded_ix_data)?;
            let fee_info = Self::get_fee_info(&g_ix.involved_instructions);

            let (fee_referrer, fee_amount, fee_mint_is_in_mint) = if fee_rates.provider_fee_bps > 0
            {
                match fee_info {
                    Some(fee_info) if fee_rates.provider_fee_bps > 0 => {
                        let ta_data = txn
                            .get_account_data_after(&fee_info.destination)
                            .or_else(|_| txn.get_account_data_before(&fee_info.destination))
                            .ok()
                            .flatten();
                        if let Some(ta_data) = ta_data {
                            let mut reader = StepAccountReader::new(&ta_data);
                            let ta_mint = reader.read_next_pubkey()?.to_string();
                            if ta_mint != *in_mint && ta_mint != *out_mint {
                                bail!("Fee token account mint does not match in or out mint. This likely is not the fee ix!");
                            }
                            let fee_mint_is_in_mint = ta_mint == *in_mint;

                            // For us, should be `5ccy...`
                            // `from_parts` usage for const Decimals
                            let fee_referrer = reader.read_next_pubkey()?.to_string();
                            let fee_amount = Decimal::from(fee_info.amount);
                            // let fee_pct = Decimal::from(fee_rates.provider_fee_bps)
                            //     / Decimal::from_parts(10000, 0, 0, false, 0);
                            // let out_amount_dec = Decimal::from(out_amount);
                            // let estimated_fee = out_amount_dec
                            //     .checked_div(Decimal::ONE - fee_pct)
                            //     .and_then(|est_total| est_total.checked_sub(out_amount_dec))
                            //     .context("Could not calculate estimated fee")?;

                            if fee_amount.is_zero() {
                                (None, None, None)
                            } else {
                                // let pct_diff = estimated_fee
                                //     .checked_div(fee_amount)
                                //     .and_then(|ratio| ratio.checked_sub(Decimal::ONE))
                                //     .context("Could not calculate percentage difference")?;

                                // if pct_diff > Decimal::from_parts(20, 0, 0, false, 2) {
                                //     bail!("Percentage difference between estimated fee and actual fee is too large. This likely is not the fee ix! (Estimated: {}, Actual: {}, Diff: {})", estimated_fee, fee_amount, pct_diff);
                                // }

                                (
                                    Some(fee_referrer),
                                    Some(fee_amount),
                                    Some(fee_mint_is_in_mint),
                                )
                            }
                        } else {
                            (None, None, None)
                        }
                    }
                    None if fee_rates.provider_fee_bps > 0 => {
                        bail!("Fee bps specified, but could not find fee transfer ix. Did Titan change their order of instructions?");
                    }
                    _ => (None, None, None),
                }
            } else {
                (None, None, None)
            };

            let swap_dooot = Dooot::SwapEvent(SwapEventDooot {
                time: txn.get_block_time(),
                program_pubkey: TITAN_SWAPS_PROGRAM_ID.into(),
                swap_auth_pubkey: auth.clone(),
                in_mint_pubkey: in_mint.clone(),
                out_mint_pubkey: out_mint.clone(),
                in_amount: Decimal::from(in_amount as u64),
                out_amount: Decimal::from(out_amount as u64),
                referring_platform_account_pubkey: fee_referrer,
                platform_referral_fee: fee_amount,
                fee_mint_is_in_mint,
                parent_amm_program_pubkey: None,
                market_pubkey: None,
                fees: vec![],
                slippage_bps: None,
                tx_sig: txn.get_signature().to_string(),
            });

            dooots.push(swap_dooot);
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct TitanFeeRates {
    pub provider_fee_bps: u16,
    pub service_fee_bps: u16,
}

#[derive(Debug)]
pub struct TitanFeeInfo {
    pub amount: u64,
    /// Destination will have the mint.
    ///
    /// Using this, we don't have to check for `ExactOut` or `ExactIn`
    pub destination: String,
}

impl TitanSwaps {
    #[inline]
    pub fn get_fee_rates(ix_data: &[u8]) -> Result<TitanFeeRates> {
        let mut reader = StepAccountReader::new(ix_data);
        reader.advance_unclamped(8 + 8 + 8 + 1);
        let provider_fee_bps = reader.read_next_le_u16()?;
        let service_fee_bps = reader.read_next_le_u16()?;
        Ok(TitanFeeRates {
            provider_fee_bps,
            service_fee_bps,
        })
    }

    #[inline]
    pub fn get_fee_info(ixs: &[FlattenedIxWithIndex]) -> Option<TitanFeeInfo> {
        let last_ix = ixs.last()?;

        let i = last_ix
            .get_parsed_token_program_instruction()
            .ok()
            .flatten()?;

        let destination;
        let amount;
        match i {
            TokenParsedInstruction::TokenTransfer(ix) => {
                destination = ix.destination;
                amount = ix.amount;
            }
            TokenParsedInstruction::TokenTransferChecked(ix) => {
                destination = ix.destination;
                amount = ix.token_amount.amount.parse().ok()?;
            }
            _ => {
                return None;
            }
        }

        Some(TitanFeeInfo {
            amount,
            destination,
        })
    }
}

#[cfg(test)]
mod tests {
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;

    use super::*;

    #[tokio::test]
    async fn titan_swap_with_fees() {
        let txn = DecodedTransaction::load_rabbit_message_from_file(
            "./tests/titan/titan_swap_with_fees.json",
        )
        .unwrap();

        let mut dooots = Vec::new();

        TitanSwaps
            .invoke(&txn, &mut dooots, &StepUtils::default())
            .await
            .unwrap();

        let Dooot::SwapEvent(swap) = dooots.first().unwrap() else {
            panic!("Dooot is not a swap event");
        };

        assert_eq!(
            swap.swap_auth_pubkey,
            "qUeL7JzC52V1DvvPkqnMd74QjThWtSJY5G1PkKv1ur7"
        );
        assert_eq!(
            swap.in_mint_pubkey,
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
        );
        assert_eq!(
            swap.out_mint_pubkey,
            "So11111111111111111111111111111111111111112"
        );
        assert_eq!(
            swap.referring_platform_account_pubkey,
            Some("5cyyVVBMFmQg1dFXnQpe6iYx818eYtHZpEzuvegVQAmU".into())
        );
        assert_eq!(swap.platform_referral_fee, Some(Decimal::from(306)));
    }

    #[tokio::test]
    async fn titan_swap_with_fees_2() {
        let txn = DecodedTransaction::load_rabbit_message_from_file(
            "./tests/titan/swap_with_fees_2.json",
        )
        .unwrap();

        let mut dooots = Vec::new();

        TitanSwaps
            .invoke(&txn, &mut dooots, &StepUtils::default())
            .await
            .unwrap();

        let Dooot::SwapEvent(swap) = dooots.first().unwrap() else {
            panic!("Dooot is not a swap event");
        };

        assert_eq!(
            swap.swap_auth_pubkey,
            "qUeL7JzC52V1DvvPkqnMd74QjThWtSJY5G1PkKv1ur7"
        );
        assert_eq!(
            swap.in_mint_pubkey,
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
        );
        assert_eq!(
            swap.out_mint_pubkey,
            "So11111111111111111111111111111111111111112"
        );
        assert_eq!(
            swap.referring_platform_account_pubkey,
            Some("5cyyVVBMFmQg1dFXnQpe6iYx818eYtHZpEzuvegVQAmU".into())
        );
        assert_eq!(swap.platform_referral_fee, Some(Decimal::from(51)));
    }

    #[tokio::test]
    async fn titan_fee_diff_pct() {
        let txn = DecodedTransaction::load_rabbit_message_from_file(
            "./tests/titan/swap_with_fees_3.json",
        )
        .unwrap();

        let mut dooots = Vec::new();

        TitanSwaps
            .invoke(&txn, &mut dooots, &StepUtils::default())
            .await
            .unwrap();

        let Dooot::SwapEvent(_swap) = dooots.first().unwrap() else {
            panic!("Dooot is not a swap event");
        };
    }

    #[tokio::test]
    async fn titan_closed_fee_account() {
        let txn = DecodedTransaction::load_rabbit_message_from_file(
            "./tests/titan/closed_fee_account.json",
        )
        .unwrap();

        let mut dooots = Vec::new();

        TitanSwaps
            .invoke(&txn, &mut dooots, &StepUtils::default())
            .await
            .unwrap();

        let Dooot::SwapEvent(_swap) = dooots.first().unwrap() else {
            panic!("Dooot is not a swap event");
        };
    }
}
