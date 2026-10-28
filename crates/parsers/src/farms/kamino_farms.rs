use anyhow::Result;
use async_trait::async_trait;
use borsh::BorshDeserialize;
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
use std::str::FromStr;
use step_ingestooor_idl_gens::kamino_farms_gen::kamino_farms::accounts::{farmState, userState};
use step_ingestooor_sdk::{
    dooot::{
        Dooot, FarmGlobalDooot, FarmInfoDooot, FarmInfoUnderlyingMintVault, FarmPerUnitType,
        FarmUserDooot,
    },
    trait_defs::transaction::TransactionLike,
    utils::{step_utils::StepUtils, util_fns::convert_blocktime_to_micros},
};

use crate::{state_source::StateSource, TransactionParserFilter, TransactionParserTrait};

pub struct KaminoFarms;

const KAMINO_FARMS_PROGRAM_ID: &str = "FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr";

const KAMINO_FARM_STATE_DISC: &[u8] = &[0xc6, 0x66, 0xd8, 0x4a, 0x3f, 0x42, 0xa3, 0xbe];
const KAMINO_USER_STATE_DISC: &[u8] = &[0x48, 0xb1, 0x55, 0xf9, 0x4c, 0xa7, 0xba, 0x7e];

#[async_trait]
impl TransactionParserTrait for KaminoFarms {
    fn get_name(&self) -> &'static str {
        "KaminoFarms"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::AccountId(
            KAMINO_FARMS_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> anyhow::Result<()> {
        let time = decoded_transaction.get_block_time();
        let accts_iter = decoded_transaction.iter_accounts().filter(|a| {
            a.post_owner_program
                .is_some_and(|pk| pk == KAMINO_FARMS_PROGRAM_ID)
        });

        for acct in accts_iter {
            let Some(post) = acct.unpack_post_datum()? else {
                log::warn!("Kamino post datum empty for account: {:?}", acct.pubkey);
                continue;
            };

            let was_created = acct.pre_datum.is_none();
            let discriminator = &post[0..8];

            match discriminator {
                KAMINO_FARM_STATE_DISC => {
                    let kamino_farm_state =
                        farmState::deserialize(&mut &post[8..]).map_err(|e| {
                            anyhow::anyhow!("Failed to deserialize Kamino farmState: {}", e)
                        })?;

                    if was_created {
                        let reward_token_vaults = kamino_farm_state
                            .reward_infos
                            .iter()
                            .filter(|reward_info| {
                                reward_info.token.mint.to_string()
                                    == "11111111111111111111111111111111"
                            })
                            .map(|reward_info| FarmInfoUnderlyingMintVault {
                                mint: reward_info.token.mint.to_string(),
                                vault: reward_info.rewards_vault.to_string(),
                            })
                            .collect();
                        let farm_info_doot = Dooot::FarmInfo(FarmInfoDooot {
                            farm_pubkey: acct.pubkey.to_string(),
                            program_pubkey: KAMINO_FARMS_PROGRAM_ID.to_string(),
                            deposit_token_mint: kamino_farm_state.token.mint.to_string(),
                            deposit_token_vault: kamino_farm_state.farm_vault.to_string(),
                            reward_token_vaults,
                        });
                        dooots.push(farm_info_doot);
                    }

                    let reward_infos: Vec<&_> = kamino_farm_state
                        .reward_infos
                        .iter()
                        .filter(|reward_info| {
                            reward_info.token.mint.to_string() == "11111111111111111111111111111111"
                        })
                        .collect();

                    let reward_rates: Vec<u64> = reward_infos
                        .iter()
                        .map(|reward_info| {
                            reward_info
                                .reward_schedule_curve
                                .points
                                .iter()
                                .rfind(|point| point.reward_per_time_unit > 0)
                                .map(|point| point.reward_per_time_unit)
                                .unwrap_or_else(|| 0)
                        })
                        .collect();

                    let reward_atoms_per_atom_stored: Vec<String> = reward_infos
                        .iter()
                        .enumerate()
                        .filter_map(|(index, reward_info)| {
                            reward_rates
                                .get(index)
                                .and_then(|rr| rr.checked_div(reward_info.rewards_available))
                                .map(|dec| dec.to_string())
                        })
                        .collect();

                    let farm_gstate_doot = Dooot::FarmGlobal(FarmGlobalDooot {
                        time,
                        farm_pubkey: acct.pubkey.to_string(),
                        per_unit_type: FarmPerUnitType::PerSecond,
                        last_updated: 0, // USE reward_infos.last_issuance[0] since it is the same for all active reward infos *assumption*
                        reward_end: vec![],
                        reward_rates,
                        reward_atoms_per_atom_stored,
                        total_qty_staked: 0, // COME BACK AND FIX
                        closed: false,
                    });
                    dooots.push(farm_gstate_doot);
                }

                KAMINO_USER_STATE_DISC => {
                    let kamino_user_state =
                        userState::deserialize(&mut &post[8..]).map_err(|e| {
                            anyhow::anyhow!("Failed to deserialize Kamino userState: {}", e)
                        })?;

                    let reward_atoms_pending: Vec<u64> = kamino_user_state
                        .rewards_issued_unclaimed
                        .iter()
                        .filter(|&&x| x > 0)
                        .copied()
                        .collect();

                    let farm_ustate_dooot = Dooot::FarmUser(FarmUserDooot {
                        time,
                        farm_pubkey: kamino_user_state.farm_state.to_string(),
                        user_pubkey: kamino_user_state.owner.to_string(),
                        position_pubkey: acct.pubkey.to_string(),
                        qty_staked: kamino_user_state.active_stake_scaled as u64,
                        reward_atoms_per_atom_complete: vec![], // Not provided for the user
                        reward_atoms_pending,
                        closed: false,
                    });
                    dooots.push(farm_ustate_dooot);
                }

                _ => continue,
            }
        }

        Ok(())
    }
}

impl StateSource for KaminoFarms {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let program_accounts =
            acct_provider.program_accounts(Pubkey::from_str(KAMINO_FARMS_PROGRAM_ID)?, None);

