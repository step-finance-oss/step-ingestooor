use std::{
    collections::{hash_map::Entry, HashMap},
    sync::mpsc::SyncSender,
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
use solana_transaction_status::{UiInstruction, UiParsedInstruction};
use step_ingestooor_sdk::{
    dooot::{
        CurveType, Dooot, LPInfoDooot, LPInfoUnderlyingMintVault, LiquidityChangeEventDooot,
        MintUnderlyingsGlobalDooot, SwapEventDooot,
    },
    trait_defs::transaction::{TransactionLike, TransferDirection},
    utils::{
        step_account_reader::StepAccountReader,
        step_utils::StepUtils,
        util_fns::{convert_blocktime_to_micros, get_partially_decoded_variant},
    },
};

use crate::{
    state_source::{AccountProvider, StateSource},
    TransactionParserFilter, TransactionParserTrait,
};

pub const PUMP_FUN_AMM_PROGRAM: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";

pub struct PumpFunAMM;

pub const PUMP_FUN_POOL_DISCRIMINANT: &[u8] = &[0xf1, 0x9a, 0x6d, 0x04, 0x11, 0xb1, 0x6d, 0xbc];

#[derive(Debug, Clone)]
pub struct PumpFunPoolInfo {
    pub base_mint: String,
    pub quote_mint: String,
    pub lp_mint: String,
    pub base_mint_ata: String,
    pub quote_mint_ata: String,
    pub lp_supply: u64,
}

#[async_trait]
impl TransactionParserTrait for PumpFunAMM {
    fn get_name(&self) -> &'static str {
        "PumpFunAMM"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            PUMP_FUN_AMM_PROGRAM.into(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let acct_iter = txn
            .iter_accounts()
            .filter(|a| a.post_owner_program == Some(PUMP_FUN_AMM_PROGRAM));

        let mut pools = HashMap::new();

        for acct in acct_iter {
            let Some(post) = acct.unpack_post_datum()? else {
                continue;
            };

            let disc = post.get(..8).context("No disc for Pump Pool")?;

            match disc {
                PUMP_FUN_POOL_DISCRIMINANT => {
                    let pool_info = Self::extract_pool_info(&post)?;
                    pools.insert(acct.pubkey.to_string(), pool_info.clone());

                    let base_ata = &pool_info.base_mint_ata;
                    let quote_ata = &pool_info.quote_mint_ata;

                    if acct.was_created {
                        let lp_info_dooot = Dooot::LPInfo(LPInfoDooot {
                            time,
                            curve_type: CurveType::ConstantProduct,
                            lp_mint: Some(pool_info.lp_mint.clone()),
                            pool: acct.pubkey.to_string(),
                            underlyings: vec![
                                LPInfoUnderlyingMintVault {
                                    mint: pool_info.base_mint.clone(),
                                    vault: base_ata.clone(),
                                },
                                LPInfoUnderlyingMintVault {
                                    mint: pool_info.quote_mint.clone(),
                                    vault: quote_ata.clone(),
                                },
                            ],
                            program: PUMP_FUN_AMM_PROGRAM.to_string(),
                        });

                        dooot.push(lp_info_dooot);
                    }

                    let Ok((pre_base, post_base)) = txn.get_pre_and_post_token_balance(base_ata)
                    else {
                        continue;
                    };
                    let Ok((pre_quote, post_quote)) = txn.get_pre_and_post_token_balance(quote_ata)
                    else {
                        continue;
                    };

                    let base_changed = post_base.is_some() && pre_base != post_base;
                    let quote_changed = post_quote.is_some() && pre_quote != post_quote;

                    if !base_changed && !quote_changed && !acct.datum_changed {
                        continue;
                    }

                    let post_base_balance = post_base
                        .map(|b| b.ui_token_amount.amount.parse::<u64>())
                        .transpose()?;
                    let post_quote_balance = post_quote
                        .map(|b| b.ui_token_amount.amount.parse::<u64>())
                        .transpose()?;

                    let (Some(post_base_balance), Some(post_quote_balance)) =
                        (post_base_balance, post_quote_balance)
                    else {
                        continue;
                    };

                    let mints_qty_per_one_parent = if pool_info.lp_supply == 0 {
                        vec![0.0, 0.0]
                    } else {
                        vec![
                            (post_base_balance as f64) / (pool_info.lp_supply as f64),
                            (post_quote_balance as f64) / (pool_info.lp_supply as f64),
                        ]
                    };

                    let mu_dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                        discriminant_id: acct.pubkey.to_string(),
                        mint_pubkey: pool_info.lp_mint,
                        mints: vec![pool_info.base_mint, pool_info.quote_mint],
                        mints_qty_per_one_parent,
                        platform_program_pubkey: PUMP_FUN_AMM_PROGRAM.to_string(),
                        time,
                        total_underlying_amounts: vec![
                            post_base_balance.into(),
                            post_quote_balance.into(),
                        ],
                    });

                    dooot.push(mu_dooot);
                }
                _ => {
                    continue;
                }
            }
        }

        let tree = txn.get_ix_tree()?;
        let root = tree.root().context("No root node found in ix tree")?;
        let ix_iter = root.traverse_level_order().skip(1);
        let sig = txn.get_signature();

        let txn_signers = txn
            .get_account_keys()
            .iter()
            .filter(|a| a.signer)
            .collect::<Vec<_>>();

        for node in ix_iter {
            let ix = node.data().context("No ix found in node")?;
            let Some(ix) = get_partially_decoded_variant(ix) else {
                continue;
            };

            if ix.program_id != PUMP_FUN_AMM_PROGRAM {
                continue;
            }

            let Some((pool_pk, pool_info)) = ix
                .accounts
                .iter()
                .find(|a| pools.contains_key(a.as_str()))
                .and_then(|a| Some((a.as_str(), pools.get(a.as_str())?)))
            else {
                continue;
            };

            let Some(user) = ix
                .accounts
                .iter()
                .find(|&a| txn_signers.iter().any(|s| s.pubkey == *a))
            else {
                // No signer that is also in ixs here?
                continue;
            };

            let parent_program_id = node
                .parent()
                .and_then(|n| *n.data())
                .and_then(|ix| match ix {
                    UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(p_ix)) => {
                        Some(p_ix.program_id.clone())
                    }
                    _ => None,
                });

            let lp_mint = &pool_info.lp_mint;
            let token_a_mint = &pool_info.base_mint;
            let token_b_mint = &pool_info.quote_mint;

            let ixs_scan = node
                .traverse_level_order()
                .skip(1)
                .filter_map(|kid| *kid.data());

            let txfer_amounts = txn.get_total_transfer_amounts_for_pubkey_ui_ixs(
                ixs_scan,
                &[user],
                &[lp_mint, token_a_mint, token_b_mint],
                &[lp_mint, token_a_mint, token_b_mint],
            )?;

            let lp_mint_direction = txfer_amounts.get_direction(lp_mint, user);
            let token_a_direction = txfer_amounts.get_direction(token_a_mint, user);
            let token_b_direction = txfer_amounts.get_direction(token_b_mint, user);

            match (lp_mint_direction, token_a_direction, token_b_direction) {
                (_, TransferDirection::Zero, TransferDirection::Zero) => {
                    // No underlyings moved, no need to emit any events
                    // We don't care about LP Mint in this case
                    continue;
                }
                (TransferDirection::Receive | TransferDirection::Send, _, _) => {
                    // Exchange of lp tokens with underlyings, emit liquidity change event
                    // get_in_amount will be either positive or negative depending on the direction
                    // This is relative to the `user`, but can also be flipped to be relative to the pool
                    // Deposit => positive change, Withdraw => negative change
                    let change_amt_a = txfer_amounts.get_in_amount_i128(token_a_mint, user);
                    let change_amt_b = txfer_amounts.get_in_amount_i128(token_b_mint, user);
                    if change_amt_a == 0 && change_amt_b == 0 {
                        continue;
                    }

                    let liq_change_dooot = Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                        amounts: vec![change_amt_a.into(), change_amt_b.into()],
                        mints: vec![token_a_mint.clone(), token_b_mint.clone()],
                        owner: user.to_string(),
                        pool: pool_pk.to_string(),
                        time,
                    });

                    dooot.push(liq_change_dooot);
                }
                (TransferDirection::Zero, dir_a, dir_b) => {
                    // No LP tokens moved, but underlying tokens did, this is a Swap, or a Fee Claim
                    let mut in_mint = token_a_mint;
                    let mut out_mint = token_b_mint;

                    let mut in_amount = txfer_amounts.get_in_amount_i128(in_mint, user);
                    let mut out_amount = txfer_amounts.get_out_amount_i128(out_mint, user);

                    if dir_a == dir_b {
                        // Fee Claim, emit liq change for both mints
                        let abs_in = in_amount.abs();
                        let abs_out = out_amount.abs();
                        let sign = match dir_a {
                            TransferDirection::Receive => -1,
                            TransferDirection::Send => 1,
                            TransferDirection::Zero => 0,
                        };
                        let amt_a = abs_in * sign;
                        let amt_b = abs_out * sign;
                        let liq_change_dooot =
                            Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                                amounts: vec![amt_a.into(), amt_b.into()],
                                mints: vec![token_a_mint.clone(), token_b_mint.clone()],
                                owner: user.to_string(),
                                pool: pool_pk.to_string(),
                                time,
                            });

                        dooot.push(liq_change_dooot);
                    } else {
                        if in_amount == 0 || out_amount == 0 {
                            continue;
                        }

                        if in_amount < 0 && out_amount < 0 {
                            // Just switch n flip signs
                            std::mem::swap(&mut in_mint, &mut out_mint);
                            std::mem::swap(&mut in_amount, &mut out_amount);
                            in_amount = -in_amount;
                            out_amount = -out_amount;
                        }

                        if in_amount < 0 || out_amount < 0 {
                            // An amount is still negative after doing the swap and abs()
                            continue;
                        }

                        let swap_dooot = Dooot::SwapEvent(SwapEventDooot {
                            time,
                            in_amount: in_amount.into(),
                            in_mint_pubkey: in_mint.clone(),
                            out_amount: out_amount.into(),
                            out_mint_pubkey: out_mint.clone(),
                            program_pubkey: PUMP_FUN_AMM_PROGRAM.to_string(),
                            swap_auth_pubkey: user.to_string(),
                            market_pubkey: Some(pool_pk.to_string()),
                            parent_amm_program_pubkey: parent_program_id,
                            fee_mint_is_in_mint: None,
                            referring_platform_account_pubkey: None,
                            platform_referral_fee: None,
                            fees: vec![],
                            slippage_bps: None,
                            tx_sig: sig.to_string(),
                        });

                        dooot.push(swap_dooot);
                    }
                }
            }
        }

        Ok(())
    }
}

