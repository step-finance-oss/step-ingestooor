use crate::state_source::{AccountProvider, StateSource};
use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rust_decimal::Decimal;
use solana_sdk::account::ReadableAccount;
use std::sync::mpsc::SyncSender;
use step_ingestooor_sdk::dooot::SolBalanceUserDooot;
use step_ingestooor_sdk::utils::step_utils::StepUtils;
use step_ingestooor_sdk::utils::util_fns::convert_blocktime_to_micros;
use step_ingestooor_sdk::{dooot::Dooot, trait_defs::transaction::TransactionLike};
pub struct SolBalance;
#[async_trait]
impl TransactionParserTrait for SolBalance {
    fn get_name(&self) -> &'static str {
        "SOL Balance"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::None]
    }

    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let account_keys = decoded_transaction.get_account_keys();
        let pre_balances = decoded_transaction.get_pre_balances();
        let post_balances = decoded_transaction.get_post_balances();

        for (index, a) in account_keys.iter().enumerate() {
            if !a.writable {
                continue;
            }
            let pre_balance = pre_balances.get(index).context("pre balance not found")?;
            let post_balance = post_balances.get(index).context("post balance not found")?;
            if pre_balance != post_balance {
                let owner_pubkey = a.pubkey.to_string();
                let balance_dooot = Dooot::SolBalanceUser(SolBalanceUserDooot {
                    time: decoded_transaction.get_block_time(),
                    account_pubkey: owner_pubkey,
                    balance: Decimal::from(*post_balance),
                });
                dooot.push(balance_dooot.clone());
            }
        }
        Ok(())
    }
}

impl StateSource for SolBalance {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        let time = acct_provider.get_block_time();
        let time_stamp = convert_blocktime_to_micros(time, Some(0), None)?;
        for a in acct_provider.all_accounts() {
            if a.account.lamports() == 0 {
                continue;
            }
            let account_pubkey = a.pubkey.to_string();
            let balance_dooot = Dooot::SolBalanceUser(SolBalanceUserDooot {
                time: time_stamp,
                account_pubkey,
                balance: Decimal::from(a.account.lamports()),
            });
            tx.send(balance_dooot)?;
        }

        Ok(())
    }
    fn get_name(&self) -> &'static str {
        "SOL Balance"
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, rc::Rc};

    use super::*;
    use serde_json::Value;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;

    #[tokio::test]
    async fn parser_invoke() {
        let message_json = fs::read_to_string("./tests/sol_balance/sol_balance.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = SolBalance;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        println!("dooot: {:#?}", dooot);

        let Dooot::SolBalanceUser(balance_dooot) = &dooot[0] else {
            panic!("Expected a sol balance dooot");
        };

        let expected_owner_pubkey = "HzuK5PCN6gi8gaKHZwRMhXS4sJiHyUFM3dtBHXLykVQU";
        let expected_balance = Decimal::from(1504951441);

        let actual_owner_pubkey = balance_dooot.account_pubkey.to_string();
        let actual_balance = balance_dooot.balance;

        assert_eq!(expected_owner_pubkey, actual_owner_pubkey);
        assert_eq!(expected_balance, actual_balance);

        let Dooot::SolBalanceUser(balance_dooot) = &dooot[1] else {
            panic!("Expected a sol balance dooot");
        };

        let expected_owner_pubkey = "CKttbFuPAN6pEnX3Wz5Fva2U23gWyq5h4ekb8dyncURX";
        let expected_balance = Decimal::from(130567965520u64);

        let actual_owner_pubkey = balance_dooot.account_pubkey.to_string();
        let actual_balance = balance_dooot.balance;

        assert_eq!(expected_owner_pubkey, actual_owner_pubkey);
        assert_eq!(expected_balance, actual_balance);

        let Dooot::SolBalanceUser(balance_dooot) = &dooot[2] else {
            panic!("Expected a sol balance dooot");
        };

        let expected_owner_pubkey = "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL";
        let expected_balance = Decimal::from(954046);

        let actual_owner_pubkey = balance_dooot.account_pubkey.to_string();
        let actual_balance = balance_dooot.balance;

        assert_eq!(expected_owner_pubkey, actual_owner_pubkey);
        assert_eq!(expected_balance, actual_balance);

        let Dooot::SolBalanceUser(balance_dooot) = &dooot[3] else {
            panic!("Expected a sol balance dooot");
        };

        let expected_owner_pubkey = "5PAmrLHaPnH95QqiCQ5x9Hn5MPGQZmQhKuL1kyS24r7G";
        let expected_balance = Decimal::from(72891868450u64);

        let actual_owner_pubkey = balance_dooot.account_pubkey.to_string();
        let actual_balance = balance_dooot.balance;

        assert_eq!(expected_owner_pubkey, actual_owner_pubkey);
        assert_eq!(expected_balance, actual_balance);

        let Dooot::SolBalanceUser(balance_dooot) = &dooot[4] else {
            panic!("Expected a sol balance dooot");
        };

        let expected_owner_pubkey = "AbcuyoPeYnddzFoFQudsiFka8qd6tTwvLgxwtpTKTpKC";
        let expected_balance = Decimal::from(1480573676862u64);

        let actual_owner_pubkey = balance_dooot.account_pubkey.to_string();
        let actual_balance = balance_dooot.balance;

        assert_eq!(expected_owner_pubkey, actual_owner_pubkey);
        assert_eq!(expected_balance, actual_balance);

        let Dooot::SolBalanceUser(balance_dooot) = &dooot[5] else {
            panic!("Expected a sol balance dooot");
        };

        let expected_owner_pubkey = "B6YvSdbUpH3NH7oWp9LM7rXCh4agtJQmwvyKpiYqqz5s";
        let expected_balance = Decimal::from(51355343737u64);

        let actual_owner_pubkey = balance_dooot.account_pubkey.to_string();
        let actual_balance = balance_dooot.balance;

        assert_eq!(expected_owner_pubkey, actual_owner_pubkey);
        assert_eq!(expected_balance, actual_balance);
    }
}
