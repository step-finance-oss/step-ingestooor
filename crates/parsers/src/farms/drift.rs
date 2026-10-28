use std::collections::{HashMap, HashSet};

use crate::state_source::{AccountProvider, StateSource};
use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::Result;
use async_trait::async_trait;
use borsh::BorshDeserialize;
use solana_sdk::account::ReadableAccount;
use solana_sdk::pubkey::Pubkey;
use step_ingestooor_idl_gens::drift_gen::drift::accounts::{InsuranceFundStake, SpotMarket};
use step_ingestooor_sdk::dooot::{Dooot, FarmGlobalDooot, FarmPerUnitType};
use step_ingestooor_sdk::dooot::{FarmInfoDooot, FarmUserDooot};
use step_ingestooor_sdk::trait_defs::transaction::AccountDatumIterItem;
use step_ingestooor_sdk::trait_defs::transaction::TransactionLike;
use step_ingestooor_sdk::utils::step_utils::StepUtils;
use step_ingestooor_sdk::utils::util_fns::convert_blocktime_to_micros;

pub struct DriftFarms;

pub const DRIFT_PROGRAM_ID: &str = "dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH";

pub const SPOT_MARKET_DISCRIMINATOR: [u8; 8] = [0x64, 0xb1, 0x08, 0x6b, 0xa8, 0x41, 0x41, 0x27];
pub const INSURANCE_FUND_STAKE_DISCRIMINATOR: [u8; 8] =
    [0x6e, 0xca, 0x0e, 0x2a, 0x5f, 0x49, 0x5a, 0x5f];

#[async_trait]
impl TransactionParserTrait for DriftFarms {
    fn get_name(&self) -> &'static str {
        "DriftFarms"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::AccountId(
            DRIFT_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> anyhow::Result<()> {
        let time = decoded_transaction.get_block_time();
        let accounts = decoded_transaction
            .iter_accounts_filtered(|a| a.pubkey == DRIFT_PROGRAM_ID)
            .filter(|a| a.datum_changed);

        let mut spot_market_datum: Option<AccountDatumIterItem<'_>> = None;
        let mut insurance_fund_stake_datum: Option<AccountDatumIterItem<'_>> = None;

        for account in accounts {
            let Some(post) = account.unpack_post_datum()? else {
                log::warn!("Drift post datum empty for account: {:?}", account.pubkey);
                continue;
            };
            let discriminator = &post[0..8];
            if discriminator == SPOT_MARKET_DISCRIMINATOR {
                spot_market_datum = Some(account);
            } else if discriminator == INSURANCE_FUND_STAKE_DISCRIMINATOR {
                insurance_fund_stake_datum = Some(account);
            }
        }

        if let Some(spot_market_datum) = spot_market_datum {
            let Some(post) = spot_market_datum.unpack_post_datum()? else {
                log::warn!("Drift post datum empty for spot market");
                return Err(anyhow::anyhow!("Drift spot_market_datum post datum empty"));
            };
            let post_spot_market = SpotMarket::deserialize(&mut &post[8..])?;
            if spot_market_datum.was_created {
                let farm_doot = FarmInfoDooot {
                    farm_pubkey: post_spot_market.pubkey.to_string(),
                    program_pubkey: DRIFT_PROGRAM_ID.to_string(),
                    deposit_token_mint: post_spot_market.mint.to_string(),
                    deposit_token_vault: post_spot_market.insurance_fund.vault.to_string(),
                    reward_token_vaults: vec![],
                };
                dooot.push(Dooot::FarmInfo(farm_doot));
            }

            //unpack pre datum but it can be empty
            let pre_spot_market = spot_market_datum.unpack_pre_datum()?;

            if spot_market_datum.was_created || pre_spot_market.is_some() {
                let post_vault = decoded_transaction
                    .get_account_data_after(&post_spot_market.insurance_fund.vault.to_string())?
                    .ok_or(anyhow::anyhow!("Vault post token account not found"))?;
                let vault_balance = u64::from_le_bytes(post_vault[64..72].try_into()?) as u128;

                // Check if we should emit GFarmState
                let should_emit = if spot_market_datum.was_created {
                    true // Always emit for new markets
                } else if let Some(pre_data) = pre_spot_market {
                    // Emit only if shares or vault balance changed
                    let pre_vault = decoded_transaction
                        .get_account_data_before(
                            &post_spot_market.insurance_fund.vault.to_string(),
                        )?
                        .ok_or(anyhow::anyhow!("Vault pre token account not found"))?;
                    let pre_spot_market = SpotMarket::deserialize(&mut &pre_data[8..])?;
                    let pre_vault_balance =
                        u64::from_le_bytes(pre_vault[64..72].try_into()?) as u128;
                    let post_vault_balance =
                        u64::from_le_bytes(post_vault[64..72].try_into()?) as u128;
                    post_spot_market.insurance_fund.total_shares
                        != pre_spot_market.insurance_fund.total_shares
                        || pre_vault_balance != post_vault_balance
                } else {
                    return Err(anyhow::anyhow!(
                        "Unreachable: Drift pre spot market datum empty for spot market"
                    ));
                };

                if should_emit {
                    let reward_rate = if post_spot_market.insurance_fund.total_shares > 0 {
                        vault_balance / post_spot_market.insurance_fund.total_shares
                    } else {
                        0
                    };
                    let farm_gstate_doot = FarmGlobalDooot {
                        time,
                        farm_pubkey: post_spot_market.pubkey.to_string(),
                        // DOUBLE CHECK
                        per_unit_type: FarmPerUnitType::PerSecond,
                        last_updated: 0,
                        reward_end: vec![],
                        reward_rates: vec![reward_rate as u64], // CHECK u128 CONVERSION
                        reward_atoms_per_atom_stored: vec![],
                        total_qty_staked: 0, // COME BACK AND FIX
                        closed: false,
                    };
                    dooot.push(Dooot::FarmGlobal(farm_gstate_doot));
                }
            }

            if let Some(insurance_fund_stake_datum) = insurance_fund_stake_datum {
                let Some(post) = insurance_fund_stake_datum.unpack_post_datum()? else {
                    log::warn!("Drift post datum empty");
                    return Ok(());
                };
                let insurance_fund_stake = InsuranceFundStake::deserialize(&mut &post[8..])?;
                let user_doot = FarmUserDooot {
                    time,
                    user_pubkey: insurance_fund_stake_datum.pubkey.to_string(),
                    farm_pubkey: post_spot_market.pubkey.to_string(),
                    position_pubkey: insurance_fund_stake_datum.pubkey.to_string(),
                    qty_staked: insurance_fund_stake.if_shares as u64, // CHECK u128 CONVERSION
                    reward_atoms_pending: vec![],
                    reward_atoms_per_atom_complete: vec![],
                    closed: false,
                };
                dooot.push(Dooot::FarmUser(user_doot));
            }
        }

        Ok(())
    }
}

impl StateSource for DriftFarms {
    fn get_name(&self) -> &'static str {
        "DriftFarms"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let drift_pk = DRIFT_PROGRAM_ID.parse::<Pubkey>()?;
        let drift_accounts = acct_provider.program_accounts(drift_pk, None);
        let mut vault_accounts_set = HashSet::new();
        let mut vault_accounts = HashMap::new();
        let mut spot_market_accounts = HashMap::new();