impl PumpFunAMM {
    pub fn extract_pool_info(data: &[u8]) -> Result<PumpFunPoolInfo> {
        let mut reader = StepAccountReader::new(data);
        reader.move_index_to(43);

        let base_mint = reader.read_next_pubkey()?.to_string();
        let quote_mint = reader.read_next_pubkey()?.to_string();
        let lp_mint = reader.read_next_pubkey()?.to_string();

        let base_mint_ata = reader.read_next_pubkey()?.to_string();
        let quote_mint_ata = reader.read_next_pubkey()?.to_string();
        let lp_supply = reader.read_next_le_u64()?;

        Ok(PumpFunPoolInfo {
            base_mint,
            quote_mint,
            lp_mint,
            base_mint_ata,
            quote_mint_ata,
            lp_supply,
        })
    }
}

impl StateSource for PumpFunAMM {
    fn get_name(&self) -> &'static str {
        "PumpFunAMM"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        let blocktime = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let pool_iter = acct_provider.program_accounts(
            PUMP_FUN_AMM_PROGRAM.parse::<Pubkey>()?,
            Some(PUMP_FUN_POOL_DISCRIMINANT),
        );

        let mut pools = HashMap::new();
        let mut atas = HashMap::new();

        for acct in pool_iter {
            let Ok(pool_info) = Self::extract_pool_info(acct.account.data()) else {
                continue;
            };

            atas.insert(pool_info.base_mint_ata.clone(), None);
            atas.insert(pool_info.quote_mint_ata.clone(), None);
            pools.insert(acct.pubkey.to_string(), pool_info);
        }

