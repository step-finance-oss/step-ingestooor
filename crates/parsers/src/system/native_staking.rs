use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::Result;
use async_trait::async_trait;
use log::debug;
use solana_account_decoder::parse_stake::{parse_stake, StakeAccountType};
use step_ingestooor_sdk::{
    dooot::{Dooot, NativeStakeEventDooot, NativeStakeEventType},
    trait_defs::transaction::TransactionLike,
    utils::{constants::STAKE_PROGRAM_ID, stake_ixs::*, step_utils::StepUtils},
};

pub struct NativeStaking;

#[async_trait]
impl TransactionParserTrait for NativeStaking {
    fn get_name(&self) -> &'static str {
        "Native Staking"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            STAKE_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let tx_id = decoded_transaction.get_signature();
        let block_time = decoded_transaction.get_block_time();
        for ix in decoded_transaction
            .get_flattened_ixs()
            .filter_map(|a| a.get_parsed_stake_program_instruction().transpose())
        {
            let ix = ix?;

            let event_type = match ix {
                StakeProgramParsedInstruction::Delegate(_)
                | StakeProgramParsedInstruction::Redelegate(_) => NativeStakeEventType::Delegate,
                StakeProgramParsedInstruction::Withdraw(_) => NativeStakeEventType::Withdraw,
                StakeProgramParsedInstruction::Deactivate(_)
                | StakeProgramParsedInstruction::DeactivateDelinquent(_) => {
                    NativeStakeEventType::Deactivate
                }
                _ => continue,
            };

            let Some(stake_account_pubkey) = ix.get_stake_account() else {
                continue;
            };
            let stake_account_pubkey = stake_account_pubkey.clone();
            // read the pre datum here so we can get the amounts
            if event_type == NativeStakeEventType::Withdraw
                || event_type == NativeStakeEventType::Deactivate
            {
                let pre_stake_account = decoded_transaction
                    .get_account_data_before(&stake_account_pubkey)
                    .map_err(|e| {
                        anyhow::anyhow!("Pre uncompressed stake account not found (is err): {}", e)
                    })?;
                let Some(pre_stake_account) = pre_stake_account else {
                    // if pre is none then the account was created in this & prob closed after. Skipping for now as we only care about retail
                    debug!(
                        "error pre stake account not found {} on {}",
                        stake_account_pubkey, tx_id
                    );
                    continue;
                };
                let stake_account = parse_stake(&pre_stake_account)?;
                let Some((withdraw_auth_pubkey, stake_auth_pubkey, account_balance)) =
                    self.get_stake_acc_data(stake_account)?
                else {
                    debug!("Stake account data not found for {}", stake_account_pubkey);
                    continue;
                };

                let amount = match (&event_type, ix) {
                    (
                        &NativeStakeEventType::Withdraw,
                        StakeProgramParsedInstruction::Withdraw(withdraw_data),
                    ) => withdraw_data.lamports, // for withdraws, using the amount from the ix. Since there's partial withdraws
                    _ => account_balance,
                };

                dooot.push(Dooot::NativeStakeEvent(NativeStakeEventDooot {
                    time: block_time,
                    tx_id: tx_id.to_string(),
                    event_type,
                    stake_account_pubkey,
                    withdraw_auth_pubkey: Some(withdraw_auth_pubkey),
                    stake_auth_pubkey: Some(stake_auth_pubkey),
                    amount: rust_decimal::Decimal::from(amount),
                }));
                continue;
            }

            //get stake account and deser
            let Some(stake_account) = decoded_transaction
                .get_account_data_after(&stake_account_pubkey)
                .map_err(|e| anyhow::anyhow!("Stake account not found (is err): {}", e))?
            else {
                // TODO its possible after a delegate there is a split off ix that creates a new stake account and closes the old one
                // we should handle this case later but for now we only care about retail
                debug!("error stake account not found {:#?}", event_type);
                break;
            };
            let stake_account = parse_stake(&stake_account)?;

            let (withdraw_auth_pubkey, stake_auth_pubkey, account_balance) = self
                .get_stake_acc_data(stake_account)?
                .ok_or(anyhow::anyhow!(
                    "Stake account data not found for {}",
                    stake_account_pubkey
                ))?;

            dooot.push(Dooot::NativeStakeEvent(NativeStakeEventDooot {
                time: block_time,
                tx_id: tx_id.to_string(),
                event_type,
                stake_account_pubkey,
                withdraw_auth_pubkey: Some(withdraw_auth_pubkey),
                stake_auth_pubkey: Some(stake_auth_pubkey),
                amount: rust_decimal::Decimal::from(account_balance),
            }));
        }
        Ok(())
    }
}

impl NativeStaking {
    pub fn get_stake_acc_data(
        &self,
        stake_account: StakeAccountType,
    ) -> Result<Option<(String, String, u64)>> {
        match stake_account {
            StakeAccountType::Initialized(s) | StakeAccountType::Delegated(s) => Ok(Some((
                s.meta.authorized.withdrawer.to_string(),
                s.meta.authorized.staker.to_string(),
                // get voter from delegation. ITs the validator
                s.stake
                    .map(|s| s.delegation.stake.parse())
                    .transpose()
                    .map_err(|e| anyhow::anyhow!("Stake delegation amount parse err: {}", e))?
                    .unwrap_or(0u64),
            ))),
            _ => Ok(None),
        }
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
        let message_json =
            fs::read_to_string("./tests/native_staking/native_staking_1.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = NativeStaking;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        println!("dooot: {:#?}", dooot);

        let Dooot::NativeStakeEvent(stake_event) = &dooot[0] else {
            panic!("Expected a stake dooot");
        };
        assert!(matches!(
            stake_event.event_type,
            NativeStakeEventType::Delegate
        ));
        assert_eq!(
            stake_event.stake_account_pubkey,
            "Cjtwb3vitbPtSrRSocUo8U8jmvnofutUqLKBBAjkCAFo"
        );
        assert_eq!(
            stake_event.withdraw_auth_pubkey,
            Some("DNobRHNw1MRGtLaduS32y6rxQVLt8iznZZdF4Gk188Ei".to_string())
        );
        assert_eq!(
            stake_event.stake_auth_pubkey,
            Some("DNobRHNw1MRGtLaduS32y6rxQVLt8iznZZdF4Gk188Ei".to_string())
        );
        assert_eq!(
            stake_event.amount,
            rust_decimal::Decimal::from(5052806567u64)
        );
    }
    #[tokio::test]
    async fn invoke_failed() {
        let path = "./tests/stake_pool/failed.json";
        let message_json = fs::read_to_string(path).unwrap();
        let json_val = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = NativeStaking;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn invoke_move_lamports() {
        let path = "./tests/spl/move-lamports.json";
        let message_json = fs::read_to_string(path).unwrap();
        let json_val = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = NativeStaking;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn invoke_move_stake() {
        let path = "./tests/spl/move-stake.json";
        let message_json = fs::read_to_string(path).unwrap();
        let json_val = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = NativeStaking;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
    }
}
