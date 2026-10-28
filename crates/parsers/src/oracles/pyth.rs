use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::{Decimal, MathematicalOps};
use step_ingestooor_sdk::{
    dooot::{Dooot, OraclePriceEventDooot},
    trait_defs::transaction::TransactionLike,
    utils::{step_account_reader::StepAccountReader, step_utils::StepUtils},
};

use crate::{TransactionParserFilter, TransactionParserTrait};

pub const PYTH_READER_PROGRAM_ID: &str = "rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ";
pub const PRICE_UPDATE_ACCT_DISCRIMINATOR: &[u8] =
    &[0x22, 0xf1, 0x23, 0x63, 0x9d, 0x7e, 0xf4, 0xcd];

pub struct PythReader;

#[async_trait]
impl TransactionParserTrait for PythReader {
    fn get_name(&self) -> &'static str {
        "PythReader"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            PYTH_READER_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let acct_iter = txn
            .iter_accounts()
            .filter(|a| a.post_owner_program == Some(PYTH_READER_PROGRAM_ID));
        for acct in acct_iter {
            let Some(post) = acct.unpack_post_datum()? else {
                continue;
            };
            let pre = acct.unpack_pre_datum()?;

            let disc = &post[..8];
            if disc != PRICE_UPDATE_ACCT_DISCRIMINATOR {
                continue;
            }

            let post_price_data = Self::get_price_data_from_account(&post)?;
            let pre_price_data = pre
                .map(|p| Self::get_price_data_from_account(&p))
                .transpose()?;

            let price_changed = if let Some(pre_price_data) = pre_price_data {
                (post_price_data.price_int != pre_price_data.price_int)
                    || (post_price_data.exponent != pre_price_data.exponent)
            } else {
                true
            };

            if !price_changed {
                continue;
            }

            // log::warn!("Getting price for Pyth account: {:?}", acct.pubkey);
            // log::warn!("Data: {:#04X?}", &post);
            let Some(price) = post_price_data.to_decimal_price() else {
                continue;
            };

            let confidence = Some(post_price_data.confidence_int);

            let oracle_price_dooot = Dooot::OraclePriceEvent(OraclePriceEventDooot {
                time,
                price,
                confidence,
                feed_account_pubkey: acct.pubkey.to_string(),
                feed_id: post_price_data.feed_id,
                mint: None,
                exponent: Some(post_price_data.exponent),
            });

            dooot.push(oracle_price_dooot);
        }

        Ok(())
    }
}

impl PythReader {
    fn get_price_data_from_account(account: &[u8]) -> Result<ExtractedPriceData> {
        let mut reader = StepAccountReader::new(account);
        // See https://github.com/pyth-network/pyth-crosschain/blob/28cf8af7e5c616e10fa2d6d1e289e6fc4ab06e48/target_chains/solana/pyth_solana_receiver_sdk/src/price_update.rs#L52
        // And https://github.com/pyth-network/pyth-crosschain/blob/main/pythnet/pythnet_sdk/src/messages.rs#L93
        // Not sure why we need to subtract 1. GH repo is out of date with chain or something.
        reader.move_index_to(8 + 32 + 2 - 1);

        let feed_id = reader.read_next_pubkey()?;
        let price_int = reader.read_next_le_i64()?;
        let confidence_int = reader.read_next_le_u64()?;
        let exponent = reader.read_next_le_i32()?;

        let price_data = ExtractedPriceData {
            feed_id: feed_id.to_string(),
            price_int,
            confidence_int,
            exponent,
        };

        Ok(price_data)
    }
}

#[derive(Debug)]
struct ExtractedPriceData {
    pub feed_id: String,
    pub price_int: i64,
    pub confidence_int: u64,
    pub exponent: i32,
}

impl ExtractedPriceData {
    pub fn _to_f64_price(&self) -> f64 {
        self.price_int as f64 * 10.0_f64.powi(self.exponent)
    }

    pub fn to_decimal_price(&self) -> Option<Decimal> {
        let factor = Decimal::from(10).checked_powi(self.exponent as i64)?;
        let price = Decimal::from(self.price_int);

        Some(price * factor)
    }
}
