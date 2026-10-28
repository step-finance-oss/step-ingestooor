use std::str::FromStr;

use anyhow::Result;
use async_trait::async_trait;
use borsh::BorshDeserialize;
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
use step_ingestooor_idl_gens::meteora_farms_gen;
use step_ingestooor_sdk::{
    dooot::{
        Dooot, FarmGlobalDooot, FarmInfoDooot, FarmInfoUnderlyingMintVault, FarmPerUnitType,
        FarmUserDooot,
    },
    trait_defs::transaction::TransactionLike,
    utils::{step_utils::StepUtils, util_fns::convert_blocktime_to_micros},
};

use crate::{state_source::StateSource, TransactionParserFilter, TransactionParserTrait};

pub const METEORA_FARMS_PROGRAM_ID: &str = "FarmuwXPWXvefWUeqFAa5w6rifLkq5X6E8bimYvrhCB1";

const METEORA_USER_POSITION_DISCRIMINATOR: [u8; 8] =
    [0x9f, 0x75, 0x5f, 0xe3, 0xef, 0x97, 0x3a, 0xec];

const METEORA_FARM_DISCRIMINATOR: [u8; 8] = [0xf1, 0x9a, 0x6d, 0x04, 0x11, 0xb1, 0x6d, 0xbc];

pub struct MeteoraFarms;

#[async_trait]
impl TransactionParserTrait for MeteoraFarms {
    fn get_name(&self) -> &'static str {
        "MeteoraFarms"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            METEORA_FARMS_PROGRAM_ID.into(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let accts_iter = txn.iter_accounts().filter(|a| {
            a.post_owner_program
                .is_some_and(|pk| pk == METEORA_FARMS_PROGRAM_ID)
                && a.datum_changed
        });

        for acct in accts_iter {
            let Some(data) = acct.unpack_post_datum()? else {
                continue;
            };

            let Some(disc) = data.get(0..8).and_then(|a| a.try_into().ok()) else {
                continue;
            };

            match disc {
                METEORA_FARM_DISCRIMINATOR => {
                    let farm = meteora_farms_gen::meteora_farms::accounts::pool::deserialize(
                        &mut &data[8..],
                    )?;

                    let was_created = acct.pre_datum.is_none();
                    if was_created {
                        let farm_info = Dooot::FarmInfo(FarmInfoDooot {
                            deposit_token_mint: farm.staking_mint.to_string(),
                            deposit_token_vault: farm.staking_vault.to_string(),
                            reward_token_vaults: vec![
                                FarmInfoUnderlyingMintVault {
                                    mint: farm.reward_a_mint.to_string(),
                                    vault: farm.reward_a_vault.to_string(),
                                },
                                FarmInfoUnderlyingMintVault {
                                    mint: farm.reward_b_mint.to_string(),
                                    vault: farm.reward_b_vault.to_string(),
                                },
                            ],
                            farm_pubkey: acct.pubkey.to_string(),
                            program_pubkey: METEORA_FARMS_PROGRAM_ID.into(),
                        });

                        dooots.push(farm_info);
                    }

                    let global_state = Dooot::FarmGlobal(FarmGlobalDooot {
                        time,
                        farm_pubkey: acct.pubkey.to_string(),
                        per_unit_type: FarmPerUnitType::PerSecond,
                        last_updated: farm.last_update_time,
                        reward_end: vec![farm.reward_duration_end, farm.reward_duration_end],
                        reward_rates: vec![farm.reward_a_rate, farm.reward_b_rate],
                        reward_atoms_per_atom_stored: vec![
                            farm.reward_a_per_token_stored.to_string(),
                            farm.reward_b_per_token_stored.to_string(),
                        ],
                        total_qty_staked: farm.total_staked,
                        closed: false,
                    });

                    dooots.push(global_state);
                }
                METEORA_USER_POSITION_DISCRIMINATOR => {
                    let user = meteora_farms_gen::meteora_farms::accounts::user::deserialize(
                        &mut &data[8..],
                    )?;

                    let user_state = Dooot::FarmUser(FarmUserDooot {
                        time,
                        farm_pubkey: user.pool.to_string(),
                        user_pubkey: user.owner.to_string(),
                        position_pubkey: acct.pubkey.to_string(),
                        reward_atoms_pending: vec![
                            user.reward_a_per_token_pending,
                            user.reward_b_per_token_pending,
                        ],
                        reward_atoms_per_atom_complete: vec![
                            user.reward_a_per_token_complete.to_string(),
                            user.reward_b_per_token_complete.to_string(),
                        ],
                        qty_staked: user.balance_staked,
                        closed: false,
                    });

                    dooots.push(user_state);
                }
                _ => {
                    continue;
                }
            }
        }

        Ok(())
    }
}

impl StateSource for MeteoraFarms {
    fn get_name(&self) -> &'static str {
        "MeteoraFarms"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let program_accounts =
            acct_provider.program_accounts(Pubkey::from_str(METEORA_FARMS_PROGRAM_ID)?, None);