        for acct in program_accounts {
            let data = acct.account.data();
            let discriminator = &data[..8];

            match discriminator {
                KAMINO_FARM_STATE_DISC => {
                    let Ok(kamino_farm_state) = farmState::deserialize(&mut &data[8..])
                        .map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to deserialize Kamino farmState {}: {}",
                                acct.pubkey,
                                e
                            )
                        })
                        .inspect_err(|e| log::error!("{e}"))
                    else {
                        continue;
                    };
                    let reward_token_vaults = kamino_farm_state
                        .reward_infos
                        .iter()
                        .filter(|reward_info| {
                            reward_info.token.mint.to_string() == "11111111111111111111111111111111"
                        })
                        .map(|reward_info| FarmInfoUnderlyingMintVault {
                            mint: reward_info.token.mint.to_string(),
                            vault: reward_info.rewards_vault.to_string(),
                        })
                        .collect();
                    let farm_info_doot = Dooot::FarmInfo(FarmInfoDooot {
                        farm_pubkey: acct.pubkey.to_string(),
                        program_pubkey: KAMINO_FARMS_PROGRAM_ID.to_string(),
                        deposit_token_mint: kamino_farm_state.token.mint.to_string(),
                        deposit_token_vault: kamino_farm_state.farm_vault.to_string(),
                        reward_token_vaults,
                    });
                    tx.send(farm_info_doot)?;

                    let reward_infos: Vec<&_> = kamino_farm_state
                        .reward_infos
                        .iter()
                        .filter(|reward_info| {
                            reward_info.token.mint.to_string() == "11111111111111111111111111111111"
                        })
                        .collect();

                    let reward_rates: Vec<u64> = reward_infos
                        .iter()
                        .map(|reward_info| {
                            reward_info
                                .reward_schedule_curve
                                .points
                                .iter()
                                .rfind(|point| point.reward_per_time_unit > 0)
                                .map(|point| point.reward_per_time_unit)
                                .unwrap_or_else(|| 0)
                        })
                        .collect();

                    let reward_atoms_per_atom_stored: Vec<String> = reward_infos
                        .iter()
                        .enumerate()
                        .filter_map(|(index, reward_info)| {
                            reward_rates
                                .get(index)
                                .and_then(|rr| rr.checked_div(reward_info.rewards_available))
                                .map(|dec| dec.to_string())
                        })
                        .collect();

                    let farm_gstate_doot = Dooot::FarmGlobal(FarmGlobalDooot {
                        time,
                        farm_pubkey: acct.pubkey.to_string(),
                        per_unit_type: FarmPerUnitType::PerSecond,
                        last_updated: 0,
                        reward_end: vec![],
                        reward_rates,
                        reward_atoms_per_atom_stored,
                        total_qty_staked: 0, // COME BACK AND FIX
                        closed: false,
                    });
                    tx.send(farm_gstate_doot)?;
                }

                KAMINO_USER_STATE_DISC => {
                    let Ok(kamino_user_state) = userState::deserialize(&mut &data[8..])
                        .map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to deserialize Kamino userState {}: {}",
                                acct.pubkey,
                                e
                            )
                        })
                        .inspect_err(|e| log::error!("{e}"))
                    else {
                        continue;
                    };

                    let reward_atoms_pending: Vec<u64> = kamino_user_state
                        .rewards_issued_unclaimed
                        .iter()
                        .filter(|&&x| x > 0)
                        .copied()
                        .collect();

                    let farm_ustate_dooot = Dooot::FarmUser(FarmUserDooot {
                        time,
                        farm_pubkey: kamino_user_state.farm_state.to_string(),
                        user_pubkey: kamino_user_state.owner.to_string(),
                        position_pubkey: acct.pubkey.to_string(),
                        qty_staked: kamino_user_state.active_stake_scaled as u64,
                        reward_atoms_per_atom_complete: vec![],
                        reward_atoms_pending,
                        closed: false,
                    });
                    tx.send(farm_ustate_dooot)?;
                }

                _ => continue,
            }
        }

        Ok(())
    }

    fn get_name(&self) -> &'static str {
        "KaminoFarms"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, rc::Rc};
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;

    #[tokio::test]
    async fn invoke_test() {
        let path = "./tests/kamino/farm_deposit.json";
        let message_json = fs::read_to_string(path).unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = KaminoFarms;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 2);
    }
}
