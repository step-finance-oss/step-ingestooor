use std::str::FromStr;

use crate::{
    amm::dlmm::meteora::{BinArray, MeteoraDLMM, PositionV2},
    state_source::StateSource,
    TransactionParserFilter, TransactionParserTrait,
};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use borsh::BorshDeserialize;
use rust_decimal::{prelude::ToPrimitive, Decimal};
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
use step_ingestooor_idl_gens::{
    kamino_vaults_gen::{self, kamino_vaults::accounts::WhirlpoolStrategy},
    orca_whirlies_gen::{self},
    raydium_clmm_gen,
};
use step_ingestooor_sdk::{
    dooot::{Dooot, MintUnderlyingsGlobalDooot, VaultInfoDooot, VaultInfoUnderlyingMintVault},
    trait_defs::transaction::TransactionLike,
    utils::{
        step_utils::StepUtils,
        util_fns::{convert_blocktime_to_micros, unpack_mint_any_size},
    },
};

pub const KAMINO_VAULTS_PROGRAM_ID: &str = "6LtLpnUFNByNXLyCoK9wA2MykKAmQNZKBdY8s47dehDc";
pub const KAMINO_VAULTS_STRATEGY_DISCRIMINATOR: &[u8] =
    &[0xbe, 0xb2, 0xe7, 0xb8, 0x31, 0xba, 0x67, 0x0d];

pub struct KaminoVaults;

