use std::collections::{HashMap, HashSet};

use solana_sdk::account::ReadableAccount;
use solana_sdk::pubkey::Pubkey;
use step_ingestooor_sdk::trait_defs::transaction::TxAccountInfoStage;

use crate::state_source::StateSource;
use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::Result;
use async_trait::async_trait;
use borsh::{BorshDeserialize, BorshSerialize};
use step_ingestooor_sdk::dooot::{FarmInfoUnderlyingMintVault, FarmPerUnitType, FarmUserDooot};
use step_ingestooor_sdk::utils::constants::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID};
use step_ingestooor_sdk::utils::util_fns::convert_blocktime_to_micros;
use step_ingestooor_sdk::{
    dooot::{Dooot, FarmGlobalDooot, FarmInfoDooot},
    trait_defs::transaction::TransactionLike,
    utils::step_utils::StepUtils,
};

pub const RAYDIUM_FARM_PROGRAM_ID_V6: &str = "FarmqiPv5eAj3j1GMdMCMUGXqPUvmquZtMy86QH6rzhG";

pub struct RaydiumFarmsV6;

#[async_trait]
impl TransactionParserTrait for RaydiumFarmsV6 {
    fn get_name(&self) -> &'static str {
        "RaydiumFarmsV6"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            RAYDIUM_FARM_PROGRAM_ID_V6.into(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();

        let accts = txn.iter_accounts();

        for acct in accts {
            if !acct.datum_changed {
                continue;
            }
            let (data, owner_program) = if acct.was_closed {
                (acct.unpack_pre_datum()?, acct.pre_owner_program)
            } else {
                (acct.unpack_post_datum()?, acct.post_owner_program)
            };
            if let (Some(data), Some(RAYDIUM_FARM_PROGRAM_ID_V6)) = (data, owner_program) {
                match data.len() {
                    1976 => {
                        let farm = RaydiumFarmStateV6::try_from_slice(&data)?;

                        let mut reward_token_vaults: Vec<FarmInfoUnderlyingMintVault> = vec![];

                        let mut reward_end: Vec<u64> = vec![];
                        let mut reward_rates: Vec<u64> = vec![];
                        let mut reward_atoms_per_atom_stored: Vec<String> = vec![];

                        for reward_info in &farm.reward_infos {
                            reward_token_vaults.push(FarmInfoUnderlyingMintVault {
                                mint: reward_info.reward_mint.to_string(),
                                vault: reward_info.reward_vault.to_string(),
                            });
                            reward_end.push(reward_info.reward_end_time);
                            reward_rates.push(reward_info.reward_per_second);
                            reward_atoms_per_atom_stored
                                .push(reward_info.acc_reward_per_share.to_string());
                        }

                        if acct.was_created {
                            let farm_info = FarmInfoDooot {
                                farm_pubkey: acct.pubkey.to_string(),
                                program_pubkey: RAYDIUM_FARM_PROGRAM_ID_V6.to_string(),
                                deposit_token_mint: farm.lp_mint.to_string(),
                                deposit_token_vault: farm.lp_vault.to_string(),
                                reward_token_vaults,
                            };
                            dooots.push(Dooot::FarmInfo(farm_info));
                        }

                        let total_qty_staked = txn
                            .get_token_account_balance(
                                &farm.lp_vault.to_string(),
                                TxAccountInfoStage::Post,
                            )
                            .ok()
                            .flatten()
                            .unwrap_or(0);

                        let global_farm_dooot = FarmGlobalDooot {
                            time,
                            per_unit_type: FarmPerUnitType::PerSlot,
                            farm_pubkey: acct.pubkey.to_string(),
                            last_updated: farm.reward_infos[0].reward_last_update_time, // I think these always all update at same time
                            reward_end,
                            reward_rates,
                            reward_atoms_per_atom_stored,
                            total_qty_staked,
                            closed: acct.was_closed,
                        };
                        dooots.push(Dooot::FarmGlobal(global_farm_dooot));
                    }
                    296 => {
                        let farm = RaydiumFarmLedgerV6_1::try_from_slice(&data)?;

                        let mut reward_atoms_per_atom_complete: Vec<String> = vec![];
                        let mut reward_atoms_pending: Vec<u64> = vec![];

                        for debt in &farm.reward_debts {
                            reward_atoms_per_atom_complete.push(if acct.was_closed {
                                "0".to_string()
                            } else {
                                debt.to_string()
                            });
                            reward_atoms_pending.push(0);
                        }

                        let user_farm_dooot = FarmUserDooot {
                            time,
                            farm_pubkey: farm.id.to_string(),
                            user_pubkey: farm.owner.to_string(),
                            position_pubkey: acct.pubkey.to_string(),
                            qty_staked: if acct.was_closed { 0 } else { farm.deposited },
                            reward_atoms_pending,
                            reward_atoms_per_atom_complete,
                            closed: acct.was_closed,
                        };
                        dooots.push(Dooot::FarmUser(user_farm_dooot));
                    }
                    _ => {
                        continue;
                    }
                }
            }
        }
        Ok(())
    }
}