        let mut gstate_dooots = vec![];
        let mut user_dooots = vec![];
        // grouped by market index
        let mut insurance_fund_stake_accounts = HashMap::new();
        let mut insurance_fund_stake_pubkeys = HashMap::new();
        for account in drift_accounts {
            let data = account.account.data();
            let pubkey = account.pubkey;
            let Some(discrim) = data.get(..8) else {
                continue;
            };
            if discrim == SPOT_MARKET_DISCRIMINATOR {
                let spot_market = SpotMarket::deserialize(&mut &data[8..])?;
                vault_accounts_set.insert(spot_market.insurance_fund.vault.to_string());
                spot_market_accounts.insert(account.pubkey, spot_market);
            } else if discrim == INSURANCE_FUND_STAKE_DISCRIMINATOR {
                let insurance_fund_stake = InsuranceFundStake::deserialize(&mut &data[8..])?;
                // vec this
                insurance_fund_stake_accounts
                    .entry(insurance_fund_stake.market_index)
                    .or_insert(vec![])
                    .push(insurance_fund_stake);
                insurance_fund_stake_pubkeys.insert(insurance_fund_stake.market_index, pubkey);
            }
        }

        for account in acct_provider.all_accounts() {
            if vault_accounts_set.contains(&account.pubkey.to_string()) {
                let data = account.account.data();
                let balance_bytes = &data[64..72].try_into()?;
                let balance = u64::from_le_bytes(*balance_bytes) as u128;
                vault_accounts.insert(account.pubkey, balance);
            };
        }

        for (spot_market_key, spot_market) in spot_market_accounts {
            let farm_doot = FarmInfoDooot {
                farm_pubkey: spot_market_key.to_string(),
                program_pubkey: DRIFT_PROGRAM_ID.to_string(),
                deposit_token_mint: spot_market.mint.to_string(),
                deposit_token_vault: spot_market.insurance_fund.vault.to_string(),
                reward_token_vaults: vec![],
            };
            tx.send(Dooot::FarmInfo(farm_doot))?;

            let vault_balance = *vault_accounts
                .get(&spot_market.insurance_fund.vault)
                .expect("vault balance not found");

            let reward_rate = if spot_market.insurance_fund.total_shares > 0 {
                vault_balance / spot_market.insurance_fund.total_shares
            } else {
                0
            };

            let farm_gstate_doot = FarmGlobalDooot {
                time,
                farm_pubkey: spot_market_key.to_string(),
                // DOUBLE CHECK
                per_unit_type: FarmPerUnitType::PerSecond,
                last_updated: 0, // NEED TO FIND THIS OR MAKE IT ON OPTION
                reward_end: vec![],
                reward_rates: vec![reward_rate as u64],
                reward_atoms_per_atom_stored: vec![],
                total_qty_staked: 0, // COME BACK AND FIX
                closed: false,
            };
            gstate_dooots.push(Dooot::FarmGlobal(farm_gstate_doot));

            let insurance_fund_stake_accounts =
                insurance_fund_stake_accounts.get(&spot_market.market_index);
            if let Some(insurance_fund_stake_accounts) = insurance_fund_stake_accounts {
                for insurance_fund_stake in insurance_fund_stake_accounts {
                    let user_doot = FarmUserDooot {
                        time,
                        user_pubkey: insurance_fund_stake.authority.to_string(),
                        farm_pubkey: spot_market_key.to_string(),
                        position_pubkey: insurance_fund_stake_pubkeys
                            .get(&spot_market.market_index)
                            .expect("position pubkey not found")
                            .to_string(),
                        qty_staked: insurance_fund_stake.if_shares as u64, // CHECK u128 CONVERSION
                        reward_atoms_pending: vec![],
                        reward_atoms_per_atom_complete: vec![],
                        closed: false,
                    };
                    user_dooots.push(Dooot::FarmUser(user_doot));
                }
            }
        }

        for dooot in gstate_dooots {
            tx.send(dooot)?;
        }

        for dooot in user_dooots {
            tx.send(dooot)?;
        }

        Ok(())
    }
}
