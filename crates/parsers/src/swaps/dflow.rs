use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use rust_decimal::Decimal;
use solana_sdk::pubkey::Pubkey;
use step_ingestooor_idl_gens::dflow_gen::dflow::{
    events::{FeeEvent, SwapEvent},
    utils::Event,
};
use step_ingestooor_sdk::{
    dooot::{Dooot, SwapEventDooot},
    trait_defs::transaction::{FlattenedIxWithIndex, TransactionLike},
    utils::{
        step_account_reader::StepAccountReader, step_utils::StepUtils,
        util_fns::get_partially_decoded_variant,
    },
};

use crate::{TransactionParserFilter, TransactionParserTrait};

pub const DFLOW_PROGRAM_ID: &str = "DF1ow4tspfHX9JwWJsAb9epbkA8hmpSEAtxXy1V27QBH";
//swap
pub const DFLOW_SWAP_DISC: &[u8] = &[248, 198, 158, 145, 225, 117, 135, 200];
//swap2
pub const DFLOW_SWAP2_DISC: &[u8] = &[65, 75, 63, 76, 235, 91, 91, 136];
//swap2_with_destination
pub const DFLOW_SWAP2_WITH_DEST_DISC: &[u8] = &[95, 123, 213, 246, 122, 1, 86, 231];
//swap2_with_destination_native
pub const DFLOW_SWAP2_WITH_DEST_NATIVE_DISC: &[u8] = &[222, 100, 184, 146, 186, 196, 105, 165];
//swap_with_destination
pub const DFLOW_SWAP_WITH_DEST_DISC: &[u8] = &[168, 172, 24, 77, 197, 156, 135, 101];
//swap_with_destination_native
pub const DFLOW_SWAP_WITH_DEST_NATIVE_DISC: &[u8] = &[205, 77, 127, 108, 241, 32, 196, 195];
pub const DFLOW_EVENT_IX_DISC: &[u8] = &[0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d];

pub struct DFlowSwaps;

#[async_trait]
impl TransactionParserTrait for DFlowSwaps {
    fn get_name(&self) -> &'static str {
        "DFlowSwaps"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            DFLOW_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let grouped_ixs = txn.get_grouped_instructions(|ix| {
            let pid = ix.get_ix_program_id(txn);
            if pid != DFLOW_PROGRAM_ID {
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

            matches!(
                disc,
                DFLOW_SWAP_DISC
                    | DFLOW_SWAP2_DISC
                    | DFLOW_SWAP2_WITH_DEST_DISC
                    | DFLOW_SWAP2_WITH_DEST_NATIVE_DISC
                    | DFLOW_SWAP_WITH_DEST_DISC
                    | DFLOW_SWAP_WITH_DEST_NATIVE_DISC
            )
        })?;

        for g_ix in grouped_ixs {
            let Some(parent_ix) =
                get_partially_decoded_variant(g_ix.parent_instruction.instruction)
            else {
                continue;
            };

            let dec = bs58::decode(&parent_ix.data).into_vec()?;
            let disc = dec.get(0..8).context("Invalid instruction data")?;

            let auth = match disc {
                DFLOW_SWAP_DISC
                | DFLOW_SWAP2_DISC
                | DFLOW_SWAP2_WITH_DEST_DISC
                | DFLOW_SWAP2_WITH_DEST_NATIVE_DISC
                | DFLOW_SWAP_WITH_DEST_DISC
                | DFLOW_SWAP_WITH_DEST_NATIVE_DISC => &parent_ix.accounts[3],
                _ => continue,
            };

            let event_info = Self::get_event_info(&g_ix.involved_instructions)?;

            let swap_event = match event_info.get_in_and_out_mints() {
                Ok(swap_event) => swap_event,
                Err(DFlowError::NoSwapEventsFound) => continue,
                Err(DFlowError::Other(e)) => bail!(e),
            };
            let in_mint = swap_event.0.to_string();
            let out_mint = swap_event.1.to_string();

            if in_mint == out_mint {
                // Arb, we don't care rn
                continue;
            }

            let transfer_amounts = txn.get_total_transfer_amounts_for_pubkey_flattened_ixs(
                g_ix.involved_instructions.iter(),
                &[auth],
                &[&in_mint],
                &[&out_mint],
            )?;

            let in_amount = transfer_amounts.get_in_amount_i128(&in_mint, auth);
            let out_amount = transfer_amounts.get_out_amount_i128(&out_mint, auth);

            if in_amount < 0 || out_amount < 0 {
                continue;
            }

            let (referring_platform_account_pubkey, platform_referral_fee, fee_mint_is_in_mint) =
                match event_info.fee_event {
                    Some(fee_event) => {
                        let fee_mint = fee_event.mint.to_string();
                        let fee_amount = fee_event.amount;
                        let fee_ta = fee_event.account.to_string();
                        let fee_owner = {
                            let data = txn
                                .get_account_data_after(&fee_ta)?
                                .context("Fee token account should still exist after swap")?;
                            let mut reader = StepAccountReader::new(&data);
                            reader.advance_unclamped(32);
                            reader.read_next_pubkey()?.to_string()
                        };
                        let fee_mint_is_in_mint = fee_mint == in_mint;

                        (
                            Some(fee_owner),
                            Some(Decimal::from(fee_amount)),
                            Some(fee_mint_is_in_mint),
                        )
                    }
                    None => (None, None, None),
                };

            let swap_dooot = Dooot::SwapEvent(SwapEventDooot {
                time: txn.get_block_time(),
                program_pubkey: DFLOW_PROGRAM_ID.to_string(),
                swap_auth_pubkey: auth.to_string(),
                in_mint_pubkey: in_mint,
                out_mint_pubkey: out_mint,
                in_amount: Decimal::from(in_amount as u64),
                out_amount: Decimal::from(out_amount as u64),
                fee_mint_is_in_mint,
                referring_platform_account_pubkey,
                platform_referral_fee,
                parent_amm_program_pubkey: None,
                market_pubkey: None,
                fees: vec![],
                slippage_bps: None,
                tx_sig: txn.get_signature().to_string(),
            });

            dooot.push(swap_dooot);
        }

        Ok(())
    }
}

