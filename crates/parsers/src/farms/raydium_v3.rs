use std::collections::{HashMap, HashSet};

use solana_sdk::pubkey::Pubkey;
use step_ingestooor_sdk::trait_defs::transaction::TxAccountInfoStage;
use step_ingestooor_sdk::utils::constants::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID};

use crate::state_source::StateSource;
use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::Result;
use async_trait::async_trait;
use borsh::{BorshDeserialize, BorshSerialize};
use solana_sdk::account::ReadableAccount;
use step_ingestooor_sdk::dooot::{FarmPerUnitType, FarmUserDooot};
use step_ingestooor_sdk::utils::util_fns::convert_blocktime_to_micros;
use step_ingestooor_sdk::{
    dooot::{Dooot, FarmGlobalDooot, FarmInfoDooot, FarmInfoUnderlyingMintVault},
    trait_defs::transaction::TransactionLike,
    utils::step_utils::StepUtils,
};

pub const RAYDIUM_FARM_PROGRAM_ID_V3: &str = "EhhTKczWMGQt46ynNeRX1WfeagwwJd7ufHvCDjRxjo5Q";

pub struct RaydiumFarmsV3;

#[async_trait]
impl TransactionParserTrait for RaydiumFarmsV3 {
    fn get_name(&self) -> &'static str {
        "RaydiumFarmsV3"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            RAYDIUM_FARM_PROGRAM_ID_V3.into(),
        )]
    }
    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let current_slot = txn.get_slot();

        let accts = txn.iter_accounts();

        for acct in accts {
            if let Some(data) = acct.unpack_post_datum()? {
                if !acct.datum_changed {
                    continue;
                }

                match data.len() {
                    200 => {
                        let farm = RaydiumFarmStateV3::try_from_slice(&data)?;

                        if acct.was_created {
                            let lp_vault_info =
                                txn.get_token_account_data_by_key(&farm.lp_vault.to_string())?;
                            let reward_vault_info =
                                txn.get_token_account_data_by_key(&farm.reward_vault.to_string())?;

                            let farm_info = FarmInfoDooot {
                                farm_pubkey: acct.pubkey.to_string(),
                                program_pubkey: RAYDIUM_FARM_PROGRAM_ID_V3.to_string(),
                                deposit_token_mint: lp_vault_info.mint,
                                deposit_token_vault: farm.lp_vault.to_string(),
                                reward_token_vaults: vec![FarmInfoUnderlyingMintVault {
                                    mint: reward_vault_info.mint,
                                    vault: farm.reward_vault.to_string(),
                                }],
                            };
                            dooots.push(Dooot::FarmInfo(farm_info));
                        }

                        let total_qty_staked = txn
                            .get_token_account_balance(
                                &farm.lp_vault.to_string(),
                                TxAccountInfoStage::Post,
                            )?
                            .unwrap_or(0);

                        let global_farm_dooot = FarmGlobalDooot {
                            time,
                            per_unit_type: FarmPerUnitType::PerSlot,
                            farm_pubkey: acct.pubkey.to_string(),
                            last_updated: farm.last_slot,
                            reward_end: vec![u64::MAX],
                            reward_rates: vec![farm.per_slot_reward],
                            reward_atoms_per_atom_stored: vec![farm.per_share_reward.to_string()],
                            total_qty_staked,
                            closed: false,
                        };
                        dooots.push(Dooot::FarmGlobal(global_farm_dooot));
                    }
                    88 => {
                        let farm = RaydiumFarmLedgerV3_1::try_from_slice(&data)?;

                        let user_farm_dooot = FarmUserDooot {
                            time,
                            farm_pubkey: farm.id.to_string(),
                            user_pubkey: farm.owner.to_string(),
                            position_pubkey: acct.pubkey.to_string(),
                            qty_staked: farm.deposited,
                            reward_atoms_pending: vec![0], // no reward atoms pending for ray v3
                            reward_atoms_per_atom_complete: vec![farm.reward_debts[0].to_string()], // only a single reward for ray v3
                            closed: false,
                        };
                        dooots.push(Dooot::FarmUser(user_farm_dooot));
                    }
                    232 => {
                        let farm = RaydiumFarmLedgerV3_2::try_from_slice(&data)?;

                        let user_farm_dooot = FarmUserDooot {
                            time,
                            farm_pubkey: farm.id.to_string(),
                            user_pubkey: farm.owner.to_string(),
                            position_pubkey: acct.pubkey.to_string(),
                            qty_staked: farm.deposited,
                            reward_atoms_pending: vec![0], // no reward atoms pending for ray v3
                            reward_atoms_per_atom_complete: vec![farm.reward_debts[0].to_string()], // only a single reward for ray v3
                            closed: false,
                        };
                        dooots.push(Dooot::FarmUser(user_farm_dooot));
                    }
                    _ => {
                        continue;
                    }
                }
            } else if acct.was_closed {
                let Some(data) = acct.unpack_pre_datum()? else {
                    continue;
                };

                match data.len() {
                    200 => {
                        // zero out the farm global dooot for a closed farm
                        let global_farm_dooot = FarmGlobalDooot {
                            time,
                            per_unit_type: FarmPerUnitType::PerSlot,
                            farm_pubkey: acct.pubkey.to_string(),
                            last_updated: current_slot,
                            reward_end: vec![u64::MAX],
                            reward_rates: vec![0],
                            reward_atoms_per_atom_stored: vec!["0".to_string()],
                            total_qty_staked: 0,
                            closed: true,
                        };
                        dooots.push(Dooot::FarmGlobal(global_farm_dooot));
                    }
                    88 => {
                        let farm = RaydiumFarmLedgerV3_1::try_from_slice(&data)?;

                        let user_farm_dooot = FarmUserDooot {
                            time,
                            farm_pubkey: farm.id.to_string(),
                            user_pubkey: farm.owner.to_string(),
                            position_pubkey: acct.pubkey.to_string(),
                            qty_staked: 0,
                            reward_atoms_pending: vec![0],
                            reward_atoms_per_atom_complete: vec!["0".to_string()],
                            closed: true,
                        };
                        dooots.push(Dooot::FarmUser(user_farm_dooot));
                    }
                    232 => {
                        let farm = RaydiumFarmLedgerV3_2::try_from_slice(&data)?;

                        let user_farm_dooot = FarmUserDooot {
                            time,
                            farm_pubkey: farm.id.to_string(),
                            user_pubkey: farm.owner.to_string(),
                            position_pubkey: acct.pubkey.to_string(),
                            qty_staked: 0,
                            reward_atoms_pending: vec![0],
                            reward_atoms_per_atom_complete: vec!["0".to_string()],
                            closed: true,
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

impl StateSource for RaydiumFarmsV3 {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let raydium_pk = RAYDIUM_FARM_PROGRAM_ID_V3.parse::<Pubkey>()?;
        let raydium_accts_iter = acct_provider.program_accounts(raydium_pk, None);

        let mut raydium_farm_lookup: HashMap<Pubkey, RaydiumFarmStateV3> = HashMap::new();
        let mut vault_info_lookup: HashMap<Pubkey, (String, u64)> = HashMap::new();
        let mut vault_accounts = HashSet::new();

        for acct in raydium_accts_iter {
            let pk = acct.pubkey;
            let data = acct.account.data();
            match data.len() {
                200 => {
                    let farm = RaydiumFarmStateV3::try_from_slice(data)?;
                    let mut skip = false;
                    if farm.lp_vault == Pubkey::default() {
                        println!(
                            "Farm {} has default lp_vault. Skipping bc idk how to handle.",
                            pk
                        );
                        skip = true;
                    }
                    if farm.reward_vault == Pubkey::default() {
                        println!(
                            "Farm {} has default reward_vault. Skipping bc idk how to handle.",
                            pk
                        );
                        skip = true;
                    }
                    if skip {
                        continue;
                    }
                    raydium_farm_lookup.insert(pk, farm.clone());
                    vault_accounts.insert(farm.lp_vault);
                    vault_accounts.insert(farm.reward_vault);
                }
                88 => {
                    let farm = RaydiumFarmLedgerV3_1::try_from_slice(data)?;

                    let user_info_dooot = Dooot::FarmUser(FarmUserDooot {
                        time,
                        farm_pubkey: farm.id.to_string(),
                        user_pubkey: farm.owner.to_string(),
                        position_pubkey: acct.pubkey.to_string(),
                        qty_staked: farm.deposited,
                        reward_atoms_pending: vec![0], // no reward atoms pending for ray v3
                        reward_atoms_per_atom_complete: vec![farm.reward_debts[0].to_string()],
                        closed: false,
                    });
                    tx.send(user_info_dooot)?;
                }
                232 => {
                    let farm = RaydiumFarmLedgerV3_2::try_from_slice(data)?;

                    let user_info_dooot = Dooot::FarmUser(FarmUserDooot {
                        time,
                        farm_pubkey: farm.id.to_string(),
                        user_pubkey: farm.owner.to_string(),
                        position_pubkey: acct.pubkey.to_string(),
                        qty_staked: farm.deposited,
                        reward_atoms_pending: vec![0], // no reward atoms pending for ray v3
                        reward_atoms_per_atom_complete: vec![farm.reward_debts[0].to_string()], // only a single reward for ray v3
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
                let mint = Pubkey::try_from(&data[0..32])?.to_string();
                let amount = u64::from_le_bytes(data[64..72].try_into()?);
                vault_info_lookup.insert(pk, (mint, amount));
            }
        }

        for (farm_pk, farm_info) in raydium_farm_lookup {
            let farm_info_dooot = Dooot::FarmInfo(FarmInfoDooot {
                farm_pubkey: farm_pk.to_string(),
                program_pubkey: RAYDIUM_FARM_PROGRAM_ID_V3.to_string(),
                deposit_token_mint: vault_info_lookup
                    .get(&farm_info.lp_vault)
                    .expect("lp_vault not found in vault_info_lookup")
                    .0
                    .clone(),
                deposit_token_vault: farm_info.lp_vault.to_string(),
                reward_token_vaults: vec![FarmInfoUnderlyingMintVault {
                    mint: vault_info_lookup
                        .get(&farm_info.reward_vault)
                        .expect("reward_vault not found in vault_info_lookup")
                        .0
                        .clone(),
                    vault: farm_info.reward_vault.to_string(),
                }],
            });
            tx.send(farm_info_dooot)?;

            let total_qty_staked = vault_info_lookup
                .get(&farm_info.lp_vault)
                .expect("lp_vault not found in vault_info_lookup for total_qty_staked")
                .1;

            let global_farm_dooot = Dooot::FarmGlobal(FarmGlobalDooot {
                time,
                per_unit_type: FarmPerUnitType::PerSlot,
                farm_pubkey: farm_pk.to_string(),
                last_updated: farm_info.last_slot,
                reward_end: vec![u64::MAX],
                reward_rates: vec![farm_info.per_slot_reward],
                reward_atoms_per_atom_stored: vec![farm_info.per_share_reward.to_string()],
                total_qty_staked,
                closed: false,
            });
            tx.send(global_farm_dooot)?;
        }

        Ok(())
    }

    fn get_name(&self) -> &'static str {
        "RaydiumFarmsV3"
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
    async fn test_withdraw_all_and_claim_v3() {
        let file =
            fs::read_to_string("./tests/raydium/farms/withdraw_all_and_claim_v3.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let parser = RaydiumFarmsV3 {};
        let (step_utils, _) = init_test_extras_bare().await;

        let mut dooots = vec![];
        parser.invoke(&txn, &mut dooots, &step_utils).await.unwrap();

        if let (Dooot::FarmGlobal(global), Dooot::FarmUser(user)) = (&dooots[0], &dooots[1]) {
            // user account is closed so rewards should be 0
            let current_slot = 351998820_u64;
            let total_reward = calculate_pending_rewards(global, user, current_slot);

            assert_eq!(total_reward, 0);
        }
    }

    #[tokio::test]
    async fn test_withdraw_partial_v3() {
        let file = fs::read_to_string("./tests/raydium/farms/withdraw_partial_v3_2.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let parser = RaydiumFarmsV3 {};
        let (step_utils, _) = init_test_extras_bare().await;

        let mut dooots = vec![];
        parser.invoke(&txn, &mut dooots, &step_utils).await.unwrap();

        if let (Dooot::FarmGlobal(global), Dooot::FarmUser(user)) = (&dooots[0], &dooots[1]) {
            // at this slot time total rewards for this user should be 57 RAY atoms
            let current_slot = 351995999_u64;
            let total_reward = calculate_pending_rewards(global, user, current_slot);

            assert_eq!(total_reward, 57);
        }
    }

    fn calculate_pending_rewards(
        global: &FarmGlobalDooot,
        user: &FarmUserDooot,
        current_slot: u64,
    ) -> u128 {
        let last_slot = global.last_updated;
        let reward_rate = global.reward_rates[0];
        let total_qty_staked = global.total_qty_staked;
        let reward_atoms_per_atom_stored = global.reward_atoms_per_atom_stored[0]
            .parse::<u128>()
            .unwrap();

        let reward_atoms_per_atom_elapsed = ((current_slot - last_slot) * reward_rate) as u128
            * 1_000_000_000
            / total_qty_staked as u128;
        let total_reward_per_atom = reward_atoms_per_atom_stored + reward_atoms_per_atom_elapsed;

        let reward_atoms_per_atom_complete = user.reward_atoms_per_atom_complete[0]
            .parse::<u128>()
            .unwrap();
        let qty_staked = user.qty_staked as u128;

        let reward_owed = qty_staked * total_reward_per_atom / 1_000_000_000;

        // NOTE: For Raydium V3, 'reward_atoms_per_atom_complete' is a *total* not a "per atom" value.

        reward_owed - reward_atoms_per_atom_complete
    }
}

/// all these layouts taken from these links:
/// -> https://github.com/raydium-io/raydium-sdk/blob/master/src/farm/layout.ts
/// -> https://github.com/step-finance/step-common/blob/master/packages/farms/src/lib/info/raydium/layout.ts
#[derive(Debug, BorshDeserialize, BorshSerialize, Clone)]
pub struct RaydiumFarmStateV3 {
    pub state: u64,
    pub nonce: u64,
    pub lp_vault: Pubkey,
    pub reward_vault: Pubkey,
    pub fee_owner: Pubkey,
    pub owner: Pubkey,
    pub fee_y: u64,
    pub fee_x: u64,
    pub total_reward: u64,
    pub per_share_reward: u128,
    pub last_slot: u64,
    pub per_slot_reward: u64,
}

#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub struct RaydiumFarmLedgerV3_1 {
    pub state: u64,
    pub id: Pubkey,
    pub owner: Pubkey,
    pub deposited: u64,
    pub reward_debts: [u64; 1],
}

#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub struct RaydiumFarmLedgerV3_2 {
    pub state: u64,
    pub id: Pubkey,
    pub owner: Pubkey,
    pub deposited: u64,
    pub reward_debts: [u128; 1],
    pub unused: u64,
    pub vote_locked_balance: u64,
    pub padding: [u64; 15],
}
