use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;

use rust_decimal::Decimal;
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};

use crate::{state_source::StateSource, TransactionParserFilter, TransactionParserTrait};
use step_ingestooor_sdk::{
    dooot::{
        Dooot, LPInfoDooot, LPInfoUnderlyingMintVault, LiquidityChangeEventDooot,
        MintUnderlyingsGlobalDooot, SwapEventDooot, SwapFees,
    },
    trait_defs::{pod_types::RefFromByteRef, transaction::TransactionLike},
    utils::{
        step_utils::StepUtils,
        util_fns::{
            convert_blocktime_to_micros, get_partially_decoded_variant, get_program_id_of_ix,
        },
    },
};

pub const SABER_PROGRAM_ID: &str = "SSwpkEEcbUqx4vtoEByFjSkhKdCT862DNVb52nZg1UZ";
pub const TOKEN_INFO_OFFSET: usize = 107;

pub struct SaberPools;

#[async_trait]
impl TransactionParserTrait for SaberPools {
    fn get_name(&self) -> &'static str {
        "SaberPools"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(SABER_PROGRAM_ID.into())]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let accts: Vec<_> = txn
            .iter_accounts()
            .filter(|a| a.post_owner_program.is_some_and(|p| p == SABER_PROGRAM_ID))
            .collect();
        let mut pool_map = HashMap::new();

        let time = txn.get_block_time();
        for acct in accts {
            let Some(post) = acct.unpack_post_datum()? else {
                log::warn!("Saber post datum is empty. Most likely closed acct");
                continue;
            };

            if post.len() != 395 {
                log::warn!(
                    "Saber post datum has unexpected length: {} {}",
                    post.len(),
                    txn.get_signature()
                );
                continue;
            }

            let pool_id = acct.pubkey;
            let SaberPoolInfo {
                token_a_reserve,
                token_b_reserve,
                pool_mint,
                token_a_mint,
                token_b_mint,
                trade_fee_numerator,
                trade_fee_denominator,
            } = SaberPools::get_info_from_account_data(&post)?;

            let was_created = acct.was_created;
            pool_map.insert(
                pool_id.to_string(),
                (trade_fee_numerator, trade_fee_denominator),
            );

            if was_created {
                let lp_info = Dooot::LPInfo(LPInfoDooot {
                    time,
                    curve_type: step_ingestooor_sdk::dooot::CurveType::Stable,
                    lp_mint: Some(pool_mint.clone()),
                    pool: pool_id.into(),
                    program: SABER_PROGRAM_ID.into(),
                    underlyings: vec![
                        LPInfoUnderlyingMintVault {
                            mint: token_a_mint.clone(),
                            vault: token_a_reserve.clone(),
                        },
                        LPInfoUnderlyingMintVault {
                            mint: token_b_mint.clone(),
                            vault: token_b_reserve.clone(),
                        },
                    ],
                });

                dooots.push(lp_info);
            }

            match txn.get_token_supply(&pool_mint) {
                Some(supply) => {
                    let post_token_a_balance =
                        match txn.get_post_token_account_balance(&token_a_reserve.to_string()) {
                            Ok(Some(balance)) => balance,
                            _ => {
                                continue;
                            }
                        };

                    let post_token_b_balance =
                        match txn.get_post_token_account_balance(&token_b_reserve.to_string()) {
                            Ok(Some(balance)) => balance,
                            _ => {
                                continue;
                            }
                        };

                    let a_per_parent = (post_token_a_balance as f64) / (supply as f64);
                    let b_per_parent = (post_token_b_balance as f64) / (supply as f64);

                    let mint_underlying_dooot =
                        Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                            mint_pubkey: pool_mint,
                            platform_program_pubkey: SABER_PROGRAM_ID.into(),
                            mints: vec![token_a_mint, token_b_mint],
                            time,
                            total_underlying_amounts: vec![
                                post_token_a_balance.into(),
                                post_token_b_balance.into(),
                            ],
                            mints_qty_per_one_parent: vec![a_per_parent, b_per_parent],
                            discriminant_id: pool_id.to_string(),
                        });

                    dooots.push(mint_underlying_dooot);
                }
                None => {
                    continue;
                }
            }
        }

        let ix_tree = txn.get_ix_tree()?;
        let ix_iter = ix_tree
            .root()
            .context("UNREACHABLE - Tree should have a root")?
            .traverse_level_order()
            .skip(1);

        for node in ix_iter {
            let ix = node
                .data()
                .context("UNREACHABLE - Tree node should have data")?;
            let Some(ix) = get_partially_decoded_variant(ix) else {
                continue;
            };

            if ix.program_id != SABER_PROGRAM_ID {
                continue;
            }

            let ix_data = bs58::decode(&ix.data).into_vec()?;

            let Some(tag) = ix_data.first() else {
                continue;
            };

            if *tag == 0 || *tag > 4 {
                continue;
            }

            let pool = &ix.accounts[0];
            let user = &ix.accounts[2];
            let ixs_scan = node.children().filter_map(|kid| *kid.data());

            match tag {
                1 => {
                    // Swap ix
                    let parent_id = node
                        .parent()
                        .and_then(|p| *p.data())
                        .and_then(get_program_id_of_ix);

                    let ata_in = &ix.accounts[3];
                    let ata_out = &ix.accounts[6];

                    let ata_in_data = txn.get_token_account_data_by_key(ata_in)?;
                    let ata_out_data = txn.get_token_account_data_by_key(ata_out)?;

                    let mint_in = ata_in_data.mint;
                    let mint_out = ata_out_data.mint;

                    let txfer_amounts = txn.get_total_transfer_amounts_for_pubkey_ui_ixs(
                        ixs_scan,
                        &[user.as_str()],
                        &[&mint_in],
                        &[&mint_out],
                    )?;

                    let amount_in = txfer_amounts.get_in_amount_u64(&mint_in, user);
                    let amount_out = txfer_amounts.get_out_amount_u64(&mint_out, user);

                    let (trade_fee_numerator, trade_fee_denominator) = pool_map
                        .get(pool)
                        .context("UNREACHABLE - Pool should be in map")?;

                    let swap_row = Dooot::SwapEvent(SwapEventDooot {
                        time,
                        in_amount: amount_in.into(),
                        out_amount: amount_out.into(),
                        in_mint_pubkey: mint_in.to_string(),
                        out_mint_pubkey: mint_out.to_string(),
                        program_pubkey: SABER_PROGRAM_ID.to_string(),
                        swap_auth_pubkey: user.to_string(),
                        market_pubkey: Some(pool.to_string()),
                        parent_amm_program_pubkey: parent_id,
                        fee_mint_is_in_mint: None,
                        referring_platform_account_pubkey: None,
                        platform_referral_fee: None,
                        fees: vec![SwapFees {
                            mint: mint_in.to_string(),
                            amount: Decimal::from(
                                amount_in * trade_fee_numerator / trade_fee_denominator,
                            ),
                        }],
                        slippage_bps: None,
                        tx_sig: txn.get_signature().to_string(),
                    });

                    dooots.push(swap_row);
                }
                2..=4 => {
                    // Liq change
                    let mint_a = &ix.accounts[5];
                    let mint_b = &ix.accounts[6];
                    let liq_row = match tag {
                        2 => {
                            // Deposit
                            let txfer_amounts = txn.get_total_transfer_amounts_for_pubkey_ui_ixs(
                                ixs_scan,
                                &[user.as_str()],
                                &[mint_a, mint_b],
                                &[],
                            )?;

                            let a_in = txfer_amounts.get_in_amount_u64(mint_a, user);
                            let b_in = txfer_amounts.get_in_amount_u64(mint_b, user);

                            Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                                time,
                                mints: vec![mint_a.to_string(), mint_b.to_string()],
                                amounts: vec![a_in.into(), b_in.into()],
                                owner: user.to_string(),
                                pool: pool.to_string(),
                            })
                        }
                        3 | 4 => {
                            // Withdraw
                            let txfer_amounts = txn.get_total_transfer_amounts_for_pubkey_ui_ixs(
                                ixs_scan,
                                &[user.as_str()],
                                &[],
                                &[mint_a, mint_b],
                            )?;

                            let a_out = txfer_amounts.get_out_amount_u64(mint_a, user);
                            let b_out = txfer_amounts.get_out_amount_u64(mint_b, user);

                            Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                                time,
                                mints: vec![mint_a.to_string(), mint_b.to_string()],
                                amounts: vec![
                                    Decimal::from(a_out) * Decimal::NEGATIVE_ONE,
                                    Decimal::from(b_out) * Decimal::NEGATIVE_ONE,
                                ],
                                owner: user.to_string(),
                                pool: pool.to_string(),
                            })
                        }
                        _ => bail!("[Saber] UNREACHABLE - Tag is checked by above match statement"),
                    };

                    dooots.push(liq_row);
                }
                _ => {
                    continue;
                }
            }
        }

        Ok(())
    }
}