        for acct in program_accounts {
            let pk = acct.pubkey;
            let data = acct.account.data();

            let Some(disc) = data.get(0..8).and_then(|a| a.try_into().ok()) else {
                continue;
            };

            let mut gstate_dooots = vec![];
            let mut user_dooots = vec![];

            match disc {
                METEORA_FARM_DISCRIMINATOR => {
                    // Write lookup and gstate dooot
                    let farm = meteora_farms_gen::meteora_farms::accounts::pool::deserialize(
                        &mut &data[8..],
                    )?;

                    let farm_info = Dooot::FarmInfo(FarmInfoDooot {
                        deposit_token_mint: farm.staking_mint.to_string(),
                        deposit_token_vault: farm.staking_vault.to_string(),
                        reward_token_vaults: vec![
                            FarmInfoUnderlyingMintVault {
                                mint: farm.reward_a_mint.to_string(),
                                vault: farm.reward_a_vault.to_string(),
                            },
                            FarmInfoUnderlyingMintVault {
                                mint: farm.reward_b_mint.to_string(),
                                vault: farm.reward_b_vault.to_string(),
                            },
                        ],
                        farm_pubkey: pk.to_string(),
                        program_pubkey: METEORA_FARMS_PROGRAM_ID.into(),
                    });

                    tx.send(farm_info)?;

                    let global_state = Dooot::FarmGlobal(FarmGlobalDooot {
                        time,
                        farm_pubkey: acct.pubkey.to_string(),
                        per_unit_type: FarmPerUnitType::PerSecond,
                        last_updated: farm.last_update_time,
                        reward_end: vec![farm.reward_duration_end, farm.reward_duration_end],
                        reward_rates: vec![farm.reward_a_rate, farm.reward_b_rate],
                        reward_atoms_per_atom_stored: vec![
                            farm.reward_a_per_token_stored.to_string(),
                            farm.reward_b_per_token_stored.to_string(),
                        ],
                        total_qty_staked: farm.total_staked,
                        closed: false,
                    });

                    gstate_dooots.push(global_state);
                }
                METEORA_USER_POSITION_DISCRIMINATOR => {
                    // Write user dooot
                    let user = meteora_farms_gen::meteora_farms::accounts::user::deserialize(
                        &mut &data[8..],
                    )?;

                    let user_state = Dooot::FarmUser(FarmUserDooot {
                        time,
                        farm_pubkey: user.pool.to_string(),
                        user_pubkey: user.owner.to_string(),
                        position_pubkey: acct.pubkey.to_string(),
                        reward_atoms_pending: vec![
                            user.reward_a_per_token_pending,
                            user.reward_b_per_token_pending,
                        ],
                        reward_atoms_per_atom_complete: vec![
                            user.reward_a_per_token_complete.to_string(),
                            user.reward_b_per_token_complete.to_string(),
                        ],
                        qty_staked: user.balance_staked,
                        closed: false,
                    });

                    user_dooots.push(user_state);
                }
                _ => {
                    continue;
                }
            }

            for dooot in gstate_dooots {
                tx.send(dooot)?;
            }

            for dooot in user_dooots {
                tx.send(dooot)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use step_ingestooor_sdk::{
        decoded_transaction::DecodedTransaction, utils::test_utils::init_test_extras_bare,
    };

    use crate::{MeteoraFarms, TransactionParserTrait};

    use super::*;

    #[tokio::test]
    async fn test_meteora_farms() {
        let (utils, _) = init_test_extras_bare().await;
        let file_json = fs::read_to_string("tests/meteora_farms/stake.json").unwrap();
        let value = serde_json::from_str(&file_json).unwrap();

        let tx = DecodedTransaction::from_rabbit_message_json(value).unwrap();

        let mut dooots = vec![];
        MeteoraFarms.invoke(&tx, &mut dooots, &utils).await.unwrap();

        println!("{:?}", dooots);
        assert_eq!(dooots.len(), 2);
    }

    #[tokio::test]
    async fn test_withdraw_partial() {
        let (utils, _) = init_test_extras_bare().await;
        let file_json = fs::read_to_string("tests/meteora_farms/withdraw_partial.json").unwrap();
        let value = serde_json::from_str(&file_json).unwrap();

        let tx = DecodedTransaction::from_rabbit_message_json(value).unwrap();

        let mut dooots = vec![];
        MeteoraFarms.invoke(&tx, &mut dooots, &utils).await.unwrap();

        if let (Dooot::FarmGlobal(global), Dooot::FarmUser(user)) = (&dooots[0], &dooots[1]) {
            // user account is closed so rewards should be 0
            let current_time = 1752726417_u64;
            let rewards = calculate_pending_rewards(global, user, current_time);
            println!("{:?}", rewards);
        };

        println!("{:?}", dooots);
        assert_eq!(dooots.len(), 2);
    }

    fn calculate_pending_rewards(
        global: &FarmGlobalDooot,
        user: &FarmUserDooot,
        current_time: u64,
    ) -> Vec<u128> {
        let mut rewards: Vec<u128> = vec![];

        for (i, reward_rate) in global.reward_rates.iter().enumerate() {
            let last_updated = global.last_updated;
            let last_applicable = if current_time > global.reward_end[i] {
                global.reward_end[i]
            } else {
                current_time
            };
            let reward_atoms_per_atom_elapsed = ((last_applicable - last_updated) * reward_rate)
                as u128
                / global.total_qty_staked as u128;

            let reward_atoms_per_atom_stored = global.reward_atoms_per_atom_stored[i]
                .parse::<u128>()
                .unwrap();

            let total_reward_per_atom =
                reward_atoms_per_atom_stored + reward_atoms_per_atom_elapsed;

            let reward_atoms_per_atom_complete = user.reward_atoms_per_atom_complete[i]
                .parse::<u128>()
                .unwrap();
            let reward_owed = user.qty_staked as u128
                * (total_reward_per_atom - reward_atoms_per_atom_complete)
                / 1_000_000_000;

            let reward_atoms = reward_owed + user.reward_atoms_pending[i] as u128;

            rewards.push(reward_atoms);
        }

        rewards
    }
}