impl StateSource for RaydiumFarmsV6 {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let raydium_pk = RAYDIUM_FARM_PROGRAM_ID_V6.parse::<Pubkey>()?;
        let raydium_accts_iter = acct_provider.program_accounts(raydium_pk, None);

        let mut raydium_farm_lookup: HashMap<Pubkey, RaydiumFarmStateV6> = HashMap::new();
        let mut vault_accounts = HashSet::new();
        let mut deposit_vault_balance_lookup: HashMap<Pubkey, u64> = HashMap::new();

        for acct in raydium_accts_iter {
            let pk = acct.pubkey;
            let data = acct.account.data();
            match data.len() {
                1976 => {
                    let farm = RaydiumFarmStateV6::try_from_slice(data)?;
                    let mut skip = false;
                    if farm.lp_vault == Pubkey::default() {
                        println!(
                            "Farm {} has default lp_vault. Skipping bc idk how to handle.",
                            pk
                        );
                        skip = true;
                    }
                    if skip {
                        continue;
                    }
                    raydium_farm_lookup.insert(pk, farm.clone());
                    vault_accounts.insert(farm.lp_vault);
                }
                296 => {
                    let farm = RaydiumFarmLedgerV6_1::try_from_slice(data)?;

                    let mut reward_atoms_per_atom_complete: Vec<String> = vec![];
                    let mut reward_atoms_pending: Vec<u64> = vec![];

                    for debt in &farm.reward_debts {
                        reward_atoms_per_atom_complete.push(debt.to_string());
                        reward_atoms_pending.push(0);
                    }

                    let user_info_dooot = Dooot::FarmUser(FarmUserDooot {
                        time,
                        farm_pubkey: farm.id.to_string(),
                        user_pubkey: farm.owner.to_string(),
                        position_pubkey: acct.pubkey.to_string(),
                        qty_staked: farm.deposited,
                        reward_atoms_pending,
                        reward_atoms_per_atom_complete,
                        closed: false,
                    });
                    tx.send(user_info_dooot)?;
                }
                _ => {
                    continue;
                }
            }
        }

        let all_accounts_iter = acct_provider.all_accounts();
        for acct in all_accounts_iter {
            let owner_pk = acct.account.owner().to_string();
            if owner_pk != TOKEN_PROGRAM_ID && owner_pk != TOKEN_2022_PROGRAM_ID {
                continue;
            }
            let pk = acct.pubkey;
            if vault_accounts.contains(&pk) {
                let data = acct.account.data();
                if data.len() < 73 {
                    return Err(anyhow::anyhow!(
                        "Vault account {} has invalid data length {}",
                        pk,
                        data.len()
                    ));
                }
                let amount = u64::from_le_bytes(data[64..72].try_into()?);
                deposit_vault_balance_lookup.insert(pk, amount);
            }
        }