#[async_trait]
impl TransactionParserTrait for KaminoVaults {
    fn get_name(&self) -> &'static str {
        "Kamino Vaults"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            KAMINO_VAULTS_PROGRAM_ID.into(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let accts = txn
            .iter_accounts()
            .filter(|a| a.datum_changed && a.post_owner_program == Some(KAMINO_VAULTS_PROGRAM_ID));
        for acct in accts {
            let Some(data) = acct.unpack_post_datum()? else {
                log::warn!("KAMINO VAULTS: Missing post datum (could be closed)");
                continue;
            };

            if !data.starts_with(KAMINO_VAULTS_STRATEGY_DISCRIMINATOR) {
                // this is not a strategy account
                continue;
            }

            let strategy =
                kamino_vaults_gen::kamino_vaults::accounts::WhirlpoolStrategy::deserialize(
                    &mut &data[8..],
                )?;

            let pool_addr = strategy.pool;
            let position_addr = strategy.position;
            let token_a_mint = strategy.token_a_mint;
            let token_b_mint = strategy.token_b_mint;
            let vault_a_addr = strategy.token_a_vault;
            let vault_b_addr = strategy.token_b_vault;
            let shares_mint = strategy.shares_mint;
            let tick_array_lower = strategy.tick_array_lower;
            let tick_array_upper = strategy.tick_array_upper;

            let vault_info = Dooot::VaultInfo(VaultInfoDooot {
                time,
                deposit_tokens: vec![
                    VaultInfoUnderlyingMintVault {
                        mint: token_a_mint.to_string(),
                        vault: vault_a_addr.to_string(),
                    },
                    VaultInfoUnderlyingMintVault {
                        mint: token_b_mint.to_string(),
                        vault: vault_b_addr.to_string(),
                    },
                ],
                reward_tokens: vec![],
                vault_mint: Some(shares_mint.to_string()),
                vault_pubkey: acct.pubkey.to_string(),
                program_pubkey: KAMINO_VAULTS_PROGRAM_ID.to_string(),
                name: None,
            });

            dooot.push(vault_info);

            let Some(pool_data) = txn
                .get_account_data_after(&pool_addr.to_string())
                .ok()
                .flatten()
            else {
                // missing pool data, not involved in this txn
                continue;
            };

            let Some(position_data) = txn
                .get_account_data_after(&position_addr.to_string())
                .ok()
                .flatten()
            else {
                // missing position data, not involved in this txn
                continue;
            };

            let mut mint_underlying_rows = MintUnderlyingsGlobalDooot {
                mint_pubkey: shares_mint.to_string(),
                platform_program_pubkey: KAMINO_VAULTS_PROGRAM_ID.into(),
                mints: vec![],
                mints_qty_per_one_parent: vec![],
                time,
                total_underlying_amounts: vec![],
                discriminant_id: acct.pubkey.to_string(),
            };

            let Some(vault_a_post_balance) = txn
                .get_post_token_account_balance(&vault_a_addr.to_string())
                .ok()
                .flatten()
            else {
                continue;
            };
            let Some(vault_b_post_balance) = txn
                .get_post_token_account_balance(&vault_b_addr.to_string())
                .ok()
                .flatten()
            else {
                continue;
            };

            let mut total_a: u64 = vault_a_post_balance;
            let mut total_b: u64 = vault_b_post_balance;

            let Some(shares_mint_data) = txn
                .get_account_data_after(&shares_mint.to_string())
                .ok()
                .flatten()
            else {
                continue;
            };
            let ktoken_supply = unpack_mint_any_size(&shares_mint_data)?.supply;

            let kamino_underlying_info =
                KaminoUnderlyingPlatformInfo::try_pool_and_position_from_slice(
                    &pool_data[8..],
                    &position_data[8..],
                )?;

            match kamino_underlying_info {
                KaminoUnderlyingPlatformInfo::MeteoraInfo(position) => {
                    let Some(lower_bin_array_data) =
                        txn.get_account_data_after(&tick_array_lower.to_string())?
                    else {
                        log::warn!(
                            "KAMINO VAULTS: Missing lower bin array for sig: {}",
                            txn.get_signature()
                        );
                        continue;
                    };

                    let lower_bin_array = BinArray::deserialize(&mut &lower_bin_array_data[8..])
                        .map_err(|e| anyhow!(e))?;

                    let Some(upper_bin_array_data) =
                        txn.get_account_data_after(&tick_array_upper.to_string())?
                    else {
                        log::warn!(
                            "KAMINO VAULTS: Missing upper bin array for sig: {}",
                            txn.get_signature()
                        );
                        continue;
                    };

                    let upper_bin_array = BinArray::deserialize(&mut &upper_bin_array_data[8..])
                        .map_err(|e| anyhow!(e))?;

                    let bin_arrays = [&lower_bin_array, &upper_bin_array];

                    let (token_a, token_b) = MeteoraDLMM::get_token_holdings_for_position(
                        bin_arrays.as_slice(),
                        position,
                    );
                    total_a += token_a;
                    total_b += token_b;
                }
                KaminoUnderlyingPlatformInfo::OrcaInfo(boxed_state) => {
                    let position = &boxed_state.1;
                    let position_mint = position.position_mint.to_string();
                    let position_qty = Decimal::ONE;

                    mint_underlying_rows.mints.push(position_mint);

                    let pos_qty_per_parent = if ktoken_supply == 0 {
                        0.0
                    } else {
                        (position_qty / Decimal::from(ktoken_supply))
                            .to_f64()
                            .ok_or_else(|| anyhow!("Failed to convert position qty to f64"))?
                    };

                    mint_underlying_rows
                        .mints_qty_per_one_parent
                        .push(pos_qty_per_parent);
                    mint_underlying_rows
                        .total_underlying_amounts
                        .push(position_qty);
                }
                KaminoUnderlyingPlatformInfo::RaydiumInfo(boxed_state) => {
                    let position = &boxed_state.1;
                    let position_mint = position.nft_mint.to_string();
                    let position_qty = Decimal::ONE;

                    mint_underlying_rows.mints.push(position_mint);

                    let pos_qty_per_parent = if ktoken_supply == 0 {
                        0.0
                    } else {
                        (position_qty / Decimal::from(ktoken_supply))
                            .to_f64()
                            .ok_or_else(|| anyhow!("Failed to convert position qty to f64"))?
                    };

                    mint_underlying_rows
                        .mints_qty_per_one_parent
                        .push(pos_qty_per_parent);
                    mint_underlying_rows
                        .total_underlying_amounts
                        .push(position_qty);
                }
            }

            mint_underlying_rows.mints.push(token_a_mint.to_string());
            mint_underlying_rows.mints.push(token_b_mint.to_string());

            let (amt_token_a_per_parent, amt_token_b_per_parent) = if ktoken_supply == 0 {
                (0.0, 0.0)
            } else {
                (
                    (total_a as f64) / (ktoken_supply as f64),
                    (total_b as f64) / (ktoken_supply as f64),
                )
            };

            mint_underlying_rows
                .mints_qty_per_one_parent
                .push(amt_token_a_per_parent);
            mint_underlying_rows
                .mints_qty_per_one_parent
                .push(amt_token_b_per_parent);

            mint_underlying_rows
                .total_underlying_amounts
                .push(Decimal::from(total_a));
            mint_underlying_rows
                .total_underlying_amounts
                .push(Decimal::from(total_b));

            dooot.push(Dooot::MintUnderlyingsGlobal(mint_underlying_rows));
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum KaminoUnderlyingPlatformInfo<'a> {
    MeteoraInfo(&'a PositionV2),
    OrcaInfo(
        Box<(
            orca_whirlies_gen::orca_whirlies::accounts::Whirlpool,
            orca_whirlies_gen::orca_whirlies::accounts::Position,
        )>,
    ),
    RaydiumInfo(
        Box<(
            raydium_clmm_gen::raydium_clmm::accounts::PoolState,
            raydium_clmm_gen::raydium_clmm::accounts::PersonalPositionState,
        )>,
    ),
}

impl<'a> KaminoUnderlyingPlatformInfo<'a> {
    pub fn try_pool_and_position_from_slice(
        pool_bytes: &[u8],
        position_bytes: &'a [u8],
    ) -> Result<Self> {
        // let meteora_pool = meteora_dlmm_gen::LbPair::try_from_slice(pool_bytes);
        let meteora_position = MeteoraDLMM::position_v2_reader(position_bytes)
            .context("Failed to deserialize meteora position");
        if let Ok(meteora_position) = meteora_position {
            // Allocate a new PositionV2 in a Box with the same lifetime as position_bytes
            let position_copy = Box::new(meteora_position);
            // Leak the Box to get a static reference (not generally recommended)
            let position_ref: &'a PositionV2 = Box::leak(position_copy);
            return Ok(KaminoUnderlyingPlatformInfo::MeteoraInfo(position_ref));
        }

        let orca_pool =
            orca_whirlies_gen::orca_whirlies::accounts::Whirlpool::try_from_slice(pool_bytes);
        let orca_position =
            orca_whirlies_gen::orca_whirlies::accounts::Position::try_from_slice(position_bytes);
        if let (Ok(orca_pool), Ok(orca_position)) = (orca_pool, orca_position) {
            return Ok(KaminoUnderlyingPlatformInfo::OrcaInfo(Box::new((
                orca_pool,
                orca_position,
            ))));
        }

        let raydium_pool =
            raydium_clmm_gen::raydium_clmm::accounts::PoolState::try_from_slice(pool_bytes);
        let raydium_position =
            raydium_clmm_gen::raydium_clmm::accounts::PersonalPositionState::try_from_slice(
                position_bytes,
            );
        if let (Ok(raydium_pool), Ok(raydium_position)) = (raydium_pool, raydium_position) {
            return Ok(KaminoUnderlyingPlatformInfo::RaydiumInfo(Box::new((
                raydium_pool,
                raydium_position,
            ))));
        }

        Err(anyhow!(
            "Unknown Kamino platform variant. Perhaps Kamino now supports a new type of CLMM?"
        ))
    }
}

impl StateSource for KaminoVaults {
    fn get_name(&self) -> &'static str {
        "KaminoVaults"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let blocktime = acct_provider.get_block_time();
        let time = convert_blocktime_to_micros(blocktime, Some(0), None)?;
        let vaults_iter = acct_provider.program_accounts(
            Pubkey::from_str(KAMINO_VAULTS_PROGRAM_ID)?,
            Some(KAMINO_VAULTS_STRATEGY_DISCRIMINATOR),
        );

        for acct in vaults_iter {
            let data = acct.account.data();
            let Ok(vault) = WhirlpoolStrategy::deserialize(&mut &data[8..]) else {
                continue;
            };

            let token_a_mint = vault.token_a_mint;
            let token_b_mint = vault.token_b_mint;
            let vault_a_addr = vault.token_a_vault;
            let vault_b_addr = vault.token_b_vault;
            let shares_mint = vault.shares_mint;

            let vault_info = Dooot::VaultInfo(VaultInfoDooot {
                time,
                deposit_tokens: vec![
                    VaultInfoUnderlyingMintVault {
                        mint: token_a_mint.to_string(),
                        vault: vault_a_addr.to_string(),
                    },
                    VaultInfoUnderlyingMintVault {
                        mint: token_b_mint.to_string(),
                        vault: vault_b_addr.to_string(),
                    },
                ],
                reward_tokens: vec![],
                vault_mint: Some(shares_mint.to_string()),
                vault_pubkey: acct.pubkey.to_string(),
                program_pubkey: KAMINO_VAULTS_PROGRAM_ID.to_string(),
                name: None,
            });

            tx.send(vault_info)?;
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

    use super::*;

    #[tokio::test]
    async fn try_closed_strategy() {
        let txn_file = fs::read_to_string("tests/kamino/closed_strategy.json").unwrap();
        let json = serde_json::from_str(&txn_file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();

        let (step_utils, _) = init_test_extras_bare().await;

        let kamino_vaults = KaminoVaults;
        let mut dooot = vec![];
        kamino_vaults
            .invoke(&txn, &mut dooot, &step_utils)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn try_strategy_swap() {
        let txn_file = fs::read_to_string("tests/kamino/strategy_swap.json").unwrap();
        let json = serde_json::from_str(&txn_file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();

        let (step_utils, _) = init_test_extras_bare().await;

        let kamino_vaults = KaminoVaults;
        let mut dooot = vec![];
        kamino_vaults
            .invoke(&txn, &mut dooot, &step_utils)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn try_deposit() {
        let txn_file = fs::read_to_string("tests/kamino/deposit.json").unwrap();
        let json = serde_json::from_str(&txn_file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();

        let (step_utils, _) = init_test_extras_bare().await;

        let kamino_vaults = KaminoVaults;
        let mut dooot = vec![];
        kamino_vaults
            .invoke(&txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooot.len(), 2);
    }
}