        let ata_iter = acct_provider.all_accounts();

        for acct in ata_iter {
            if let Entry::Occupied(mut e) = atas.entry(acct.pubkey.to_string()) {
                let data = acct.account.data();
                let balance_bytes = &data[64..72].try_into()?;
                let balance = u64::from_le_bytes(*balance_bytes);
                e.insert(Some(balance));
            }
        }

        for (pool_pk, pool_info) in pools.iter() {
            let lp_info_dooot = Dooot::LPInfo(LPInfoDooot {
                time: blocktime,
                curve_type: CurveType::ConstantProduct,
                lp_mint: Some(pool_info.lp_mint.clone()),
                pool: pool_pk.to_string(),
                underlyings: vec![
                    LPInfoUnderlyingMintVault {
                        mint: pool_info.base_mint.clone(),
                        vault: pool_info.base_mint_ata.clone(),
                    },
                    LPInfoUnderlyingMintVault {
                        mint: pool_info.quote_mint.clone(),
                        vault: pool_info.quote_mint_ata.clone(),
                    },
                ],
                program: PUMP_FUN_AMM_PROGRAM.to_string(),
            });

            tx.send(lp_info_dooot)?;

            let base_ata = &pool_info.base_mint_ata;
            let quote_ata = &pool_info.quote_mint_ata;

            let Some(base_balance) = atas.get(base_ata).and_then(|b| *b) else {
                continue;
            };
            let Some(quote_balance) = atas.get(quote_ata).and_then(|b| *b) else {
                continue;
            };

            let lp_supply = pool_info.lp_supply;
            let mints_qty_per_one_parent = if lp_supply == 0 {
                vec![0.0, 0.0]
            } else {
                vec![
                    (base_balance as f64) / (lp_supply as f64),
                    (quote_balance as f64) / (lp_supply as f64),
                ]
            };

            let mu_dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                discriminant_id: pool_pk.to_string(),
                mint_pubkey: pool_info.lp_mint.clone(),
                mints: vec![pool_info.base_mint.clone(), pool_info.quote_mint.clone()],
                mints_qty_per_one_parent,
                platform_program_pubkey: PUMP_FUN_AMM_PROGRAM.to_string(),
                time: blocktime,
                total_underlying_amounts: vec![base_balance.into(), quote_balance.into()],
            });

            tx.send(mu_dooot)?;
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
    async fn test_extract_pool_info() {
        let file = fs::read_to_string("./tests/pump_fun/lp_1.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let parser = PumpFunAMM {};
        let (step_utils, _) = init_test_extras_bare().await;

        let mut dooots = vec![];
        parser.invoke(&txn, &mut dooots, &step_utils).await.unwrap();
    }
}
