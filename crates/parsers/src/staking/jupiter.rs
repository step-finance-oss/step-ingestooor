use std::str::FromStr;

use anyhow::Result;
use async_trait::async_trait;
use borsh::BorshDeserialize;

use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
use step_ingestooor_sdk::{
    dooot::{Dooot, FarmInfoDooot, FarmUserDooot},
    trait_defs::{discriminatable::Discriminatable, transaction::TransactionLike},
    utils::{step_utils::StepUtils, util_fns::convert_blocktime_to_micros},
};

use step_ingestooor_idl_gens::jupiter_staking_gen::jupiter_staking::accounts::{Escrow, Locker};

use crate::state_source::StateSource;

pub struct JupiterStaking;
pub const JUPITER_STAKING_PROGRAM_ID: &str = "voTpe3tHQ7AjQHMapgSue2HJFAh2cGsdokqN3XqmVSj";

#[async_trait]
impl crate::TransactionParserTrait for JupiterStaking {
    fn get_name(&self) -> &'static str {
        "JupiterStaking"
    }
    fn get_filters(&self) -> Vec<crate::TransactionParserFilter> {
        vec![crate::TransactionParserFilter::ProgramId(
            JUPITER_STAKING_PROGRAM_ID.to_string(),
        )]
    }
    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let accounts = decoded_transaction
            .iter_accounts()
            .filter(|a| a.post_owner_program == Some(JUPITER_STAKING_PROGRAM_ID))
            .collect::<Vec<_>>();
        let time = decoded_transaction.get_block_time();
        for acct in accounts {
            let data = match acct.unpack_post_datum()? {
                Some(data) => data,
                None => {
                    log::warn!(
                        "JUPITER STAKING: Missing post datum (could be closed) for account {}",
                        acct.pubkey
                    );
                    continue;
                }
            };
            let discrim = &data[0..8];
            if discrim == Locker::discriminator().1 {
                let locker = Locker::deserialize(&mut &data[8..])?;
                let farm_info = Dooot::FarmInfo(FarmInfoDooot {
                    deposit_token_mint: locker.token_mint.to_string(),
                    deposit_token_vault: acct.pubkey.to_string(), // vault is the users escrows
                    reward_token_vaults: vec![],
                    farm_pubkey: acct.pubkey.to_string(),
                    program_pubkey: JUPITER_STAKING_PROGRAM_ID.to_string(),
                });
                dooot.push(farm_info);
            }
            if discrim == Escrow::discriminator().1 {
                let escrow = Escrow::deserialize(&mut &data[8..])?;
                let farm_user = Dooot::FarmUser(FarmUserDooot {
                    time,
                    farm_pubkey: escrow.locker.to_string(),
                    user_pubkey: escrow.owner.to_string(),
                    position_pubkey: acct.pubkey.to_string(),
                    qty_staked: escrow.amount,
                    reward_atoms_pending: vec![],
                    reward_atoms_per_atom_complete: vec![],
                    closed: false,
                });
                dooot.push(farm_user);
            }
        }
        Ok(())
    }
}

impl StateSource for JupiterStaking {
    fn get_name(&self) -> &'static str {
        "JupiterStaking"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let program_accounts =
            acct_provider.program_accounts(Pubkey::from_str(JUPITER_STAKING_PROGRAM_ID)?, None);

        for acct in program_accounts {
            let data = acct.account.data();
            if data.starts_with(Locker::discriminator().1) {
                let locker = Locker::deserialize(&mut &data[8..])?;
                let farm_info = Dooot::FarmInfo(FarmInfoDooot {
                    deposit_token_mint: locker.token_mint.to_string(),
                    deposit_token_vault: acct.pubkey.to_string(), // vault is the users escrows
                    reward_token_vaults: vec![],
                    farm_pubkey: acct.pubkey.to_string(),
                    program_pubkey: JUPITER_STAKING_PROGRAM_ID.to_string(),
                });
                tx.send(farm_info)?;
            }
            if data.starts_with(Escrow::discriminator().1) {
                let escrow = Escrow::deserialize(&mut &data[8..])?;
                let farm_user = Dooot::FarmUser(FarmUserDooot {
                    time,
                    farm_pubkey: escrow.locker.to_string(),
                    user_pubkey: escrow.owner.to_string(),
                    position_pubkey: acct.pubkey.to_string(),
                    qty_staked: escrow.amount,
                    reward_atoms_pending: vec![],
                    reward_atoms_per_atom_complete: vec![],
                    closed: false,
                });
                tx.send(farm_user)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, rc::Rc};

    use super::*;
    use crate::TransactionParserTrait;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;

    #[tokio::test]
    async fn stake() {
        let txn_json = fs::read_to_string("tests/jupiter_staking/stake.json").unwrap();
        let decoded_txn = serde_json::from_str::<DecodedTransaction>(&txn_json).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = JupiterStaking;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 2);
    }
}
