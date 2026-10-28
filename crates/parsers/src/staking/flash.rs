use crate::state_source::AccountProvider;
use crate::state_source::StateSource;
use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::Context;
use anyhow::{Ok, Result};
use async_trait::async_trait;
use borsh::BorshDeserialize;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use solana_sdk::account::ReadableAccount;
use solana_sdk::pubkey::Pubkey;
use std::sync::mpsc::SyncSender;
use step_ingestooor_idl_gens::flash_trade_gen::flash_trade::accounts::Pool;
use step_ingestooor_sdk::{
    dooot::{Dooot, MintUnderlyingsGlobalDooot},
    trait_defs::transaction::TransactionLike,
    utils::{step_utils::StepUtils, util_fns::convert_blocktime_to_micros},
};

pub struct FlashTrade;

pub const FLASH_PROGRAM_ID: &str = "FLASH6Lo6h3iasJKWDs2F8TkW2UKf3s15C8PMGuVfgBn";
pub const FLASH_POOL_DISC: &[u8] = &[0xf1, 0x9a, 0x6d, 0x04, 0x11, 0xb1, 0x6d, 0xbc];

#[async_trait]
impl TransactionParserTrait for FlashTrade {
    fn get_name(&self) -> &'static str {
        "Flash Trade Staking"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::AccountId(
            FLASH_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> anyhow::Result<()> {
        let accounts = txn
            .iter_accounts()
            .filter(|a| a.post_owner_program == Some(FLASH_PROGRAM_ID) && a.datum_changed);

        for acct in accounts {
            let data = match acct.unpack_post_datum()? {
                Some(d) => d,
                None => {
                    log::warn!(
                        "FLASH TRADE: Missing post datum (could be closed) for account {}",
                        acct.pubkey
                    );
                    continue;
                }
            };
            if !data.starts_with(FLASH_POOL_DISC) {
                continue;
            }
            let pool = Pool::deserialize(&mut &data[8..])?;

            let active_amount = Decimal::from(pool.compounding_stats.active_amount);
            let total_supply = Decimal::from(pool.compounding_stats.total_supply);
            let per_price = if total_supply.is_zero() {
                Decimal::ZERO
            } else {
                active_amount / total_supply
            };
            let per_price = per_price.to_f64().context(format!(
                "Failed to convert per price to f64 got {:?}",
                per_price
            ))?;

            let dooot_underlying = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                time: txn.get_block_time(),
                mint_pubkey: pool.lp_mint.to_string(),
                platform_program_pubkey: FLASH_PROGRAM_ID.to_string(),
                mints: vec![pool.compounding_mint.to_string()],
                mints_qty_per_one_parent: vec![per_price],
                total_underlying_amounts: vec![total_supply],
                discriminant_id: acct.pubkey.to_string(),
            });
            dooot.push(dooot_underlying);
        }

        Ok(())
    }
}

impl StateSource for FlashTrade {
    fn get_name(&self) -> &'static str {
        "FlashTrade"
    }
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        let timestamp = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let accts = acct_provider.program_accounts(
            Pubkey::from_str(FLASH_PROGRAM_ID).expect("failed to parse program_id to pubkey"),
            Some(FLASH_POOL_DISC),
        );
        for acct in accts {
            let data = acct.account.data();
            if data.starts_with(FLASH_POOL_DISC) {
                let pool = Pool::deserialize(&mut &data[8..])?;
                let aum_usd = Decimal::from(pool.compounding_stats.active_amount);
                let total_supply = Decimal::from(pool.compounding_stats.total_supply);
                let per_price = if total_supply.is_zero() {
                    Decimal::ZERO
                } else {
                    aum_usd / total_supply
                };
                let per_price = per_price.to_f64().unwrap_or(0.0);
                let dooot_underlying = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                    time: timestamp,
                    mint_pubkey: pool.lp_mint.to_string(),
                    platform_program_pubkey: FLASH_PROGRAM_ID.to_string(),
                    mints: vec![pool.compounding_mint.to_string()],
                    mints_qty_per_one_parent: vec![per_price],
                    total_underlying_amounts: vec![total_supply],
                    discriminant_id: acct.pubkey.to_string(),
                });
                tx.send(dooot_underlying)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use step_ingestooor_sdk::utils::test_utils::init_test_extras_bare;

    use super::*;
    use std::{fs, rc::Rc};
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    use TransactionParserTrait;

    #[tokio::test]
    async fn invoke_test() {
        let path = "./tests/flash/txn_flash_staking_2AcSvRG5D681hE61gcnsHXNeXVj1i5m8pqeauoyaVmRN13P1GRWMQ68SXn4xZfPrRnr46qSJFZWcxH6J4UCoTZ31.json";
        let message_json = fs::read_to_string(path).unwrap();
        let decoded_txn: DecodedTransaction = serde_json::from_str(&message_json).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser: FlashTrade = FlashTrade;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 1);

        let dooot = dooot.first().unwrap();
        match dooot {
            Dooot::MintUnderlyingsGlobal(mint_underlying) => {
                assert_eq!(
                    mint_underlying.mint_pubkey,
                    "CrdMPbjooMmz6RoVgUnczWoeZka2QF14pikcCTpzRMxz".to_string()
                );
                assert_eq!(
                    mint_underlying.platform_program_pubkey,
                    FLASH_PROGRAM_ID.to_string()
                );
                assert_eq!(
                    mint_underlying.mints,
                    vec!["AbVzeRUss8QJYzv2WDizDJ2RtsD1jkVyRjNdAzX94JhG".to_string()]
                );
                assert_eq!(
                    mint_underlying.mints_qty_per_one_parent,
                    vec![1.0348844093504816]
                );
                assert_eq!(
                    mint_underlying.total_underlying_amounts,
                    vec![Decimal::from(314614038000u64)]
                );
            }
            _ => panic!("Expected MintUnderlyings"),
        }
    }

    #[tokio::test]
    async fn swap_tx() {
        let f_str = fs::read_to_string("./tests/flash/swap.json").unwrap();
        let f_json: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&f_str).unwrap())
                .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: FlashTrade = FlashTrade;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 1);
    }

    #[tokio::test]
    async fn swap_tx2() {
        let f_str = fs::read_to_string("./tests/flash/swap2.json").unwrap();
        let f_json: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&f_str).unwrap())
                .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: FlashTrade = FlashTrade;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 1);
        let dooot = dooot.first().unwrap();

        let Dooot::MintUnderlyingsGlobal(mint_underlying) = dooot else {
            panic!("Expected MintUnderlyings");
        };
        assert_eq!(
            mint_underlying.mints_qty_per_one_parent,
            vec![1.1829547471407411]
        );
    }
}
