use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;

use rust_decimal::Decimal;

use step_ingestooor_idl_gens::jup_v6_gen::jup_v6::{events::FeeEvent, utils::Event};

use solana_transaction_status::{UiInstruction, UiParsedInstruction};
use step_ingestooor_sdk::{
    dooot::{Dooot, SwapEventDooot},
    trait_defs::transaction::{FlattenedIxWithIndex, TransactionLike},
    utils::{
        step_account_reader::StepAccountReader, step_utils::StepUtils,
        util_fns::get_partially_decoded_variant,
    },
};

use crate::{TransactionParserFilter, TransactionParserTrait};

pub const JUP_V6_PROGRAM: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";
pub const STEP_SWAP_REFERRAL_PUBKEY: &str = "GmThisisAStepSwapThankYouForUsingStepGoodDay";

// global:route
pub const JUP_V6_ROUTE_DISC: &[u8] = &[0xe5, 0x17, 0xcb, 0x97, 0x7a, 0xe3, 0xad, 0x2a];
// global:route_with_token_ledger
pub const JUP_V6_ROUTE_WITH_TOKEN_LEDGER_DISC: &[u8] =
    &[0x96, 0x56, 0x47, 0x74, 0xa7, 0x5d, 0x0e, 0x68];
// global:shared_accounts_route
pub const JUP_V6_SHARED_ACCOUNTS_ROUTE_DISC: &[u8] =
    &[0xc1, 0x20, 0x9b, 0x33, 0x41, 0xd6, 0x9c, 0x81];
// global:shared_accounts_route_with_token_ledger
pub const JUP_V6_SHARED_ACCOUNTS_ROUTE_WITH_TOKEN_LEDGER_DISC: &[u8] =
    &[0xe6, 0x79, 0x8f, 0x50, 0x77, 0x9f, 0x6a, 0xaa];
// global:shared_accounts_exact_out_route
pub const JUP_V6_SHARED_ACCOUNTS_EXACT_OUT_ROUTE_DISC: &[u8] =
    &[0xb0, 0xd1, 0x69, 0xa8, 0x9a, 0x7d, 0x45, 0x3e];
// global:exact_out_route
pub const JUP_V6_EXACT_OUT_ROUTE_DISC: &[u8] = &[0xd0, 0x33, 0xef, 0x97, 0x7b, 0x2b, 0xed, 0x5c];

pub struct JupV6;

