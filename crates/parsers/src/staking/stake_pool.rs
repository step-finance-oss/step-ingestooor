use std::{str::FromStr, sync::mpsc::SyncSender};

use anyhow::{Context, Result};
use async_trait::async_trait;
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
use step_ingestooor_sdk::{
    dooot::{Dooot, MintUnderlyingsGlobalDooot},
    trait_defs::transaction::TransactionLike,
    utils::{step_utils::StepUtils, util_fns::convert_blocktime_to_micros},
};

use crate::state_source::{AccountProvider, StateSource};

pub struct StakePool;

const SANCTUM_SINGLE_VALIDATOR_STAKE_POOL_PROGRAM: &str =
    "SP12tWFxD9oJsVWNavTTBZvMbA6gkAmxtVgxdqvyvhY";
const STAKE_POOL_PROGRAM_ID: &str = "SPoo1Ku8WFXoNDMHPsrGSTSG1Y47rzgn41SLUNakuHy";

#[async_trait]
impl crate::TransactionParserTrait for StakePool {
    fn get_name(&self) -> &'static str {
        "Stake Pool"
    }
    fn get_filters(&self) -> Vec<crate::TransactionParserFilter> {
        vec![
            crate::TransactionParserFilter::ProgramId(
                SANCTUM_SINGLE_VALIDATOR_STAKE_POOL_PROGRAM.to_string(),
            ),
            crate::TransactionParserFilter::ProgramId(STAKE_POOL_PROGRAM_ID.to_string()),
        ]
    }
    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        //hold onto the accounts for later use
        let accounts = decoded_transaction.get_account_keys();
        //get all the stake pool ixs
        for ix in decoded_transaction.get_non_parsed_program_ixs(&[
            SANCTUM_SINGLE_VALIDATOR_STAKE_POOL_PROGRAM.to_string(),
            STAKE_POOL_PROGRAM_ID.to_string(),
        ]) {
            log::debug!("processing ix");
            //check if is writable
            let stake_account = &ix.accounts[0];
            let pa = accounts
                .iter()
                .find(|a| &a.pubkey == stake_account)
                .context("stake account not in keys")?;
            if !pa.writable {
                continue;
            }
            let pool_acct_data_before = decoded_transaction
                .get_account_data_before(stake_account)?
                //the pool might not be there in pre data, so we just default to empty array instead of None to make
                //later code simpler
                .unwrap_or_default();
            let pool_acct_data_after = decoded_transaction
                .get_account_data_after(stake_account)?
                .context("didn't find account data for stake account")?;

            let mint = Pubkey::new_from_array(pool_acct_data_after[162..194].try_into()?);
            let vault_after = u64::from_le_bytes(pool_acct_data_after[258..266].try_into()?);
            let supply_after = u64::from_le_bytes(pool_acct_data_after[266..274].try_into()?);

            if supply_after == 0 {
                log::debug!(
                    "skipping processing as supply_after is 0 for account {}",
                    stake_account
                );
                continue;
            }

            if pool_acct_data_before.is_empty() {
                let price_per_sol_after = vault_after as f64 / supply_after as f64;
                if price_per_sol_after.is_infinite() {
                    return Err(anyhow::anyhow!("price per sol is infinite/null"));
                }
                dooot.push(Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                    time: decoded_transaction.get_block_time(),
                    mint_pubkey: mint.to_string(),
                    platform_program_pubkey: ix.program_id.to_string(),
                    mints: vec!["So11111111111111111111111111111111111111112".to_string()],
                    mints_qty_per_one_parent: vec![price_per_sol_after],
                    total_underlying_amounts: vec![vault_after.into()],
                    discriminant_id: ix.program_id.to_string(),
                }));
            } else {
                let vault_before = u64::from_le_bytes(pool_acct_data_before[258..266].try_into()?);
                let supply_before = u64::from_le_bytes(pool_acct_data_before[266..274].try_into()?);

                let price_per_sol_before = vault_before as f64 / supply_before as f64;
                let price_per_sol_after = vault_after as f64 / supply_after as f64;

                if price_per_sol_after.is_infinite() {
                    return Err(anyhow::anyhow!("price per sol is infinite/null"));
                }

                if price_per_sol_before != price_per_sol_after {
                    dooot.push(Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                        time: decoded_transaction.get_block_time(),
                        mint_pubkey: mint.to_string(),
                        platform_program_pubkey: ix.program_id.to_string(),
                        mints: vec!["So11111111111111111111111111111111111111112".to_string()],
                        mints_qty_per_one_parent: vec![price_per_sol_after],
                        total_underlying_amounts: vec![vault_after.into()],
                        discriminant_id: ix.program_id.to_string(),
                    }));
                }
            }
        }
        Ok(())
    }
}

