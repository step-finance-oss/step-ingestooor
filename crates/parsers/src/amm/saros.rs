use anyhow::{anyhow, Context, Result};

use async_trait::async_trait;
use rust_decimal::Decimal;
use solana_sdk::account::ReadableAccount;
use solana_sdk::pubkey::Pubkey;

use step_ingestooor_sdk::dooot::{
    CurveType, LPInfoDooot, LPInfoUnderlyingMintVault, LiquidityChangeEventDooot,
    MintUnderlyingsGlobalDooot, SwapEventDooot, SwapFees,
};
use step_ingestooor_sdk::utils::step_account_reader::StepAccountReader;
use step_ingestooor_sdk::utils::step_utils::StepUtils;
use step_ingestooor_sdk::utils::util_fns::{
    convert_blocktime_to_micros, get_partially_decoded_variant, get_program_id_of_ix,
};

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use crate::state_source::StateSource;
use crate::{TransactionParserFilter, TransactionParserTrait};
use step_ingestooor_sdk::{dooot::Dooot, trait_defs::transaction::TransactionLike};

pub struct Saros;
pub const SAROS_SWAP_PROGRAM: &str = "SSwapUtytfBdBn1b9NUGG6foMVPtcWgpRU32HToDUZr";

#[derive(Debug)]
pub struct SarosPoolInfo {
    pub token_a: String,
    pub token_b: String,
    pub pool_mint: String,
    pub token_a_mint: String,
    pub token_b_mint: String,
    pub trade_fee_numerator: u64,
    pub trade_fee_denominator: u64,
    pub curve_type: CurveType,
}
impl Saros {
    fn get_info_from_acct_data(data: &[u8]) -> Result<SarosPoolInfo> {
        let mut reader = StepAccountReader::new(data);
        reader.move_index_to(35);

        let token_a = reader.read_next_pubkey()?;
        let token_b = reader.read_next_pubkey()?;
        let pool_mint = reader.read_next_pubkey()?;
        let token_a_mint = reader.read_next_pubkey()?;
        let token_b_mint = reader.read_next_pubkey()?;
        let trade_fee_numerator = reader.read_next_le_u64()?;
        let trade_fee_denominator = reader.read_next_le_u64()?;
        reader.advance_unclamped(48);
        let curve_type = reader.read_next_single_byte()?;

        let curve_type = match curve_type {
            0 => CurveType::ConstantProduct,
            1 => CurveType::ConstantPrice,
            2 => CurveType::Stable,
            3 => CurveType::StableWithOffset,
            _ => return Err(anyhow!("unknown curve type")),
        };

        Ok(SarosPoolInfo {
            token_a: token_a.to_string(),
            token_b: token_b.to_string(),
            pool_mint: pool_mint.to_string(),
            token_a_mint: token_a_mint.to_string(),
            token_b_mint: token_b_mint.to_string(),
            trade_fee_numerator,
            trade_fee_denominator,
            curve_type,
        })
    }
}

