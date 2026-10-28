use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use crate::{state_source::StateSource, TransactionParserFilter};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use borsh::BorshDeserialize;
use rust_decimal::Decimal;
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
use step_ingestooor_idl_gens::atrix_v1_gen::atrix_v1::accounts::PoolAccount;
use step_ingestooor_sdk::{
    dooot::{
        Dooot, LPInfoDooot, LPInfoUnderlyingMintVault, LiquidityChangeEventDooot,
        MintUnderlyingsGlobalDooot,
    },
    trait_defs::transaction::TransactionLike,
    utils::{step_utils::StepUtils, util_fns::convert_blocktime_to_micros},
};

pub struct AtrixAmmV1;

const ATRIX_AMM_V1_PROGRAM: &str = "HvwYjjzPbXWpykgVZhqvvfeeaSraQVnTiQibofaFw9M7";
const POOL_DISCRIMATOR: &[u8] = &[0x74, 0xd2, 0xbb, 0x77, 0xc4, 0xc4, 0x34, 0x89];

#[async_trait]
impl crate::TransactionParserTrait for AtrixAmmV1 {
    fn get_name(&self) -> &'static str {
        "AtrixAMMV1"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![crate::TransactionParserFilter::ProgramId(
            ATRIX_AMM_V1_PROGRAM.to_string(),
        )]
    }

    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = decoded_transaction.get_block_time();
        let accts: Vec<_> = decoded_transaction
            .iter_accounts()
            .filter(|a| {
                a.post_owner_program
                    .is_some_and(|p| p == ATRIX_AMM_V1_PROGRAM)
            })
            .collect();

        for acct in accts {
            let Some(post) = acct.unpack_post_datum()? else {
                log::warn!("Failed to unpack post datum for Atrix AMM V1. Closed account?");
                continue;
            };
            if post[..8] != *POOL_DISCRIMATOR {
                continue;
            }
            let post_pool_acc = PoolAccount::deserialize(&mut &post[8..])?;
            let was_created = acct.pre_datum.is_none();
            if was_created {
                let curve_type = if post_pool_acc.pool_type == 0 {
                    step_ingestooor_sdk::dooot::CurveType::ConstantProduct
                } else {
                    step_ingestooor_sdk::dooot::CurveType::Stable
                };
                let lp_info = Dooot::LPInfo(LPInfoDooot {
                    time,
                    curve_type,
                    lp_mint: Some(post_pool_acc.lp_mint.to_string()),
                    pool: acct.pubkey.into(),
                    program: ATRIX_AMM_V1_PROGRAM.into(),
                    underlyings: vec![
                        LPInfoUnderlyingMintVault {
                            mint: post_pool_acc.coin_mint.to_string(),
                            vault: post_pool_acc.pool_coin_account.to_string(),
                        },
                        LPInfoUnderlyingMintVault {
                            mint: post_pool_acc.pc_mint.to_string(),
                            vault: post_pool_acc.pool_pc_account.to_string(),
                        },
                    ],
                });
                dooot.push(lp_info);
            }
            let pool_mint = post_pool_acc.lp_mint.to_string();
            let token_a = post_pool_acc.coin_mint.to_string();
            let token_b = post_pool_acc.pc_mint.to_string();
            let lp_mint_data = decoded_transaction.get_token_supply(&pool_mint);

            if !was_created && acct.datum_changed {
                let Some(pre_datum) = acct.unpack_pre_datum()? else {
                    log::warn!("Failed to unpack pre datum for Atrix AMM V1.");
                    continue;
                };
                let pre_pool_acc = PoolAccount::deserialize(&mut &pre_datum[8..])?;
                let a_diff = post_pool_acc.pca - pre_pool_acc.pca;
                let b_diff = post_pool_acc.ppca - pre_pool_acc.ppca;

                let liq_change = Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                    time,
                    owner: decoded_transaction
                        .get_first_signer()
                        .ok_or(anyhow!("Unreachable - No signer?"))?,
                    pool: acct.pubkey.to_string(),
                    mints: vec![token_a.clone(), token_b.clone()],
                    amounts: vec![
                        Decimal::from(a_diff) * Decimal::NEGATIVE_ONE,
                        Decimal::from(b_diff) * Decimal::NEGATIVE_ONE,
                    ],
                });
                dooot.push(liq_change);
            }

            if let Some(supply) = lp_mint_data {
                let Some(post_token_a_balance) = decoded_transaction
                    .get_post_token_account_balance(&token_a)
                    .ok()
                    .flatten()
                else {
                    continue;
                };
                let Some(post_token_b_balance) = decoded_transaction
                    .get_post_token_account_balance(&token_b)
                    .ok()
                    .flatten()
                else {
                    continue;
                };

                if supply == 0 {
                    continue;
                }

                let a_per_parent = (post_token_a_balance as f64) / (supply as f64);
                let b_per_parent = (post_token_b_balance as f64) / (supply as f64);

                let mint_underlying_dooot =
                    Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                        mint_pubkey: pool_mint,
                        platform_program_pubkey: ATRIX_AMM_V1_PROGRAM.into(),
                        mints: vec![token_a, token_b],
                        time,
                        total_underlying_amounts: vec![
                            post_token_a_balance.into(),
                            post_token_b_balance.into(),
                        ],
                        mints_qty_per_one_parent: vec![a_per_parent, b_per_parent],
                        discriminant_id: acct.pubkey.to_string(),
                    });

                dooot.push(mint_underlying_dooot);
            }
        }
        Ok(())
    }
}

