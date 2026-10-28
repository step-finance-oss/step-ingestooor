use anyhow::{Context, Result};
use async_trait::async_trait;
use solana_transaction_status::{UiInstruction, UiParsedInstruction};
use step_ingestooor_sdk::{
    dooot::{Dooot, SwapEventDooot},
    trait_defs::transaction::TransactionLike,
    utils::step_utils::StepUtils,
};

use crate::{TransactionParserFilter, TransactionParserTrait};

pub const OKX_SWAP_PROGRAM: &str = "6m2CDdhRgxpH4WjvdzxAYbGxwdGUz5MziiL5jek2kBma";
pub const STEP_ORDER_ID: u64 = 109449u64;

pub struct OkxSwap;

#[async_trait]
impl TransactionParserTrait for OkxSwap {
    fn get_name(&self) -> &'static str {
        "OKX Swap"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            OKX_SWAP_PROGRAM.to_string(),
        )]
    }

    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let tree = decoded_transaction.get_ix_tree()?;

        // Find OKX swap instructions
        let okx_nodes = tree
            .root()
            .context("UNREACHABLE - Tree should have a root")?
            .traverse_level_order()
            .filter_map(|node| {
                let node_data = node.data().as_ref()?;
                let UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(ix)) = node_data
                else {
                    return None;
                };
                if ix.program_id != OKX_SWAP_PROGRAM {
                    return None;
                }
                Some((node, ix))
            });

        for (node, ix) in okx_nodes {
            // Get the first 5 accounts which are required for OKX swaps
            if ix.accounts.len() < 5 {
                // log::warn!("OKX swap instruction has less than 5 accounts");
                continue;
            }

            let payer = &ix.accounts[0];
            let source_mint = &ix.accounts[3];
            let destination_mint = &ix.accounts[4];

            // Get all instructions under this swap instruction
            let involved_instructions = node
                .traverse_pre_order()
                .filter_map(|n| n.data().as_ref().map(|d| *d));

            // Get transfer amounts
            let txfer_amounts = decoded_transaction.get_total_transfer_amounts_for_pubkey_ui_ixs(
                involved_instructions,
                &[payer],
                &[source_mint],
                &[destination_mint],
            )?;

            let in_amount = txfer_amounts.get_in_amount_u64(source_mint, payer).into();
            let out_amount = txfer_amounts
                .get_out_amount_u64(destination_mint, payer)
                .into();

            let ix_bytes = bs58::decode(&ix.data).into_vec()?;
            //okx has an order id for every api key used. one of these is steps (see unused constant)
            let order_bytes = ix_bytes.split_last_chunk();
            let order_bytes: &[u8; 8] = order_bytes.context("okx: Expected 8 bytes")?.1;
            //we make a fake referrer pubkey by turning the order id into a pubkey - 0 padded
            let referrer_pubkey = [0; 24].into_iter().chain(*order_bytes).collect::<Vec<u8>>();
            let referrer_pubkey = bs58::encode(&referrer_pubkey).into_string();

            // Create swap dooot
            let swap_dooot = SwapEventDooot {
                time: decoded_transaction.get_block_time(),
                program_pubkey: OKX_SWAP_PROGRAM.to_string(),
                swap_auth_pubkey: payer.to_string(),
                in_mint_pubkey: source_mint.to_string(),
                out_mint_pubkey: destination_mint.to_string(),
                in_amount,
                out_amount,
                fee_mint_is_in_mint: Some(false),
                referring_platform_account_pubkey: Some(referrer_pubkey),
                platform_referral_fee: None,
                parent_amm_program_pubkey: None,
                market_pubkey: None,
                fees: vec![],
                slippage_bps: None,
                tx_sig: decoded_transaction.get_signature().to_string(),
            };

            dooot.push(Dooot::SwapEvent(swap_dooot));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::{fs, rc::Rc};
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;

    #[tokio::test]
    async fn test_okx_swap_1() {
        let step_utils = Rc::new(StepUtils::new(None));

        // Load test transaction
        let message_json = fs::read_to_string("./tests/okx/sample1.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();

        let mut dooot = vec![];
        let parser = OkxSwap;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooot.len(), 1, "Expected exactly one swap dooot");

        let Dooot::SwapEvent(swap) = &dooot[0] else {
            panic!("Expected a swap dooot");
        };

        // Check token changes
        let expected_in_amount: rust_decimal::Decimal = 41751809633u64.into();
        let expected_out_amount: rust_decimal::Decimal = 436453422u64.into();

        assert_eq!(
            swap.swap_auth_pubkey,
            "M7TfAbuhU5J2KvKkUtPAse1sU7bBNvb4vC1a5DHsKK7"
        );
        assert_eq!(
            swap.in_mint_pubkey,
            "FS4xcBxLJbrrdXE1R6zHvKw8no4zrQn2rRFuczvepump"
        );
        assert_eq!(
            swap.in_amount, expected_in_amount,
            "Incorrect source token change"
        );
        assert_eq!(
            swap.out_mint_pubkey,
            "So11111111111111111111111111111111111111112"
        );
        assert_eq!(
            swap.out_amount, expected_out_amount,
            "Incorrect destination token change"
        );
        assert_eq!(
            swap.referring_platform_account_pubkey,
            Some("111111111111111111111111jRFg29E6TqZ".to_string())
        );
    }

    #[tokio::test]
    async fn test_okx_swap_2() {
        let step_utils = Rc::new(StepUtils::new(None));

        // Load test transaction
        let message_json = fs::read_to_string("./tests/okx/sample2.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();

        let mut dooot = vec![];
        let parser = OkxSwap;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooot.len(), 1, "Expected exactly one swap dooot");

        let Dooot::SwapEvent(swap) = &dooot[0] else {
            panic!("Expected a swap dooot");
        };

        // Check token changes
        let expected_in_amount: rust_decimal::Decimal = 1050000000u64.into();
        let expected_out_amount: rust_decimal::Decimal = 20041232381u64.into();

        assert_eq!(
            swap.swap_auth_pubkey,
            "4gmdJCXcaRwtR7zePTmTvVRTTRpqxhzEKMrxduUYFapX"
        );
        assert_eq!(
            swap.in_mint_pubkey,
            "So11111111111111111111111111111111111111112"
        );
        assert_eq!(
            swap.in_amount, expected_in_amount,
            "Incorrect source token change"
        );
        assert_eq!(
            swap.out_mint_pubkey,
            "2RBko3xoz56aH69isQMUpzZd9NYHahhwC23A5F3Spkin"
        );
        assert_eq!(
            swap.out_amount, expected_out_amount,
            "Incorrect destination token change"
        );
        assert_eq!(
            swap.referring_platform_account_pubkey,
            Some("11111111111111111111111111111111".to_string())
        );
    }

    #[tokio::test]
    async fn test_okx_swap_3() {
        let step_utils = Rc::new(StepUtils::new(None));

        // Load test transaction
        let message_json = fs::read_to_string("./tests/okx/sample3.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();

        let mut dooot = vec![];
        let parser = OkxSwap;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooot.len(), 1, "Expected exactly one swap dooot");

        let Dooot::SwapEvent(swap) = &dooot[0] else {
            panic!("Expected a swap dooot");
        };

        // Check token changes
        let expected_in_amount: rust_decimal::Decimal = 3322833874u64.into();
        let expected_out_amount: rust_decimal::Decimal = 1246943648773u64.into();

        assert_eq!(
            swap.swap_auth_pubkey,
            "9QAymzxyavhZxHKZQ9fFQjS72xdgvbaLS3vMWPRJ5fmR"
        );
        assert_eq!(
            swap.in_mint_pubkey,
            "8QhSMvYfXome11VgxFMD75hNbGQXW5QTnjA8khENkY2c"
        );
        assert_eq!(
            swap.in_amount, expected_in_amount,
            "Incorrect source token change"
        );
        assert_eq!(
            swap.out_mint_pubkey,
            "9P2TZKERwSTPjvpdQ5RDF2d1kD31Jg8uQWUMYh6jpump"
        );
        assert_eq!(
            swap.out_amount, expected_out_amount,
            "Incorrect destination token change"
        );
        assert_eq!(
            swap.referring_platform_account_pubkey,
            Some("111111111111111111111111jRFg29E6TqZ".to_string())
        );
    }
}
