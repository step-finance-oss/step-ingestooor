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

pub const RAYDIUM_FARM_PROGRAM_ID_V5: &str = "9KEPoZmtHUrBbhWN1v1KWLMkkvwY6WLtAVUCPRtRjP4z";

pub struct RaydiumFarmsV5;

#[async_trait]
impl TransactionParserTrait for RaydiumFarmsV5 {
    fn get_name(&self) -> &'static str {
        "RaydiumFarmsV5"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            RAYDIUM_FARM_PROGRAM_ID_V5.into(),
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

        let accts = txn.iter_accounts().filter(|a| {
            a.datum_changed
                && (a
                    .post_owner_program
                    .is_some_and(|p| p == RAYDIUM_FARM_PROGRAM_ID_V5)
                    || a.pre_owner_program
                        .is_some_and(|p| p == RAYDIUM_FARM_PROGRAM_ID_V5))
        });

        for acct in accts {
            if let Some(data) = acct.unpack_post_datum()? {
                match data.len() {
                    224 => {
                        let farm = RaydiumFarmStateV5::try_from_slice(&data)?;

                        let lp_vault_pk = farm.pool_lp_token_account;
                        let reward_vault_pk = farm.pool_reward_token_account;
                        let reward_vault_pk_b = farm.pool_reward_token_account_b;

                        if acct.was_created {
                            let lp_vault_info =
                                txn.get_token_account_data_by_key(&lp_vault_pk.to_string())?;
                            let reward_vault_info =
                                txn.get_token_account_data_by_key(&reward_vault_pk.to_string())?;
                            let reward_vault_info_b =
                                txn.get_token_account_data_by_key(&reward_vault_pk_b.to_string())?;

                            let farm_info = FarmInfoDooot {
                                farm_pubkey: acct.pubkey.to_string(),
                                program_pubkey: RAYDIUM_FARM_PROGRAM_ID_V5.to_string(),
                                deposit_token_mint: lp_vault_info.mint,
                                deposit_token_vault: lp_vault_pk.to_string(),
                                reward_token_vaults: vec![
                                    FarmInfoUnderlyingMintVault {
                                        mint: reward_vault_info.mint,
                                        vault: reward_vault_pk.to_string(),
                                    },
                                    FarmInfoUnderlyingMintVault {
                                        mint: reward_vault_info_b.mint,
                                        vault: reward_vault_pk_b.to_string(),
                                    },
                                ],
                            };
                            dooots.push(Dooot::FarmInfo(farm_info));
                        }

                        let total_qty_staked = txn
                            .get_token_account_balance(
                                &lp_vault_pk.to_string(),
                                TxAccountInfoStage::Post,
                            )?
                            .unwrap_or(0);

                        let global_farm_dooot = FarmGlobalDooot {
                            time,
                            per_unit_type: FarmPerUnitType::PerSlot,
                            farm_pubkey: acct.pubkey.to_string(),
                            last_updated: farm.last_slot,
                            reward_end: vec![u64::MAX, u64::MAX],
                            reward_rates: vec![farm.per_slot_reward, farm.per_slot_reward_b],
                            reward_atoms_per_atom_stored: vec![
                                farm.per_share_reward.to_string(),
                                farm.per_share_reward_b.to_string(),
                            ],
                            total_qty_staked,
                            closed: false,
                        };
                        dooots.push(Dooot::FarmGlobal(global_farm_dooot));
                    }
                    96 => {
                        let farm = RaydiumFarmLedgerV5_1::try_from_slice(&data)?;

                        let user_farm_dooot = FarmUserDooot {
                            time,
                            farm_pubkey: farm.id.to_string(),
                            user_pubkey: farm.owner.to_string(),
                            position_pubkey: acct.pubkey.to_string(),
                            qty_staked: farm.deposited,
                            reward_atoms_pending: vec![0, 0], // no reward atoms pending for ray v5
                            reward_atoms_per_atom_complete: vec![
                                farm.reward_debts[0].to_string(),
                                farm.reward_debts[1].to_string(),
                            ],
                            closed: false,
                        };
                        dooots.push(Dooot::FarmUser(user_farm_dooot));
                    }
                    248 => {
                        let farm = RaydiumFarmLedgerV5_2::try_from_slice(&data)?;

                        let user_farm_dooot = FarmUserDooot {
                            time,
                            farm_pubkey: farm.id.to_string(),
                            user_pubkey: farm.owner.to_string(),
                            position_pubkey: acct.pubkey.to_string(),
                            qty_staked: farm.deposited,
                            reward_atoms_pending: vec![0, 0], // no reward atoms pending for ray v5
                            reward_atoms_per_atom_complete: vec![
                                farm.reward_debts[0].to_string(),
                                farm.reward_debts[1].to_string(),
                            ],
                            closed: false,
                        };
                        dooots.push(Dooot::FarmUser(user_farm_dooot));
                    }
                    _ => {
                        continue;
                    }
                }
            } else if acct.was_closed {
                let data = acct
                    .unpack_pre_datum()?
                    .ok_or(anyhow::anyhow!("Failed to get pre data"))?;

                match data.len() {
                    224 => {
                        // zero out the farm global dooot for a closed farm
                        let global_farm_dooot = FarmGlobalDooot {
                            time,
                            per_unit_type: FarmPerUnitType::PerSlot,
                            farm_pubkey: acct.pubkey.to_string(),
                            last_updated: current_slot,
                            reward_end: vec![u64::MAX, u64::MAX],
                            reward_rates: vec![0, 0],
                            reward_atoms_per_atom_stored: vec!["0".to_string(), "0".to_string()],
                            total_qty_staked: 0,
                            closed: true,
                        };
                        dooots.push(Dooot::FarmGlobal(global_farm_dooot));
                    }
                    96 => {
                        let farm = RaydiumFarmLedgerV5_1::try_from_slice(&data)?;

                        let user_farm_dooot = FarmUserDooot {
                            time,
                            farm_pubkey: farm.id.to_string(),
                            user_pubkey: farm.owner.to_string(),
                            position_pubkey: acct.pubkey.to_string(),
                            qty_staked: 0,
                            reward_atoms_pending: vec![0, 0],
                            reward_atoms_per_atom_complete: vec!["0".to_string(), "0".to_string()],
                            closed: true,
                        };
                        dooots.push(Dooot::FarmUser(user_farm_dooot));
                    }
                    248 => {
                        let farm = RaydiumFarmLedgerV5_2::try_from_slice(&data)?;

                        let user_farm_dooot = FarmUserDooot {
                            time,
                            farm_pubkey: farm.id.to_string(),
                            user_pubkey: farm.owner.to_string(),
                            position_pubkey: acct.pubkey.to_string(),
                            qty_staked: 0,
                            reward_atoms_pending: vec![0, 0],
                            reward_atoms_per_atom_complete: vec!["0".to_string(), "0".to_string()],
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

impl StateSource for RaydiumFarmsV5 {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let raydium_pk = RAYDIUM_FARM_PROGRAM_ID_V5.parse::<Pubkey>()?;
        let raydium_accts_iter = acct_provider.program_accounts(raydium_pk, None);

        let mut raydium_farm_lookup: HashMap<Pubkey, RaydiumFarmStateV5> = HashMap::new();
        let mut vault_info_lookup: HashMap<Pubkey, (String, u64)> = HashMap::new();
        let mut vault_accounts = HashSet::new();

        for acct in raydium_accts_iter {
            let pk = acct.pubkey;
            let data = acct.account.data();
            match data.len() {
                224 => {
                    let farm = RaydiumFarmStateV5::try_from_slice(data)?;
                    raydium_farm_lookup.insert(pk, farm.clone());
                    vault_accounts.insert(farm.pool_lp_token_account);
                    vault_accounts.insert(farm.pool_reward_token_account);
                    vault_accounts.insert(farm.pool_reward_token_account_b);
                }
                96 => {
                    let farm = RaydiumFarmLedgerV5_1::try_from_slice(data)?;

                    let user_info_dooot = Dooot::FarmUser(FarmUserDooot {
                        time,
                        farm_pubkey: farm.id.to_string(),
                        user_pubkey: farm.owner.to_string(),
                        position_pubkey: acct.pubkey.to_string(),
                        qty_staked: farm.deposited,
                        reward_atoms_pending: vec![0, 0], // no reward atoms pending for ray v5
                        reward_atoms_per_atom_complete: vec![
                            farm.reward_debts[0].to_string(),
                            farm.reward_debts[1].to_string(),
                        ],
                        closed: false,
                    });
                    tx.send(user_info_dooot)?;
                }
                248 => {
                    let farm = RaydiumFarmLedgerV5_2::try_from_slice(data)?;

                    let user_info_dooot = Dooot::FarmUser(FarmUserDooot {
                        time,
                        farm_pubkey: farm.id.to_string(),
                        user_pubkey: farm.owner.to_string(),
                        position_pubkey: acct.pubkey.to_string(),
                        qty_staked: farm.deposited,
                        reward_atoms_pending: vec![0, 0], // no reward atoms pending for ray v5
                        reward_atoms_per_atom_complete: vec![
                            farm.reward_debts[0].to_string(),
                            farm.reward_debts[1].to_string(),
                        ],
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
                let mint = Pubkey::try_from(&data[0..32])?.to_string();
                let amount = u64::from_le_bytes(data[64..72].try_into()?);
                vault_info_lookup.insert(pk, (mint, amount));
            }
        }

        for (farm_pk, farm) in raydium_farm_lookup {
            let Some(mint_a) = vault_info_lookup
                .get(&farm.pool_reward_token_account)
                .map(|a| a.0.clone())
            else {
                log::error!("mint_a is none for farm {}", farm_pk.to_string());
                continue;
            };
            let Some(mint_b) = vault_info_lookup
                .get(&farm.pool_reward_token_account_b)
                .map(|a| a.0.clone())
            else {
                log::error!("mint_b is none for farm {}", farm_pk.to_string());
                continue;
            };
            let farm_info_dooot = Dooot::FarmInfo(FarmInfoDooot {
                farm_pubkey: farm_pk.to_string(),
                program_pubkey: RAYDIUM_FARM_PROGRAM_ID_V5.to_string(),
                deposit_token_mint: vault_info_lookup
                    .get(&farm.pool_lp_token_account)
                    .expect("pool_lp_token_account not found in vault_info_lookup")
                    .0
                    .clone(),
                deposit_token_vault: farm.pool_lp_token_account.to_string(),
                reward_token_vaults: vec![
                    FarmInfoUnderlyingMintVault {
                        mint: mint_a,
                        vault: farm.pool_reward_token_account.to_string(),
                    },
                    FarmInfoUnderlyingMintVault {
                        mint: mint_b,
                        vault: farm.pool_reward_token_account_b.to_string(),
                    },
                ],
            });
            tx.send(farm_info_dooot)?;

            let global_farm_dooot = Dooot::FarmGlobal(FarmGlobalDooot {
                time,
                per_unit_type: FarmPerUnitType::PerSlot,
                farm_pubkey: farm_pk.to_string(),
                last_updated: farm.last_slot,
                reward_end: vec![u64::MAX, u64::MAX],
                reward_rates: vec![farm.per_slot_reward, farm.per_slot_reward_b],
                reward_atoms_per_atom_stored: vec![
                    farm.per_share_reward.to_string(),
                    farm.per_share_reward_b.to_string(),
                ],
                total_qty_staked: vault_info_lookup
                    .get(&farm.pool_lp_token_account)
                    .expect(
                        "pool_lp_token_account not found in vault_info_lookup for total_qty_staked",
                    )
                    .1,
                closed: false,
            });
            tx.send(global_farm_dooot)?;
        }

        Ok(())
    }

    fn get_name(&self) -> &'static str {
        "RaydiumFarmsV5"
    }
}

/// -> https://github.com/step-finance/step-common/blob/master/packages/farms/src/lib/info/raydium/layout.ts
#[derive(Debug, BorshDeserialize, BorshSerialize, Clone)]
pub struct RaydiumFarmStateV5 {
    pub state: u64,
    pub nonce: u64,
    pub pool_lp_token_account: Pubkey,
    pub pool_reward_token_account: Pubkey,
    pub total_reward: u64,
    pub per_share_reward: u128,
    pub per_slot_reward: u64,
    pub option: u8,
    pub padding: [u8; 7],
    pub pool_reward_token_account_b: Pubkey,
    pub total_reward_b: u64,
    pub per_share_reward_b: u128,
    pub per_slot_reward_b: u64,
    pub last_slot: u64,
    pub owner: Pubkey,
}
#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub struct RaydiumFarmLedgerV5_1 {
    pub state: u64,
    pub id: Pubkey,
    pub owner: Pubkey,
    pub deposited: u64,
    pub reward_debts: [u64; 2],
}
#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub struct RaydiumFarmLedgerV5_2 {
    pub state: u64,
    pub id: Pubkey,
    pub owner: Pubkey,
    pub deposited: u64,
    pub reward_debts: [u128; 2],
    pub padding: [u64; 17],
}
