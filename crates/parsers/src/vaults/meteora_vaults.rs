use std::array::TryFromSliceError;
use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use borsh::BorshDeserialize;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::{account::ReadableAccount, pubkey};
use step_ingestooor_idl_gens::meteora_vaults_gen::meteora_vaults::accounts::Vault;

use step_ingestooor_sdk::dooot::{VaultInfoDooot, VaultInfoUnderlyingMintVault};
use step_ingestooor_sdk::{
    dooot::{Dooot, MintUnderlyingsGlobalDooot},
    trait_defs::transaction::TransactionLike,
    utils::{step_utils::StepUtils, util_fns::convert_blocktime_to_micros},
};

use crate::{state_source::StateSource, TransactionParserFilter, TransactionParserTrait};

pub const METEORA_VAULTS_PROGRAM_ID: &str = "24Uqj9JCLxUeoC3hGfh5W3s9FM9uCHDS2SG3LYwBpyTi";
pub const METEORA_VAULTS_PROGRAM_ID_PK: Pubkey =
    pubkey!("24Uqj9JCLxUeoC3hGfh5W3s9FM9uCHDS2SG3LYwBpyTi");
pub const VAULT_ACCOUNT_DISCRIMINATOR: [u8; 8] = [0xd3, 0x08, 0xe8, 0x2b, 0x02, 0x98, 0x75, 0x77];

pub struct MeteoraVaults;

#[async_trait]
impl TransactionParserTrait for MeteoraVaults {
    fn get_name(&self) -> &'static str {
        "MeteoraVaults"
    }

    fn get_filters(&self) -> Vec<crate::TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            METEORA_VAULTS_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let blocktime = time.and_utc().timestamp() as u64;
        let changed_vaults = txn.iter_accounts().filter(|a| {
            a.datum_changed
                && a.post_owner_program
                    .is_some_and(|p| p == METEORA_VAULTS_PROGRAM_ID)
        });

        for acct in changed_vaults {
            let Some(post) = acct.unpack_post_datum()? else {
                log::warn!(
                    "No post datum found for Meteora Vaults. Possibly closed vault account: {}",
                    txn.get_signature()
                );
                continue;
            };

            let Some(disc) = post.get(..8) else {
                continue;
            };

            if disc != VAULT_ACCOUNT_DISCRIMINATOR {
                continue;
            }

            let vault = Vault::deserialize(&mut &post[8..])?;
            let vtoken_mint = vault.lp_mint.to_string();
            let underlying_mint = vault.token_mint.to_string();
            let underlying_vault = vault.token_vault.to_string();

            let vault_info = Dooot::VaultInfo(VaultInfoDooot {
                time,
                deposit_tokens: vec![VaultInfoUnderlyingMintVault {
                    mint: underlying_mint.clone(),
                    vault: underlying_vault,
                }],
                reward_tokens: vec![],
                vault_mint: Some(vtoken_mint.clone()),
                vault_pubkey: acct.pubkey.to_string(),
                program_pubkey: METEORA_VAULTS_PROGRAM_ID.to_string(),
                name: None,
            });

            dooot.push(vault_info);

            let Some(vtoken_supply) = txn
                .get_account_data_after(&vtoken_mint)
                .ok()
                .flatten()
                .map(|b| {
                    let supply_bytes = b[36..44].try_into()?;
                    Ok(u64::from_le_bytes(supply_bytes))
                })
                .transpose()
                .map_err(|e: TryFromSliceError| anyhow!(e))?
            else {
                log::warn!("vToken supply not found");
                continue;
            };

            let withdrawable =
                Self::get_withdrawable_amount(&vault, blocktime).context(format!(
                    "Cannot get withdrawbale amount from Meteora Vaults? Vault: {}\n{}",
                    acct.pubkey,
                    txn.get_signature()
                ))?;

            let amt_per_parent = if vtoken_supply == 0 {
                0.0
            } else {
                (withdrawable as f64) / (vtoken_supply as f64)
            };

            let mint_underlying_dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                time,
                mint_pubkey: vtoken_mint,
                platform_program_pubkey: METEORA_VAULTS_PROGRAM_ID.to_string(),
                mints: vec![underlying_mint],
                mints_qty_per_one_parent: vec![amt_per_parent],
                total_underlying_amounts: vec![withdrawable.into()],
                discriminant_id: acct.pubkey.to_string(),
            });

            dooot.push(mint_underlying_dooot);
        }

        Ok(())
    }
}

impl StateSource for MeteoraVaults {
    fn get_name(&self) -> &'static str {
        "MeteoraVaults"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let blocktime = acct_provider.get_block_time();
        let time = convert_blocktime_to_micros(blocktime, Some(0), None)?;

        let mut lp_supplies = HashMap::new();
        let mut vaults = HashMap::new();

        let vaults_iter = acct_provider.program_accounts(
            METEORA_VAULTS_PROGRAM_ID_PK,
            Some(&VAULT_ACCOUNT_DISCRIMINATOR),
        );