pub enum DFlowError {
    NoSwapEventsFound,
    Other(anyhow::Error),
}

pub struct DFlowEventInfo {
    pub swap_events: Vec<SwapEvent>,
    pub fee_event: Option<FeeEvent>,
}

impl DFlowEventInfo {
    pub fn get_in_and_out_mints(&self) -> Result<(Pubkey, Pubkey), DFlowError> {
        if self.swap_events.is_empty() {
            return Err(DFlowError::NoSwapEventsFound);
        }

        let first_event = self
            .swap_events
            .first()
            .context("UNREACHABLE - swap_events is empty, but we checked above")
            .map_err(DFlowError::Other)?;
        let last_event = self
            .swap_events
            .last()
            .context("UNREACHABLE - swap_events is empty, but we checked above")
            .map_err(DFlowError::Other)?;

        let in_mint = first_event.input_mint;
        let out_mint = last_event.output_mint;

        Ok((in_mint, out_mint))
    }
}

impl DFlowSwaps {
    pub fn get_event_info(ixs: &[FlattenedIxWithIndex]) -> Result<DFlowEventInfo> {
        let mut swap_events: Vec<SwapEvent> = Vec::new();
        let mut fee_event: Option<FeeEvent> = None;

        for ix in ixs {
            let Some(ix) = get_partially_decoded_variant(ix.instruction) else {
                continue;
            };

            if ix.program_id != DFLOW_PROGRAM_ID {
                continue;
            }

            let dec = bs58::decode(&ix.data).into_vec()?;

            let disc = dec.get(0..8).context("Invalid slice")?;

            if matches!(disc, DFLOW_EVENT_IX_DISC) {
                let data_slice = dec.get(8..).context("Invalid slice")?;
                let event = Event::try_from_slice(data_slice).context("Event deser error")?;
                match event {
                    Event::SwapEvent(event) => {
                        swap_events.push(event);
                    }
                    Event::FeeEvent(event) => {
                        match fee_event.as_mut() {
                            Some(fee_event) => {
                                if fee_event.mint != event.mint
                                    || fee_event.account != event.account
                                {
                                    // Appears to be the same mint & account rn
                                    bail!("Multiple fee events with DIFFERENT mints or accounts!! BAD!");
                                }

                                fee_event.amount += event.amount;
                            }
                            None => {
                                fee_event.replace(event);
                            }
                        }
                    }
                }
            }
        }

        Ok(DFlowEventInfo {
            swap_events,
            fee_event,
        })
    }
}

#[cfg(test)]
mod tests {
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;

    use super::*;

    #[tokio::test]
    async fn swap_with_fee() {
        let txn =
            DecodedTransaction::load_rabbit_message_from_file("./tests/dflow/swap_with_fee.json")
                .unwrap();

        let mut dooots = Vec::new();

        DFlowSwaps
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
        assert_eq!(swap.in_amount, Decimal::from(1000));
        assert_eq!(
            swap.referring_platform_account_pubkey,
            Some("5cyyVVBMFmQg1dFXnQpe6iYx818eYtHZpEzuvegVQAmU".into())
        );
        assert_eq!(swap.platform_referral_fee, Some(Decimal::from(32)));
        assert!(swap.fee_mint_is_in_mint.is_some_and(|x| !x));
    }
}