impl StateSource for AtrixAmmV1 {
    fn get_name(&self) -> &'static str {
        "AtrixAMMV1"
    }
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let blocktime = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let atrix_pk = ATRIX_AMM_V1_PROGRAM.parse::<Pubkey>()?;
        let atrix_accts_iter = acct_provider.program_accounts(atrix_pk, None);

        let mut atrix_accounts_lookup = HashMap::new();
        let mut lp_mints = HashSet::new();
        let mut supply_amts = HashMap::new();

        for acct in atrix_accts_iter {
            let pk = acct.pubkey;
            let data = acct.account.data();
            if data[0..8] != *POOL_DISCRIMATOR {
                continue;
            }
            let pool_acct = PoolAccount::deserialize(&mut &data[8..])?;
            let pool_mint = pool_acct.lp_mint.to_string();
            let token_a = pool_acct.coin_mint.to_string();
            let token_b = pool_acct.pc_mint.to_string();
            let curve_type = if pool_acct.pool_type == 0 {
                step_ingestooor_sdk::dooot::CurveType::ConstantProduct
            } else {
                step_ingestooor_sdk::dooot::CurveType::Stable
            };
            let lp_mint_pk = Pubkey::from_str(&pool_mint)?;
            lp_mints.insert(lp_mint_pk);

            let lp_info_dooot = Dooot::LPInfo(LPInfoDooot {
                time: blocktime,
                curve_type,
                lp_mint: Some(pool_mint.clone()),
                pool: pk.to_string(),
                program: ATRIX_AMM_V1_PROGRAM.into(),
                underlyings: vec![
                    LPInfoUnderlyingMintVault {
                        mint: token_a,
                        vault: pool_acct.pool_coin_account.to_string(),
                    },
                    LPInfoUnderlyingMintVault {
                        mint: token_b,
                        vault: pool_acct.pool_pc_account.to_string(),
                    },
                ],
            });
            tx.send(lp_info_dooot)?;

            atrix_accounts_lookup.insert(pk, pool_acct);
        }

        // Get all token accounts referenced by Atrix pools
        let all_accounts_iter = acct_provider.all_accounts();
        for acct in all_accounts_iter {
            let pk = acct.pubkey;
            if lp_mints.contains(&pk) {
                let data = acct.account.data();
                let supply = u64::from_le_bytes(data[36..44].try_into()?);
                supply_amts.insert(pk, supply);
            }
        }

        for (pool_id, pool_info) in atrix_accounts_lookup {
            let lp_mint_pk = pool_info.lp_mint;

            let a_reserve_amt = pool_info.pca;
            let b_reserve_amt = pool_info.ppca;
            let supply = supply_amts
                .get(&lp_mint_pk)
                .context(format!("Missing supply for {lp_mint_pk}"))?;

            let a_per_parent = (a_reserve_amt as f64) / (*supply as f64);
            let b_per_parent = (b_reserve_amt as f64) / (*supply as f64);

            let mint_underlying_dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                mint_pubkey: lp_mint_pk.to_string(),
                platform_program_pubkey: ATRIX_AMM_V1_PROGRAM.into(),
                mints: vec![
                    pool_info.coin_mint.to_string(),
                    pool_info.pc_mint.to_string(),
                ],
                time: blocktime,
                total_underlying_amounts: vec![(a_reserve_amt).into(), (b_reserve_amt).into()],
                mints_qty_per_one_parent: vec![a_per_parent, b_per_parent],
                discriminant_id: pool_id.to_string(),
            });
            tx.send(mint_underlying_dooot)?;
        }

        Ok(())
    }
}