pub struct SaberPoolInfo {
    pub token_a_reserve: String,
    pub token_b_reserve: String,
    pub pool_mint: String,
    pub token_a_mint: String,
    pub token_b_mint: String,
    pub trade_fee_numerator: u64,
    pub trade_fee_denominator: u64,
}

impl SaberPools {
    /// Parse account data from https://github.com/saber-hq/stable-swap/blob/master/stable-swap-client/src/state.rs#L95-L99
    pub fn get_info_from_account_data(data: &[u8]) -> Result<SaberPoolInfo> {
        let info_slice = &data[TOKEN_INFO_OFFSET..];
        let token_a_reserve = Pubkey::from_bytes(&info_slice[0..32])
            .map(|pk| pk.to_string())
            .map_err(|e| anyhow!(e))?;
        let token_b_reserve = Pubkey::from_bytes(&info_slice[32..64])
            .map(|pk| pk.to_string())
            .map_err(|e| anyhow!(e))?;
        let pool_mint = Pubkey::from_bytes(&info_slice[64..96])
            .map(|pk| pk.to_string())
            .map_err(|e| anyhow!(e))?;
        let token_a_mint = Pubkey::from_bytes(&info_slice[96..128])
            .map(|pk| pk.to_string())
            .map_err(|e| anyhow!(e))?;
        let token_b_mint = Pubkey::from_bytes(&info_slice[128..160])
            .map(|pk| pk.to_string())
            .map_err(|e| anyhow!(e))?;
        let trade_fee_numerator = u64::from_le_bytes(info_slice[192..200].try_into()?);
        let trade_fee_denominator = u64::from_le_bytes(info_slice[200..208].try_into()?);

        Ok(SaberPoolInfo {
            token_a_reserve,
            token_b_reserve,
            pool_mint,
            token_a_mint,
            token_b_mint,
            trade_fee_numerator,
            trade_fee_denominator,
        })
    }
}