#[async_trait]
impl TransactionParserTrait for JupV6 {
    fn get_name(&self) -> &'static str {
        "Jupiter v6"
    }
    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            JUP_V6_PROGRAM.to_string(),
        )]
    }
    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let grouped_instructions =
            decoded_transaction.get_grouped_instructions(|ix| match ix.instruction {
                UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(ix)) => {
                    if ix.program_id != JUP_V6_PROGRAM {
                        return false;
                    }
                    let bytes = bs58::decode(&ix.data).into_vec();
                    if let Ok(bytes) = bytes {
                        let Some(disc_slice) = bytes.get(..8) else {
                            return false;
                        };

                        return matches!(
                            disc_slice,
                            JUP_V6_ROUTE_DISC
                                | JUP_V6_ROUTE_WITH_TOKEN_LEDGER_DISC
                                | JUP_V6_SHARED_ACCOUNTS_ROUTE_DISC
                                | JUP_V6_SHARED_ACCOUNTS_ROUTE_WITH_TOKEN_LEDGER_DISC
                                | JUP_V6_SHARED_ACCOUNTS_EXACT_OUT_ROUTE_DISC
                                | JUP_V6_EXACT_OUT_ROUTE_DISC
                        );
                    }
                    false
                }
                _ => false,
            })?;

        for grouped_ix in grouped_instructions {
            let inners = &grouped_ix.involved_instructions;
            let fee_events = Self::get_fee_events(inners)?;
            let swap_decoded_ix = get_partially_decoded_variant(
                grouped_ix.parent_instruction.instruction,
            )
            .ok_or(anyhow!(
                "Bad predicate for jup swaps. Did not get a PartiallyDecoded variant in tx {}",
                decoded_transaction.get_signature()
            ))?;
            let swap_instruction_data = &swap_decoded_ix.data;
            let swap_instruction_data = bs58::decode(swap_instruction_data).into_vec()?;
            let swap_ix_disc = &swap_instruction_data[..8];

            let transfer_signer: &String;
            let swapper: String;

            let in_mint: String;
            let out_mint: String;

            let mut referring_platform: String;
            let referral_fee: Option<Decimal>;
            let parent_amm_program: Option<String> = None; // None for now
            let slippage_bps: Option<u16>;
            let fee_mint_is_in_mint: Option<bool>;

            match swap_ix_disc {
                JUP_V6_ROUTE_DISC => {
                    transfer_signer = &swap_decoded_ix.accounts[1];
                    let destination_ata = &swap_decoded_ix.accounts[3];
                    let in_ata = &swap_decoded_ix.accounts[2];

                    let Ok(auth_ata_data) =
                        decoded_transaction.get_token_account_data_by_key(destination_ata)
                    else {
                        continue;
                    };

                    let Ok(in_ata_data) = decoded_transaction.get_token_account_data_by_key(in_ata)
                    else {
                        continue;
                    };

                    if auth_ata_data.owner.is_none() {
                        // Missing owner
                        return Err(anyhow::anyhow!(
                            "Missing owner for {} in {}",
                            destination_ata,
                            decoded_transaction.get_signature()
                        ));
                    }
                    slippage_bps = {
                        let mut reader = StepAccountReader::new(&swap_instruction_data);
                        reader.advance_to_end();
                        reader.rewind_clamped(3);
                        reader
                            .read_le_u16()
                            .context("Failed to read slippage bps")?
                            .into()
                    };
                    swapper = auth_ata_data.owner.context("Missing owner")?;
                    in_mint = in_ata_data.mint;
                    out_mint = swap_decoded_ix.accounts[5].clone();
                    referring_platform = swap_decoded_ix.accounts[6].clone();
                    let fee_event = fee_events
                        .iter()
                        .find(|e| e.account.to_string() == *referring_platform);
                    if let Some(fee_event) = fee_event {
                        referral_fee = Some(fee_event.amount.into());
                        fee_mint_is_in_mint = Some(fee_event.mint.to_string() == in_mint);
                    } else {
                        referral_fee = None;
                        fee_mint_is_in_mint = None;
                    }
                }
                JUP_V6_ROUTE_WITH_TOKEN_LEDGER_DISC => {
                    transfer_signer = &swap_decoded_ix.accounts[1];
                    let destination_ata = &swap_decoded_ix.accounts[3];
                    let in_ata = &swap_decoded_ix.accounts[2];
                    let Ok(auth_ata_data) =
                        decoded_transaction.get_token_account_data_by_key(destination_ata)
                    else {
                        continue;
                    };
                    let Ok(in_ata_data) = decoded_transaction.get_token_account_data_by_key(in_ata)
                    else {
                        continue;
                    };
                    if auth_ata_data.owner.is_none() {
                        // Missing owner
                        return Err(anyhow::anyhow!(
                            "Missing owner for {} in {}",
                            destination_ata,
                            decoded_transaction.get_signature()
                        ));
                    }
                    slippage_bps = {
                        let mut reader = StepAccountReader::new(&swap_instruction_data);
                        reader.advance_to_end();
                        reader.rewind_clamped(3);
                        reader
                            .read_le_u16()
                            .context("Failed to read slippage bps")?
                            .into()
                    };
                    swapper = auth_ata_data.owner.context("Missing owner")?;
                    in_mint = in_ata_data.mint;
                    out_mint = swap_decoded_ix.accounts[5].clone();
                    referring_platform = swap_decoded_ix.accounts[6].clone();
                    let fee_event = fee_events
                        .iter()
                        .find(|e| e.account.to_string() == *referring_platform);
                    if let Some(fee_event) = fee_event {
                        referral_fee = Some(fee_event.amount.into());
                        fee_mint_is_in_mint = Some(fee_event.mint.to_string() == in_mint);
                    } else {
                        referral_fee = None;
                        fee_mint_is_in_mint = None;
                    }
                }
                JUP_V6_SHARED_ACCOUNTS_ROUTE_DISC => {
                    transfer_signer = &swap_decoded_ix.accounts[2];
                    let destination_ata = &swap_decoded_ix.accounts[6];
                    let Ok(destination_acct_data) =
                        decoded_transaction.get_token_account_data_by_key(destination_ata)
                    else {
                        continue;
                    };
                    if destination_acct_data.owner.is_none() {
                        // Missing owner
                        return Err(anyhow::anyhow!(
                            "Missing owner for {} in {}",
                            destination_ata,
                            decoded_transaction.get_signature()
                        ));
                    }
                    slippage_bps = {
                        let mut reader = StepAccountReader::new(&swap_instruction_data);
                        reader.advance_to_end();
                        reader.rewind_clamped(3);
                        reader
                            .read_le_u16()
                            .context("Failed to read slippage bps")?
                            .into()
                    };
                    swapper = destination_acct_data.owner.context("Missing owner")?;
                    in_mint = swap_decoded_ix.accounts[7].clone();
                    out_mint = swap_decoded_ix.accounts[8].clone();
                    referring_platform = swap_decoded_ix.accounts[9].clone();
                    let fee_event = fee_events
                        .iter()
                        .find(|e| e.account.to_string() == *referring_platform);
                    if let Some(fee_event) = fee_event {
                        referral_fee = Some(fee_event.amount.into());
                        fee_mint_is_in_mint = Some(fee_event.mint.to_string() == in_mint);
                    } else {
                        referral_fee = None;
                        fee_mint_is_in_mint = None;
                    }
                }
                JUP_V6_SHARED_ACCOUNTS_ROUTE_WITH_TOKEN_LEDGER_DISC => {
                    transfer_signer = &swap_decoded_ix.accounts[2];
                    let destination_ata = &swap_decoded_ix.accounts[6];
                    let Ok(destination_acct_data) =
                        decoded_transaction.get_token_account_data_by_key(destination_ata)
                    else {
                        continue;
                    };
                    if destination_acct_data.owner.is_none() {
                        // Missing owner
                        return Err(anyhow::anyhow!(
                            "Missing owner for {} in {}",
                            destination_ata,
                            decoded_transaction.get_signature()
                        ));
                    }
                    slippage_bps = {
                        let mut reader = StepAccountReader::new(&swap_instruction_data);
                        reader.advance_to_end();
                        reader.rewind_clamped(3);
                        reader
                            .read_le_u16()
                            .context("Failed to read slippage bps")?
                            .into()
                    };
                    swapper = destination_acct_data.owner.context("Missing owner")?;
                    in_mint = swap_decoded_ix.accounts[7].clone();
                    out_mint = swap_decoded_ix.accounts[8].clone();
                    referring_platform = swap_decoded_ix.accounts[9].clone();
                    let fee_event = fee_events
                        .iter()
                        .find(|e| e.account.to_string() == *referring_platform);
                    if let Some(fee_event) = fee_event {
                        referral_fee = Some(fee_event.amount.into());
                        fee_mint_is_in_mint = Some(fee_event.mint.to_string() == in_mint);
                    } else {
                        referral_fee = None;
                        fee_mint_is_in_mint = None;
                    }
                }
                JUP_V6_EXACT_OUT_ROUTE_DISC => {
                    transfer_signer = &swap_decoded_ix.accounts[1];
                    let destination_ata = &swap_decoded_ix.accounts[3];
                    let Ok(destination_acct_data) =
                        decoded_transaction.get_token_account_data_by_key(destination_ata)
                    else {
                        continue;
                    };
                    if destination_acct_data.owner.is_none() {
                        // Missing owner
                        return Err(anyhow::anyhow!(
                            "Missing owner for {} in {}",
                            destination_ata,
                            decoded_transaction.get_signature()
                        ));
                    }
                    slippage_bps = {
                        let mut reader = StepAccountReader::new(&swap_instruction_data);
                        reader.advance_to_end();
                        reader.rewind_clamped(3);
                        reader
                            .read_le_u16()
                            .context("Failed to read slippage bps")?
                            .into()
                    };
                    swapper = destination_acct_data.owner.context("Missing owner")?;
                    in_mint = swap_decoded_ix.accounts[5].clone();
                    out_mint = swap_decoded_ix.accounts[6].clone();
                    referring_platform = swap_decoded_ix.accounts[7].clone();
                    let fee_event = fee_events
                        .iter()
                        .find(|e| e.account.to_string() == *referring_platform);
                    if let Some(fee_event) = fee_event {
                        referral_fee = Some(fee_event.amount.into());
                        fee_mint_is_in_mint = Some(fee_event.mint.to_string() == in_mint);
                    } else {
                        referral_fee = None;
                        fee_mint_is_in_mint = None;
                    }
                }
                JUP_V6_SHARED_ACCOUNTS_EXACT_OUT_ROUTE_DISC => {
                    transfer_signer = &swap_decoded_ix.accounts[2];
                    let destination_ata = &swap_decoded_ix.accounts[6];
                    let Ok(destination_acct_data) =
                        decoded_transaction.get_token_account_data_by_key(destination_ata)
                    else {
                        continue;
                    };
                    if destination_acct_data.owner.is_none() {
                        // Missing owner
                        return Err(anyhow::anyhow!(
                            "Missing owner for {} in {}",
                            destination_ata,
                            decoded_transaction.get_signature()
                        ));
                    }
                    slippage_bps = {
                        let mut reader = StepAccountReader::new(&swap_instruction_data);
                        reader.advance_to_end();
                        reader.rewind_clamped(3);
                        reader
                            .read_le_u16()
                            .context("Failed to read slippage bps")?
                            .into()
                    };
                    swapper = destination_acct_data.owner.context("Missing owner")?;
                    in_mint = swap_decoded_ix.accounts[7].clone();
                    out_mint = swap_decoded_ix.accounts[8].clone();
                    referring_platform = swap_decoded_ix.accounts[9].clone();
                    let fee_event = fee_events
                        .iter()
                        .find(|e| e.account.to_string() == *referring_platform);
                    if let Some(fee_event) = fee_event {
                        referral_fee = Some(fee_event.amount.into());
                        fee_mint_is_in_mint = Some(fee_event.mint.to_string() == in_mint);
                    } else {
                        referral_fee = None;
                        fee_mint_is_in_mint = None;
                    }
                }
                _ => {
                    // Unrelated jup ix
                    continue;
                }
            };

            let txfer_amounts = decoded_transaction
                .get_total_transfer_amounts_for_pubkey_flattened_ixs(
                    grouped_ix.involved_instructions.iter(),
                    &[transfer_signer.as_str()],
                    &[&in_mint],
                    &[&out_mint],
                )?;

            let in_amount = txfer_amounts
                .get_in_amount_u64(&in_mint, transfer_signer)
                .into();
            let out_amount = txfer_amounts
                .get_out_amount_u64(&out_mint, transfer_signer)
                .into();

            let step_ref_exists = decoded_transaction
                .get_account_keys()
                .iter()
                .any(|key| key.pubkey == STEP_SWAP_REFERRAL_PUBKEY);

            if step_ref_exists {
                referring_platform = STEP_SWAP_REFERRAL_PUBKEY.to_string();
            }

            let swap_dooot = Dooot::SwapEvent(SwapEventDooot {
                in_amount,
                in_mint_pubkey: in_mint,
                out_amount,
                fee_mint_is_in_mint,
                out_mint_pubkey: out_mint,
                parent_amm_program_pubkey: parent_amm_program,
                program_pubkey: JUP_V6_PROGRAM.to_string(),
                platform_referral_fee: referral_fee,
                referring_platform_account_pubkey: Some(referring_platform),
                swap_auth_pubkey: swapper,
                time: decoded_transaction.get_block_time(),
                market_pubkey: None,
                fees: vec![],
                slippage_bps,
                tx_sig: decoded_transaction.get_signature().to_string(),
            });

            dooot.push(swap_dooot);
        }

        Ok(())
    }
}