#[async_trait]
impl TransactionParserTrait for Saros {
    fn get_name(&self) -> &'static str {
        "SarosProgram"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            SAROS_SWAP_PROGRAM.to_string(),
        )]
    }

    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> anyhow::Result<()> {
        let time = decoded_transaction.get_block_time();

        let accts: Vec<_> = decoded_transaction
            .iter_accounts()
            .filter(|a| {
                a.post_owner_program
                    .is_some_and(|program| program == SAROS_SWAP_PROGRAM)
            })
            .collect();

        for acct in accts {
            let Some(post) = acct.unpack_post_datum()? else {
                log::warn!("empty saros post datum");
                continue;
            };

            if post.len() != 324 {
                log::warn!("saros post datum has unexpected length {:?}", post.len());
                continue;
            }

            let pool_id = acct.pubkey;

            let SarosPoolInfo {
                token_a,
                token_b,
                pool_mint,
                token_a_mint,
                token_b_mint,
                trade_fee_numerator,
                trade_fee_denominator,
                curve_type,
            } = Saros::get_info_from_acct_data(&post)?;

            let was_created = acct.was_created;

            if was_created {
                let lp_info = Dooot::LPInfo(LPInfoDooot {
                    time,
                    curve_type,
                    lp_mint: Some(pool_mint.clone()),
                    pool: pool_id.into(),
                    program: SAROS_SWAP_PROGRAM.into(),
                    underlyings: vec![
                        LPInfoUnderlyingMintVault {
                            mint: token_a_mint.clone(),
                            vault: token_a.clone(),
                        },
                        LPInfoUnderlyingMintVault {
                            mint: token_b_mint.clone(),
                            vault: token_b.clone(),
                        },
                    ],
                });
                dooot.push(lp_info);
            }
            let supply_after = decoded_transaction.get_token_supply(&pool_mint);

            let vault_a_pre = decoded_transaction
                .get_pre_token_account_balance(&token_a.to_string())?
                .ok_or_else(|| anyhow!("saros - pre token a balance not found"))?;
            let vault_a_post = decoded_transaction
                .get_post_token_account_balance(&token_a.to_string())?
                .ok_or_else(|| anyhow!("saros - post token a balance not found"))?;

            let vault_b_pre = decoded_transaction
                .get_pre_token_account_balance(&token_b.to_string())?
                .ok_or_else(|| anyhow!("saros - pre token b balance not found"))?;
            let vault_b_post = decoded_transaction
                .get_post_token_account_balance(&token_b.to_string())?
                .ok_or_else(|| anyhow!("saros - post token b balance not found"))?;

            let vault_a_changed = vault_a_pre != vault_a_post;
            let vault_b_changed = vault_b_pre != vault_b_post;

            if vault_a_changed || vault_b_changed {
                if let Some(supply) = supply_after {
                    // MintUnderlying
                    let (a_per_parent, b_per_parent) = if supply == 0 {
                        (0.0, 0.0)
                    } else {
                        let a_per_parent = (vault_a_post as f64) / (supply as f64);
                        let b_per_parent = (vault_b_post as f64) / (supply as f64);

                        (a_per_parent, b_per_parent)
                    };

                    let mint_underlying_dooot =
                        Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                            mint_pubkey: pool_mint.clone(),
                            platform_program_pubkey: SAROS_SWAP_PROGRAM.into(),
                            mints: vec![token_a_mint.clone(), token_b_mint.clone()],
                            total_underlying_amounts: vec![
                                vault_a_post.into(),
                                vault_b_post.into(),
                            ],
                            mints_qty_per_one_parent: vec![a_per_parent, b_per_parent],
                            time,
                            discriminant_id: pool_id.to_string(),
                        });

                    dooot.push(mint_underlying_dooot);
                }

                let tree = decoded_transaction.get_ix_tree()?;
                let root = tree.root().context("No root node found in ix tree")?;
                let ix_iter = root.traverse_level_order().skip(1);

                for node in ix_iter {
                    let ix = node.data();
                    let Some(ix) = ix.and_then(get_partially_decoded_variant) else {
                        continue;
                    };

                    if ix.program_id != SAROS_SWAP_PROGRAM {
                        continue;
                    }

                    let ix_data = bs58::decode(&ix.data).into_vec()?;

                    let Some(tag) = ix_data.first() else {
                        continue;
                    };

                    if *tag == 0 || *tag > 4 {
                        continue;
                    }

                    let user = &ix.accounts[2];
                    let pool = &ix.accounts[0];
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

                            let ata_in_data =
                                decoded_transaction.get_token_account_data_by_key(ata_in)?;
                            let ata_out_data =
                                decoded_transaction.get_token_account_data_by_key(ata_out)?;

                            let mint_in = ata_in_data.mint;
                            let mint_out = ata_out_data.mint;

                            let txfer_amounts = decoded_transaction
                                .get_total_transfer_amounts_for_pubkey_ui_ixs(
                                    ixs_scan,
                                    &[user],
                                    &[&mint_in],
                                    &[&mint_out],
                                )?;

                            let amount_in = txfer_amounts.get_in_amount_u64(&mint_in, user);
                            let amount_out = txfer_amounts.get_out_amount_u64(&mint_out, user);

                            let swap_row = Dooot::SwapEvent(SwapEventDooot {
                                time,
                                in_amount: amount_in.into(),
                                out_amount: amount_out.into(),
                                in_mint_pubkey: mint_in.to_string(),
                                out_mint_pubkey: mint_out.to_string(),
                                program_pubkey: SAROS_SWAP_PROGRAM.to_string(),
                                swap_auth_pubkey: user.to_string(),
                                market_pubkey: Some(pool.to_string()),
                                parent_amm_program_pubkey: parent_id,
                                fee_mint_is_in_mint: None,
                                referring_platform_account_pubkey: None,
                                platform_referral_fee: None,
                                fees: vec![SwapFees {
                                    mint: mint_in.to_string(),
                                    amount: Decimal::from(
                                        ((amount_in as u128) * (trade_fee_numerator as u128)
                                            / (trade_fee_denominator as u128))
                                            as u64,
                                    ),
                                }],
                                slippage_bps: None,
                                tx_sig: decoded_transaction.get_signature().to_string(),
                            });
                            dooot.push(swap_row);
                        }
                        2..=4 => {
                            // Liq change
                            let mint_a = &ix.accounts[5];
                            let mint_b = &ix.accounts[6];
                            let liq_row = match tag {
                                2 => {
                                    // Deposit
                                    let txfer_amounts = decoded_transaction
                                        .get_total_transfer_amounts_for_pubkey_ui_ixs(
                                            ixs_scan,
                                            &[user],
                                            &[mint_a, mint_b],
                                            &[],
                                        )?;

                                    let a_in = txfer_amounts.get_in_amount_u64(mint_a, user);
                                    let b_in = txfer_amounts.get_in_amount_u64(mint_b, user);

                                    let mint_a_acc = decoded_transaction
                                        .get_token_account_data_by_key(mint_a)?
                                        .mint;
                                    let mint_b_acc = decoded_transaction
                                        .get_token_account_data_by_key(mint_b)?
                                        .mint;

                                    Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                                        time,
                                        mints: vec![mint_a_acc.to_string(), mint_b_acc.to_string()],
                                        amounts: vec![a_in.into(), b_in.into()],
                                        owner: user.to_string(),
                                        pool: pool.to_string(),
                                    })
                                }
                                3 | 4 => {
                                    // Withdraw
                                    let txfer_amounts = decoded_transaction
                                        .get_total_transfer_amounts_for_pubkey_ui_ixs(
                                            ixs_scan,
                                            &[user],
                                            &[],
                                            &[mint_a, mint_b],
                                        )?;

                                    let a_out = txfer_amounts.get_out_amount_u64(mint_a, user);
                                    let b_out = txfer_amounts.get_out_amount_u64(mint_b, user);

                                    let mint_a_acc = decoded_transaction
                                        .get_token_account_data_by_key(mint_a)?
                                        .mint;
                                    let mint_b_acc = decoded_transaction
                                        .get_token_account_data_by_key(mint_b)?
                                        .mint;

                                    Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                                        time,
                                        mints: vec![mint_a_acc.to_string(), mint_b_acc.to_string()],
                                        amounts: vec![
                                            Decimal::from(a_out) * Decimal::NEGATIVE_ONE,
                                            Decimal::from(b_out) * Decimal::NEGATIVE_ONE,
                                        ],
                                        owner: user.to_string(),
                                        pool: pool.to_string(),
                                    })
                                }
                                _ => continue,
                            };
                            dooot.push(liq_row);
                        }
                        _ => {
                            continue;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

impl StateSource for Saros {
    fn get_name(&self) -> &'static str {
        "SarosProgram"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let blocktime = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;

        let saros_pubkey = SAROS_SWAP_PROGRAM.parse::<Pubkey>()?;

        let saros_acct = acct_provider.program_accounts(saros_pubkey, None);
        let mut saros_accounts_lookup = HashMap::new();
        let mut vault_accounts = HashSet::new();
        let mut lp_mints = HashSet::new();
        let mut balances = HashMap::new();
        let mut supply_amts = HashMap::new();

        for acct in saros_acct {
            let pk = acct.pubkey;

            let data = acct.account.data(); // 324 bytes
            if data.len() != 324 {
                log::warn!("Saros account data has unexpected length: {}", data.len());
                continue;
            }

            let pool_info_result = Saros::get_info_from_acct_data(data);
            let pool_info = match pool_info_result {
                Ok(info) => info,
                Err(err) => {
                    log::error!(
                        "Failed to get pool info from account data for account {:?}: {:?}",
                        pk,
                        err
                    );
                    continue;
                }
            };

            let a_reserve_pk = match Pubkey::from_str(&pool_info.token_a) {
                Ok(pk) => pk,
                Err(err) => {
                    log::error!("Failed to parse token_a: {:?}", err);
                    continue;
                }
            };
            let b_reserve_pk = match Pubkey::from_str(&pool_info.token_b) {
                Ok(pk) => pk,
                Err(err) => {
                    log::error!("Failed to parse token_b: {:?}", err);
                    continue;
                }
            };
            let lp_mint_pk = match Pubkey::from_str(&pool_info.pool_mint) {
                Ok(pk) => pk,
                Err(err) => {
                    log::error!("Failed to parse pool_mint: {:?}", err);
                    continue;
                }
            };

            vault_accounts.insert(a_reserve_pk);
            vault_accounts.insert(b_reserve_pk);
            lp_mints.insert(lp_mint_pk);

            let lp_info_schema = Dooot::LPInfo(LPInfoDooot {
                time: blocktime,
                curve_type: pool_info.curve_type.clone(),
                lp_mint: Some(pool_info.pool_mint.clone()),
                pool: pk.to_string(),
                program: SAROS_SWAP_PROGRAM.into(),
                underlyings: vec![
                    LPInfoUnderlyingMintVault {
                        mint: pool_info.token_a_mint.clone(),
                        vault: pool_info.token_a.clone(),
                    },
                    LPInfoUnderlyingMintVault {
                        mint: pool_info.token_b_mint.clone(),
                        vault: pool_info.token_b.clone(),
                    },
                ],
            });
            tx.send(lp_info_schema)?;

            saros_accounts_lookup.insert(pk, pool_info);
        }

        let all_accounts_iter = acct_provider.all_accounts();
        for acct in all_accounts_iter {
            let pk = acct.pubkey;

            if vault_accounts.contains(&pk) {
                // gives nothin in here
                let data = acct.account.data();
                let amount_result = data[64..72].try_into();
                let amount = match amount_result {
                    Ok(val) => u64::from_le_bytes(val),
                    Err(err) => {
                        log::error!(
                            "Failed to extract vault account balance for {:?}: {:?}",
                            pk,
                            err
                        );
                        continue;
                    }
                };
                balances.insert(pk, amount);
            }

            if lp_mints.contains(&pk) {
                let data = acct.account.data();
                let supply_result = data[36..44].try_into();
                let supply = match supply_result {
                    Ok(val) => u64::from_le_bytes(val),
                    Err(err) => {
                        log::error!("Failed to extract LP mint supply for {:?}: {:?}", pk, err);
                        continue;
                    }
                };
                supply_amts.insert(pk, supply);
            }
        }

        for (pool_id, pool_info) in saros_accounts_lookup {
            let a_reserve_pk = match Pubkey::from_str(&pool_info.token_a) {
                Ok(pk) => pk,
                Err(err) => {
                    log::error!("Failed to parse a_reserve_pk for pool_info: {:?}", err);
                    continue;
                }
            };
            let b_reserve_pk = match Pubkey::from_str(&pool_info.token_b) {
                Ok(pk) => pk,
                Err(err) => {
                    log::error!("Failed to parse b_reserve_pk for pool_info: {:?}", err);
                    continue;
                }
            };
            let lp_mint_pk = match Pubkey::from_str(&pool_info.pool_mint) {
                Ok(pk) => pk,
                Err(err) => {
                    log::error!("Failed to parse lp_mint_pk for pool_info: {:?}", err);
                    continue;
                }
            };

            let a_reserve_amt = match balances.get(&a_reserve_pk) {
                Some(amount) => amount,
                None => {
                    log::error!("Failed to get balance for a_reserve_pk: {:?}", a_reserve_pk);
                    continue;
                }
            };

            let b_reserve_amt = match balances.get(&b_reserve_pk) {
                Some(amount) => amount,
                None => {
                    log::error!("Failed to get balance for b_reserve_pk: {:?}", b_reserve_pk);
                    continue;
                }
            };

            let supply = match supply_amts.get(&lp_mint_pk) {
                Some(supply_amt) => supply_amt,
                None => {
                    log::error!("Failed to get supply for lp_mint_pk: {:?}", lp_mint_pk);
                    continue;
                }
            };

            let (a_per_parent, b_per_parent) = if *supply == 0 {
                (0.0, 0.0)
            } else {
                let a_per_parent = (*a_reserve_amt as f64) / (*supply as f64);
                let b_per_parent = (*b_reserve_amt as f64) / (*supply as f64);

                (a_per_parent, b_per_parent)
            };

            let mint_underlying_schema = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                mint_pubkey: pool_info.pool_mint.clone(),
                platform_program_pubkey: SAROS_SWAP_PROGRAM.into(),
                mints: vec![
                    pool_info.token_a_mint.clone(),
                    pool_info.token_b_mint.clone(),
                ],
                time: blocktime,
                total_underlying_amounts: vec![(*a_reserve_amt).into(), (*b_reserve_amt).into()],
                mints_qty_per_one_parent: vec![a_per_parent, b_per_parent],
                discriminant_id: pool_id.to_string(),
            });
            if let Err(e) = tx.send(mint_underlying_schema) {
                log::error!("Failed to send mint underlying schema: {:?}", e);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use rust_decimal::Decimal;
    use std::fs;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    use TransactionParserTrait;

    #[tokio::test]
    async fn invoke_swap_test() {
        let txn_json = fs::read_to_string("tests/saros/txn_saros_4yXrAHKr9GP4VdQbdLxkAAGiqu98hjFkq1anDF3GqEUkRXCDryo9uyWMHKamSz1s57B4GLtEiQz1rgMa1EVMhAa9.json").unwrap();
        let decoded_txn = serde_json::from_str::<DecodedTransaction>(&txn_json).unwrap();

        let mut dooot = vec![];
        let parser = Saros;

        parser
            .invoke(&decoded_txn, &mut dooot, &Default::default())
            .await
            .unwrap();
        assert_eq!(dooot.len(), 2);

        let Dooot::MintUnderlyingsGlobal(mint_underlying) = &dooot[0] else {
            panic!("Expected a MintUnderlyings schema");
        };
        let Dooot::SwapEvent(swap) = &dooot[1] else {
            panic!("Expected a Swap schema");
        };

        let expected_mint_pubkey = "5kvBykCa3dTU2HVpikGuZ5gsuqrf7L1ywrdMBozh8Hj8";
        let expected_platform_program_pubkey = "SSwapUtytfBdBn1b9NUGG6foMVPtcWgpRU32HToDUZr";
        let expected_mints = vec![
            "So11111111111111111111111111111111111111112".to_string(),
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
        ];
        let expected_mints_qty_per_one_parent = vec![0.004436346933153051, 0.0006157050430035282];
        let expected_total_underlying_amounts =
            vec![Decimal::from(59065421815u64), Decimal::from(8197482890u64)];

        let actual_mint_pubkey = &mint_underlying.mint_pubkey;
        let actual_platform_program_pubkey = &mint_underlying.platform_program_pubkey;
        let actual_mints = &mint_underlying.mints;
        let actual_mints_qty_per_one_parent = &mint_underlying.mints_qty_per_one_parent;
        let actual_total_underlying_amounts = &mint_underlying.total_underlying_amounts;

        assert_eq!(actual_mint_pubkey, expected_mint_pubkey);
        assert_eq!(
            actual_platform_program_pubkey,
            expected_platform_program_pubkey
        );
        assert_eq!(actual_mints, &expected_mints);
        assert_eq!(
            actual_mints_qty_per_one_parent,
            &expected_mints_qty_per_one_parent
        );
        assert_eq!(
            actual_total_underlying_amounts,
            &expected_total_underlying_amounts
        );

        let expected_auth = "eatyCwmyfVuZ7XiDcZxpnRTe5YEvJSefaKwKKd6hatT";
        let expected_in_mint = "So11111111111111111111111111111111111111112";
        let expected_out_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
        let expected_in_amount = Decimal::from(24755476u64);
        let expected_out_amount = Decimal::from(3426855u64);
        let expected_parent_amm_program = Some(
            "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4"
                .parse()
                .unwrap(),
        );
        let expected_market_pubkey = Some(
            "Djxfn7zWFxFqYgwesXfq8BeAirfXwfhQmNcCousXh7G7"
                .parse()
                .unwrap(),
        );

        let actual_auth = &swap.swap_auth_pubkey;
        let actual_in_mint = &swap.in_mint_pubkey;
        let actual_out_mint = &swap.out_mint_pubkey;
        let actual_in_amount = &swap.in_amount;
        let actual_out_amount = &swap.out_amount;
        let actual_parent_amm_program = &swap.parent_amm_program_pubkey;
        let actual_market_pubkey = &swap.market_pubkey;

        assert_eq!(actual_auth, expected_auth);
        assert_eq!(actual_in_mint, expected_in_mint);
        assert_eq!(actual_out_mint, expected_out_mint);
        assert_eq!(actual_in_amount, &expected_in_amount);
        assert_eq!(actual_out_amount, &expected_out_amount);
        assert_eq!(*actual_parent_amm_program, expected_parent_amm_program);
        assert_eq!(*actual_market_pubkey, expected_market_pubkey);
    }
    #[tokio::test]
    async fn invoke_failed_test() {
        let txn_json = fs::read_to_string("tests/saros/saros_failed.json").unwrap();

        let decoded_txn = serde_json::from_str::<DecodedTransaction>(&txn_json).unwrap();

        let mut dooot = vec![];
        let parser = Saros;

        parser
            .invoke(&decoded_txn, &mut dooot, &Default::default())
            .await
            .unwrap();
        println!("dooot {:#?}", dooot);
        assert_eq!(dooot.len(), 2);

        let expected_mint_pubkey = "7jXM4b6sp7wpSBbaPVJo5QZRB432qYn83ghqe4MqYh6a";
        let expected_platform_program_pubkey = "SSwapUtytfBdBn1b9NUGG6foMVPtcWgpRU32HToDUZr";
        let expected_mints = vec![
            "7dS2EfjggAHcVWe3phzrStmSWS5LPvyYBzfQNJdyNGfw".to_string(),
            "FV7JL7ndXyxCL2QGDzLrPnfd9o3nGh1iHAuKwdaZSGFQ".to_string(),
        ];
        let expected_mints_qty_per_one_parent = vec![0.1, 100000.0];
        let expected_total_underlying_amounts = vec![
            Decimal::from(100000000u64),
            Decimal::from(100000000000000u64),
        ];
        let Dooot::MintUnderlyingsGlobal(mint_underlying) = &dooot[1] else {
            panic!("Expected a MintUnderlyings schema");
        };
        let actual_mint_pubkey = &mint_underlying.mint_pubkey;
        let actual_platform_program_pubkey = &mint_underlying.platform_program_pubkey;
        let actual_mints = &mint_underlying.mints;
        let actual_mints_qty_per_one_parent = &mint_underlying.mints_qty_per_one_parent;
        let actual_total_underlying_amounts = &mint_underlying.total_underlying_amounts;
        assert_eq!(actual_mint_pubkey, expected_mint_pubkey);
        assert_eq!(
            actual_platform_program_pubkey,
            expected_platform_program_pubkey
        );
        assert_eq!(actual_mints, &expected_mints);
        assert_eq!(
            actual_mints_qty_per_one_parent,
            &expected_mints_qty_per_one_parent
        );
        assert_eq!(
            actual_total_underlying_amounts,
            &expected_total_underlying_amounts
        );
    }
}