        for vault_acct in vaults_iter {
            let data = vault_acct.account.data();
            let pk = vault_acct.pubkey;
            let vault = Vault::deserialize(&mut &data[8..])?;
            let lp_mint = vault.lp_mint;
            let underlying_mint = vault.token_mint;
            let underlying_vault = vault.token_vault;

            let vault_info = Dooot::VaultInfo(VaultInfoDooot {
                time,
                deposit_tokens: vec![VaultInfoUnderlyingMintVault {
                    mint: underlying_mint.to_string(),
                    vault: underlying_vault.to_string(),
                }],
                reward_tokens: vec![],
                vault_mint: Some(lp_mint.to_string()),
                vault_pubkey: pk.to_string(),
                program_pubkey: METEORA_VAULTS_PROGRAM_ID.to_string(),
                name: None,
            });

            tx.send(vault_info)?;

            lp_supplies.insert(lp_mint, 0);
            vaults.insert(pk, vault);
        }

        let all_mints_iter = acct_provider.all_accounts();

        for acct in all_mints_iter {
            let pk = acct.pubkey;
            if !lp_supplies.contains_key(&pk) {
                continue;
            }

            let data = acct.account.data();
            let supply_bytes = data[36..44].try_into()?;
            let supply = u64::from_le_bytes(supply_bytes);
            lp_supplies.insert(pk, supply);
        }

        for (pk, vault) in vaults {
            let Some(withdrawable) = Self::get_withdrawable_amount(&vault, blocktime as u64) else {
                log::warn!(
                    "Cannot get withdrawable amount from Meteora Vaults? Vault: {}",
                    pk
                );
                continue;
            };

            let Some(vtoken_supply) = lp_supplies.get(&vault.lp_mint) else {
                log::warn!("vToken supply not found for vault: {}", pk);
                continue;
            };

            let amt_per_parent = if *vtoken_supply == 0 {
                0.0
            } else {
                (withdrawable as f64) / (*vtoken_supply as f64)
            };

            let mint_underlying_dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                time,
                mint_pubkey: vault.lp_mint.to_string(),
                platform_program_pubkey: METEORA_VAULTS_PROGRAM_ID.to_string(),
                mints: vec![vault.token_mint.to_string()],
                mints_qty_per_one_parent: vec![amt_per_parent],
                total_underlying_amounts: vec![withdrawable.into()],
                discriminant_id: pk.to_string(),
            });

            tx.send(mint_underlying_dooot)?
        }

        Ok(())
    }
}

impl MeteoraVaults {
    pub const LOCKED_PROFIT_DEGRADATION_DENOMINATOR: u128 = 1_000_000_000_000;

    /// Ripped from https://github.com/mercurial-finance/mercurial-dynamic-amm-sdk/blob/master/programs/dynamic-vault/src/state.rs#L60
    pub fn get_withdrawable_amount(vault: &Vault, current_time: u64) -> Option<u64> {
        let duration =
            u128::from(current_time.checked_sub(vault.locked_profit_tracker.last_report)?);
        let locked_profit_degradation =
            u128::from(vault.locked_profit_tracker.locked_profit_degradation);
        let locked_fund_ratio = duration.checked_mul(locked_profit_degradation)?;

        if locked_fund_ratio > Self::LOCKED_PROFIT_DEGRADATION_DENOMINATOR {
            return Some(vault.total_amount);
        }
        let locked_profit = u128::from(vault.locked_profit_tracker.last_updated_locked_profit);

        let locked_profit = (locked_profit
            .checked_mul(Self::LOCKED_PROFIT_DEGRADATION_DENOMINATOR - locked_fund_ratio)?)
        .checked_div(Self::LOCKED_PROFIT_DEGRADATION_DENOMINATOR)?;
        let locked_profit = u64::try_from(locked_profit).ok()?;

        vault.total_amount.checked_sub(locked_profit)
    }

    pub fn derive_vault_ata_pk(vault_key: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[b"token_vault", vault_key.as_ref()],
            &METEORA_VAULTS_PROGRAM_ID_PK,
        )
        .0
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use step_ingestooor_sdk::{
        decoded_transaction::DecodedTransaction, utils::test_utils::init_test_extras_bare,
    };

    use super::*;

    #[test]
    fn test_derive_vault_ata_pk() {
        let vault_key = pubkey!("FERjPVNEa7Udq8CEv68h6tPL46Tq7ieE49HrE2wea3XT");
        let expected_key = pubkey!("HZeLxbZ9uHtSpwZC3LBr4Nubd14iHwz7bRSghRZf5VCG");

        assert_eq!(MeteoraVaults::derive_vault_ata_pk(&vault_key), expected_key);
    }

    #[tokio::test]
    async fn parser_invoke() {
        let (utils, _) = init_test_extras_bare().await;
        let txn = serde_json::from_str::<DecodedTransaction>(
            fs::read_to_string("./tests/meteora_vaults/meteora_vaults.json")
                .unwrap()
                .as_str(),
        )
        .unwrap();

        let mut dooot = vec![];
        let parser = MeteoraVaults;
        parser.invoke(&txn, &mut dooot, &utils).await.unwrap();
    }
}