impl JupV6 {
    pub fn get_fee_events(ixs: &[FlattenedIxWithIndex]) -> Result<Vec<FeeEvent>> {
        ixs.iter()
            .filter_map(|ix| {
                let ix = get_partially_decoded_variant(ix.instruction)?;

                if ix.program_id != JUP_V6_PROGRAM {
                    return None;
                }

                let Ok(decoded) = bs58::decode(&ix.data).into_vec() else {
                    return Some(Err(anyhow::anyhow!(
                        "Invalid base58 in an instruction: {:?}",
                        ix
                    )));
                };

                let Ok(Event::FeeEvent(event)) = Event::try_from_slice(&decoded[8..]) else {
                    return None;
                };

                Some(Ok(event))
            })
            .collect::<Result<Vec<FeeEvent>>>()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, rc::Rc};

    use serde_json::Value;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;

    use step_ingestooor_sdk::utils::test_utils::init_test_extras_bare;

    use super::*;

    #[tokio::test]
    async fn parser_invoke() {
        // Load test txn json
        let message_json = fs::read_to_string("./tests/jupiter/missing_ata.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = JupV6;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooot.len(), 2);

        let expected_swapper = "CsWqRAhVyvQKbi8mPbkskLwBEgtsgZ4GUtmRd8B9DFEo";

        let Dooot::SwapEvent(route_ix) = &dooot[0] else {
            panic!("Expected a swap dooot");
        };
        // First ix was WSOL -> WIF
        let expected_in_amount = Decimal::from(1000000); // WSOL
        let expected_out_amount = Decimal::from(77399); // WIF
        let expected_in_mint = "So11111111111111111111111111111111111111112";
        let expected_out_mint = "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm";

        let actual_in_amount = route_ix.in_amount;
        let actual_out_amount = route_ix.out_amount;
        let actual_in_mint = &route_ix.in_mint_pubkey;
        let actual_out_mint = &route_ix.out_mint_pubkey;

        assert_eq!(expected_in_amount, actual_in_amount);
        assert_eq!(expected_out_amount, actual_out_amount);
        assert_eq!(expected_in_mint, actual_in_mint);
        assert_eq!(expected_out_mint, actual_out_mint);
        assert_eq!(expected_swapper, &route_ix.swap_auth_pubkey);

        let Dooot::SwapEvent(route_with_tl_ix) = &dooot[1] else {
            panic!("Expected a swap dooot");
        };
        // Second ix was WIF -> WSOL
        let expected_in_amount = Decimal::from(77399); // WIF
        let expected_out_amount = Decimal::from(1001314); // WSOL
        let expected_in_mint = "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm";
        let expected_out_mint = "So11111111111111111111111111111111111111112";

        let actual_in_amount = route_with_tl_ix.in_amount;
        let actual_out_amount = route_with_tl_ix.out_amount;
        let actual_in_mint = &route_with_tl_ix.in_mint_pubkey;
        let actual_out_mint = &route_with_tl_ix.out_mint_pubkey;

        assert_eq!(expected_in_amount, actual_in_amount);
        assert_eq!(expected_out_amount, actual_out_amount);
        assert_eq!(expected_in_mint, actual_in_mint);
        assert_eq!(expected_out_mint, actual_out_mint);
        assert_eq!(expected_swapper, &route_with_tl_ix.swap_auth_pubkey);
    }

    #[tokio::test]
    async fn missing_ata_2() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_s = fs::read_to_string("./tests/jupiter/missing_ata2.json").unwrap();
        let txn =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&txn_s).unwrap())
                .unwrap();

        let parser = JupV6;
        let mut dooot = vec![];
        parser.invoke(&txn, &mut dooot, &utils).await.unwrap();
    }

    #[tokio::test]
    async fn no_dooot() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_s = fs::read_to_string("./tests/jupiter/no-dooot.json").unwrap();
        let txn =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&txn_s).unwrap())
                .unwrap();

        let parser = JupV6;
        let mut dooot = vec![];
        parser.invoke(&txn, &mut dooot, &utils).await.unwrap();

        let dooot = dooot.first().unwrap();
        let Dooot::SwapEvent(route_ix) = dooot else {
            panic!("Expected a swap dooot");
        };

        assert_eq!(
            route_ix.swap_auth_pubkey,
            "qUeL7JzC52V1DvvPkqnMd74QjThWtSJY5G1PkKv1ur7"
        );
    }
}