impl StateSource for SaberPools {
    fn get_name(&self) -> &'static str {
        "SaberPools"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let blocktime = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let saber_pk = SABER_PROGRAM_ID.parse::<Pubkey>()?;
        let saber_accts_iter = acct_provider.program_accounts(saber_pk, None);

        let mut saber_accounts_lookup = HashMap::new();
        let mut vault_accounts = HashSet::new();
        let mut lp_mints = HashSet::new();
        let mut balances = HashMap::new();
        let mut supply_amts = HashMap::new();

        for acct in saber_accts_iter {
            let pk = acct.pubkey;
            let data = acct.account.data();
            if data.len() != 395 {
                log::warn!("Saber account data has unexpected length: {}", data.len());
                continue;
            }

            let pool_info = SaberPools::get_info_from_account_data(data)?;
            let a_reserve_pk = Pubkey::from_str(&pool_info.token_a_reserve)?;
            let b_reserve_pk = Pubkey::from_str(&pool_info.token_b_reserve)?;
            let lp_mint_pk = Pubkey::from_str(&pool_info.pool_mint)?;
            vault_accounts.insert(a_reserve_pk);
            vault_accounts.insert(b_reserve_pk);
            lp_mints.insert(lp_mint_pk);

            let lp_info_dooot = Dooot::LPInfo(LPInfoDooot {
                time: blocktime,
                curve_type: step_ingestooor_sdk::dooot::CurveType::Stable,
                lp_mint: Some(pool_info.pool_mint.clone()),
                pool: pk.to_string(),
                program: SABER_PROGRAM_ID.into(),
                underlyings: vec![
                    LPInfoUnderlyingMintVault {
                        mint: pool_info.token_a_mint.clone(),
                        vault: pool_info.token_a_reserve.clone(),
                    },
                    LPInfoUnderlyingMintVault {
                        mint: pool_info.token_b_mint.clone(),
                        vault: pool_info.token_b_reserve.clone(),
                    },
                ],
            });

            tx.send(lp_info_dooot)?;

            saber_accounts_lookup.insert(pk, pool_info);
        }

        // Get all token accounts referenced by Saber pools
        let all_accounts_iter = acct_provider.all_accounts();
        for acct in all_accounts_iter {
            let pk = acct.pubkey;
            if vault_accounts.contains(&pk) {
                let data = acct.account.data();
                let amount = u64::from_le_bytes(data[64..72].try_into()?);
                balances.insert(pk, amount);
            }

            if lp_mints.contains(&pk) {
                let data = acct.account.data();
                let supply = u64::from_le_bytes(data[36..44].try_into()?);
                supply_amts.insert(pk, supply);
            }
        }

        for (pool_id, pool_info) in saber_accounts_lookup {
            let a_reserve_pk = Pubkey::from_str(&pool_info.token_a_reserve)?;
            let b_reserve_pk = Pubkey::from_str(&pool_info.token_b_reserve)?;
            let lp_mint_pk = Pubkey::from_str(&pool_info.pool_mint)?;

            let a_reserve_amt = balances.get(&a_reserve_pk).context("missing a reserve")?;
            let b_reserve_amt = balances.get(&b_reserve_pk).context("missing b reserve")?;
            let supply = supply_amts.get(&lp_mint_pk).context("missing supply")?;

            let a_per_parent = (*a_reserve_amt as f64) / (*supply as f64);
            let b_per_parent = (*b_reserve_amt as f64) / (*supply as f64);

            let mint_underlying_dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                mint_pubkey: pool_info.pool_mint.clone(),
                platform_program_pubkey: SABER_PROGRAM_ID.into(),
                mints: vec![
                    pool_info.token_a_mint.clone(),
                    pool_info.token_b_mint.clone(),
                ],
                time: blocktime,
                total_underlying_amounts: vec![(*a_reserve_amt).into(), (*b_reserve_amt).into()],
                mints_qty_per_one_parent: vec![a_per_parent, b_per_parent],
                discriminant_id: pool_id.to_string(),
            });

            tx.send(mint_underlying_dooot)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransactionParserTrait;
    use std::{fs, rc::Rc};
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;

    #[tokio::test]
    async fn invoke_test() {
        let path = "./tests/saber/saber_failed.json";
        let message_json = fs::read_to_string(path).unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser: SaberPools = SaberPools;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 0);
    }
}