impl StateSource for StakePool {
    fn get_name(&self) -> &'static str {
        "StakePool"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        let program_ids: Vec<String> = vec![
            SANCTUM_SINGLE_VALIDATOR_STAKE_POOL_PROGRAM.into(),
            STAKE_POOL_PROGRAM_ID.into(),
        ];
        let timestamp = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        for program_id in program_ids {
            let accts = acct_provider.program_accounts(
                Pubkey::from_str(&program_id).expect("failed to parse program_id to pubkey"),
                Some(&[
                    0x01u8, 0xe5u8, 0x43u8, 0x29u8, 0x00u8, 0xe0u8, 0x47u8, 0x76u8,
                ]),
            );
            for acct in accts {
                let data = acct.account.data();
                if data.len() != 611 {
                    log::info!("account wrong size {}", acct.pubkey);
                    continue;
                }
                let mint = Pubkey::new_from_array(data[162..194].try_into()?);
                let vault = u64::from_le_bytes(data[258..266].try_into()?);
                let supply = u64::from_le_bytes(data[266..274].try_into()?);

                let price_per_sol = vault as f64 / supply as f64;

                if price_per_sol.is_infinite() {
                    log::error!("price per sol is infinite/null");
                    continue;
                }

                tx.send(Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                    time: timestamp,
                    mint_pubkey: mint.to_string(),
                    platform_program_pubkey: program_id.to_string(),
                    mints: vec!["So11111111111111111111111111111111111111112".to_string()],
                    mints_qty_per_one_parent: vec![price_per_sol],
                    total_underlying_amounts: vec![vault.into()],
                    discriminant_id: acct.pubkey.to_string(),
                }))?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransactionParserTrait;
    use rust_decimal::Decimal;
    use std::{fs, rc::Rc};
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    #[tokio::test]
    async fn invoke_test() {
        let path = "./tests/stake_pool/sanctum_failed.json";
        let message_json = fs::read_to_string(path).unwrap();
        let decoded_txn: DecodedTransaction = serde_json::from_str(&message_json).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser: StakePool = StakePool;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
        println!("{:#?}", dooot);
        assert_eq!(dooot.len(), 1);

        let expected_mint_pubkey = "Dicscx2kpukGBATbjgsuzdbsVRFktV19BXEHofPQwEQF";
        let expected_platform_program_pubkey = "SP12tWFxD9oJsVWNavTTBZvMbA6gkAmxtVgxdqvyvhY";
        let expected_mints = vec!["So11111111111111111111111111111111111111112"];
        let expected_mints_qty_per_one_parent = vec![1.0];
        let expected_total_underlying_amounts = vec![Decimal::from(3282880)];
        let mint_underlying = &dooot[0];
        let mint_underlying = match mint_underlying {
            Dooot::MintUnderlyingsGlobal(mint_underlying) => mint_underlying,
            _ => panic!("Unexpected Dooot variant"),
        };
        assert_eq!(mint_underlying.mint_pubkey, expected_mint_pubkey);
        assert_eq!(
            mint_underlying.platform_program_pubkey,
            expected_platform_program_pubkey
        );
        assert_eq!(mint_underlying.mints, expected_mints);
        assert_eq!(
            mint_underlying.mints_qty_per_one_parent,
            expected_mints_qty_per_one_parent
        );
        assert_eq!(
            mint_underlying.total_underlying_amounts,
            expected_total_underlying_amounts
        );
    }
}