        for (farm_pk, farm) in raydium_farm_lookup {
            let mut reward_token_vaults: Vec<FarmInfoUnderlyingMintVault> = vec![];
            let mut reward_end: Vec<u64> = vec![];
            let mut reward_rates: Vec<u64> = vec![];
            let mut reward_atoms_per_atom_stored: Vec<String> = vec![];

            for reward_info in &farm.reward_infos {
                reward_token_vaults.push(FarmInfoUnderlyingMintVault {
                    mint: reward_info.reward_mint.to_string(),
                    vault: reward_info.reward_vault.to_string(),
                });
                reward_end.push(reward_info.reward_end_time);
                reward_rates.push(reward_info.reward_per_second);
                reward_atoms_per_atom_stored.push(reward_info.acc_reward_per_share.to_string());
            }
            let farm_info_dooot = Dooot::FarmInfo(FarmInfoDooot {
                farm_pubkey: farm_pk.to_string(),
                program_pubkey: RAYDIUM_FARM_PROGRAM_ID_V6.to_string(),
                deposit_token_mint: farm.lp_mint.to_string(),
                deposit_token_vault: farm.lp_vault.to_string(),
                reward_token_vaults,
            });
            tx.send(farm_info_dooot)?;

            let total_qty_staked = *deposit_vault_balance_lookup
                .get(&farm.lp_vault)
                .expect("lp_vault not found in deposit_vault_balance_lookup");
            let global_farm_dooot = Dooot::FarmGlobal(FarmGlobalDooot {
                time,
                per_unit_type: FarmPerUnitType::PerSlot,
                farm_pubkey: farm_pk.to_string(),
                last_updated: farm.reward_infos[0].reward_last_update_time,
                reward_end,
                reward_rates,
                reward_atoms_per_atom_stored,
                total_qty_staked,
                closed: false,
            });
            tx.send(global_farm_dooot)?;
        }
        Ok(())
    }

    fn get_name(&self) -> &'static str {
        "RaydiumFarmsV6"
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use step_ingestooor_sdk::{
        decoded_transaction::DecodedTransaction, utils::test_utils::init_test_extras_bare,
    };

    use super::*;

    #[tokio::test]
    async fn test_deposit_v6() {
        let file = fs::read_to_string("./tests/raydium/farms/deposit_v6.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let parser = RaydiumFarmsV6 {};
        let (step_utils, _) = init_test_extras_bare().await;

        let mut dooots = vec![];
        parser.invoke(&txn, &mut dooots, &step_utils).await.unwrap();

        if let (Dooot::FarmGlobal(global), Dooot::FarmUser(user)) = (&dooots[0], &dooots[1]) {
            let current_time = 1752277466_u64;
            let total_reward = calculate_pending_rewards(global, user, current_time);

            println!("total_reward: {:?}", total_reward);
        }
    }

    #[tokio::test]
    async fn test_withdraw_all_v6() {
        let file = fs::read_to_string("./tests/raydium/farms/withdraw_all_v6.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let parser = RaydiumFarmsV6 {};
        let (step_utils, _) = init_test_extras_bare().await;

        let mut dooots = vec![];
        parser.invoke(&txn, &mut dooots, &step_utils).await.unwrap();

        if let (Dooot::FarmGlobal(_global), Dooot::FarmUser(user)) = (&dooots[0], &dooots[1]) {
            assert!(user.closed, "user.closed should be true");
        } else {
            panic!("dooots[0] and dooots[1] are not FarmGlobal and FarmUser");
        }
    }

    fn calculate_pending_rewards(
        global: &FarmGlobalDooot,
        user: &FarmUserDooot,
        current_time: u64,
    ) -> Vec<u128> {
        let last_time = global.last_updated;

        let mut rewards: Vec<u128> = vec![];

        for (i, reward_rate) in global.reward_rates.iter().enumerate() {
            let reward_atoms_per_atom_elapsed = ((current_time - last_time) * reward_rate) as u128
                * 1_000_000_000_000_000
                / global.total_qty_staked as u128;

            let reward_atoms_per_atom_stored = global.reward_atoms_per_atom_stored[i]
                .parse::<u128>()
                .unwrap();

            let total_reward_per_atom =
                reward_atoms_per_atom_stored + reward_atoms_per_atom_elapsed;

            let reward_atoms_per_atom_complete = user.reward_atoms_per_atom_complete[i]
                .parse::<u128>()
                .unwrap();
            let reward_owed =
                user.qty_staked as u128 * total_reward_per_atom / 1_000_000_000_000_000;

            let reward_atoms = reward_owed - reward_atoms_per_atom_complete;

            rewards.push(reward_atoms);
        }

        rewards
    }
}

#[derive(Debug, BorshDeserialize, BorshSerialize, Clone)]
pub struct RaydiumFarmStateV6 {
    pub unused: u64,
    pub state: u64,
    pub nonce: u64,
    pub valid_reward_token_num: u64,
    pub reward_multiplier: u128,
    pub reward_period_max: u64,
    pub reward_period_min: u64,
    pub reward_period_extend: u64,
    pub lp_mint: Pubkey,
    pub lp_vault: Pubkey,
    pub reward_infos: [RaydiumFarmStateV6RewardInfo; 5],
    pub creator: Pubkey,
    pub padding_1: Pubkey,
    pub padding: [u64; 32],
}

#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub struct RaydiumFarmLedgerV6_1 {
    pub unused: u64,
    pub state: u64,
    pub id: Pubkey,
    pub owner: Pubkey,
    pub deposited: u64,
    pub reward_debts: [u128; 5],
    pub padding: [u64; 16],
}

#[derive(Debug, BorshDeserialize, BorshSerialize, Clone)]
pub struct RaydiumFarmStateV6RewardInfo {
    pub reward_state: u64,
    pub reward_open_time: u64,
    pub reward_end_time: u64,
    pub reward_last_update_time: u64,
    pub total_reward: u64,
    pub total_reward_emissioned: u64,
    pub reward_claimed: u64,
    pub reward_per_second: u64,
    pub acc_reward_per_share: u128,
    pub reward_vault: Pubkey,
    pub reward_mint: Pubkey,
    pub reward_sender: Pubkey,
    pub reward_type: u64,
    pub padding: [u64; 15],
}
