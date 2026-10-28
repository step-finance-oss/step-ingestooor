use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;

use borsh::BorshDeserialize;
use rust_decimal::Decimal;
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
use step_ingestooor_idl_gens::raydium_v6_gen::raydium_v6::{
    accounts::PoolState, events::SwapEvent, utils::InstructionUnion,
};
use step_ingestooor_sdk::{
    dooot::{
        Dooot, LPInfoDooot, LPInfoUnderlyingMintVault, LiquidityChangeEventDooot,
        MintUnderlyingsGlobalDooot, SwapEventDooot, SwapFees,
    },
    trait_defs::{
        discriminatable::Discriminatable,
        pod_types::RefFromByteRef,
        transaction::{TransactionLike, TxAccountInfoStage},
    },
    utils::{
        step_utils::StepUtils,
        util_fns::{
            convert_blocktime_to_micros, get_partially_decoded_variant, get_program_id_of_ix,
        },
    },
};

use crate::{
    state_source::{AccountProvider, StateSource},
    TransactionParserFilter, TransactionParserTrait,
};

pub const RAYDIUM_V6: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";

pub struct RaydiumV6;

#[async_trait]
impl TransactionParserTrait for RaydiumV6 {
    fn get_name(&self) -> &'static str {
        "Raydium V6"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(RAYDIUM_V6.into())]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let accounts: Vec<_> = txn
            .iter_accounts()
            .filter(|a| a.post_owner_program.is_some_and(|p| p == RAYDIUM_V6))
            .collect();

        let mut swap_events: VecDeque<_> = txn
            .get_program_events_from_logs(RAYDIUM_V6)
            .iter()
            .filter_map(|e| SwapEvent::try_from_slice(e).ok())
            .collect();

        for acct in accounts {
            let Some(post) = acct.unpack_post_datum()? else {
                log::warn!("Raydium v6 post datum empty");
                continue;
            };
            let post_disc = &post[..8];
            if post_disc != PoolState::discriminator().1 {
                continue;
            }
            let pool_state = PoolState::from_bytes(&post[8..]).map_err(|e| anyhow!(e))?;
            let token_0 = pool_state.token0_mint.to_string();
            let token_1 = pool_state.token1_mint.to_string();
            let token_0_vault = pool_state.token0_vault.to_string();
            let token_1_vault = pool_state.token1_vault.to_string();
            let lp_mint = pool_state.lp_mint.to_string();
            let was_created = acct.was_created;
            if was_created {
                let lp_info = Dooot::LPInfo(LPInfoDooot {
                    time,
                    curve_type: step_ingestooor_sdk::dooot::CurveType::ConstantProduct,
                    pool: acct.pubkey.into(),
                    lp_mint: Some(lp_mint.clone()),
                    program: RAYDIUM_V6.into(),
                    underlyings: vec![
                        LPInfoUnderlyingMintVault {
                            mint: token_0.clone(),
                            vault: token_0_vault.clone(),
                        },
                        LPInfoUnderlyingMintVault {
                            mint: token_1.clone(),
                            vault: token_1_vault.clone(),
                        },
                    ],
                });
                dooot.push(lp_info);
            }

            if !was_created && acct.datum_changed {
                let pre_token0_balance = txn
                    .get_token_account_balance(&token_0_vault, TxAccountInfoStage::Pre)?
                    .ok_or_else(|| anyhow!("Raydium v6 - No pre balance for token 0"))?;
                let post_token0_balance = txn
                    .get_token_account_balance(&token_0_vault, TxAccountInfoStage::Post)?
                    .ok_or_else(|| anyhow!("Raydium v6 - No post balance for token 0"))?;
                let pre_token1_balance = txn
                    .get_token_account_balance(&token_1_vault, TxAccountInfoStage::Pre)?
                    .ok_or_else(|| anyhow!("Raydium v6 - No pre balance for token 1"))?;
                let post_token1_balance = txn
                    .get_token_account_balance(&token_1_vault, TxAccountInfoStage::Post)?
                    .ok_or_else(|| anyhow!("Raydium v6 - No post balance for token 1"))?;

                let Some(pre) = acct.unpack_pre_datum()? else {
                    bail!("Raydium v6 pre datum empty. Unreachable - it should be exist here.");
                };

                let a_diff = post_token0_balance as i128 - pre_token0_balance as i128;
                let b_diff = post_token1_balance as i128 - pre_token1_balance as i128;

                let pre_pool_state = PoolState::from_bytes(&pre[8..]).map_err(|e| anyhow!(e))?;
                if pre_pool_state.lp_supply != pool_state.lp_supply {
                    let liq_change = Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                        time,
                        owner: txn
                            .get_first_signer()
                            .ok_or(anyhow!("Unreachable - No signer?"))?,
                        pool: acct.pubkey.to_string(),
                        mints: vec![token_0.clone(), token_1.clone()],
                        amounts: vec![Decimal::from(a_diff), Decimal::from(b_diff)],
                    });
                    dooot.push(liq_change);
                }

                let lp_supply = pool_state.lp_supply;

                let (a_per_parent, b_per_parent) = if lp_supply == 0 {
                    (0.0, 0.0)
                } else {
                    let a_per_parent = (post_token0_balance as f64) / (lp_supply as f64);
                    let b_per_parent = (post_token1_balance as f64) / (lp_supply as f64);
                    (a_per_parent, b_per_parent)
                };

                let mint_underlying_dooot =
                    Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                        mint_pubkey: pool_state.lp_mint.to_string(),
                        platform_program_pubkey: RAYDIUM_V6.into(),
                        mints: vec![token_0, token_1],
                        mints_qty_per_one_parent: vec![a_per_parent, b_per_parent],
                        time,
                        total_underlying_amounts: vec![
                            post_token0_balance.into(),
                            post_token1_balance.into(),
                        ],
                        discriminant_id: acct.pubkey.to_string(),
                    });

                dooot.push(mint_underlying_dooot);
            }
        }

        let ix_tree = txn.get_ix_tree()?;
        let ix_iter = ix_tree
            .root()
            .context("UNREACHABLE - No root node")?
            .traverse_level_order()
            .skip(1);

        for node in ix_iter {
            let ix = node.data().context("UNREACHABLE - Node should be Some")?;
            let Some(ix) = get_partially_decoded_variant(ix) else {
                continue;
            };
            if ix.program_id != RAYDIUM_V6 {
                continue;
            }
            let ix_bytes = bs58::decode(&ix.data).into_vec()?;
            let ix_decoded = match InstructionUnion::try_from_slice(&ix_bytes) {
                Ok(i) => i,
                Err(e) => {
                    log::warn!("Raydium v6 - Failed to decode instruction: {:?}", e);
                    continue;
                }
            };
            let ixs_scan = node.children().filter_map(|kid| *kid.data());
            // handle only swap instructions
            match ix_decoded {
                InstructionUnion::SwapBaseInput(_) => {
                    let parent_id = node
                        .parent()
                        .and_then(|p| *p.data())
                        .and_then(get_program_id_of_ix);
                    let user = &ix.accounts[0];
                    let pool = &ix.accounts[3];
                    let ata_in = &ix.accounts[4];
                    let ata_out = &ix.accounts[5];
                    let ata_in_data = txn.get_token_account_data_by_key(ata_in)?;
                    let ata_out_data = txn.get_token_account_data_by_key(ata_out)?;
                    let mint_in = ata_in_data.mint;
                    let mint_out = ata_out_data.mint;
                    let txfer_amounts = txn.get_total_transfer_amounts_for_pubkey_ui_ixs(
                        ixs_scan,
                        &[user],
                        &[&mint_in],
                        &[&mint_out],
                    )?;

                    let amount_in = txfer_amounts.get_total_in_amount_i128(&mint_in);
                    let amount_out = txfer_amounts.get_total_out_amount_i128(&mint_out);

                    let swap_row = Dooot::SwapEvent(SwapEventDooot {
                        time,
                        in_amount: amount_in.into(),
                        out_amount: amount_out.into(),
                        in_mint_pubkey: mint_in.to_string(),
                        out_mint_pubkey: mint_out.to_string(),
                        program_pubkey: RAYDIUM_V6.to_string(),
                        swap_auth_pubkey: user.to_string(),
                        market_pubkey: Some(pool.to_string()),
                        parent_amm_program_pubkey: parent_id,
                        fee_mint_is_in_mint: None,
                        referring_platform_account_pubkey: None,
                        platform_referral_fee: None,
                        fees: if let Some(swap_e) = swap_events.pop_front() {
                            vec![SwapFees {
                                amount: Decimal::from(swap_e.input_amount),
                                mint: mint_in.to_string(),
                            }]
                        } else {
                            vec![]
                        },
                        slippage_bps: None,
                        tx_sig: txn.get_signature().to_string(),
                    });

                    dooot.push(swap_row);
                }

                InstructionUnion::SwapBaseOutput(_) => {
                    let parent_id = node
                        .parent()
                        .and_then(|p| *p.data())
                        .and_then(get_program_id_of_ix);
                    let user = &ix.accounts[0];
                    let pool = &ix.accounts[3];
                    let ata_in = &ix.accounts[4];
                    let ata_out = &ix.accounts[5];
                    let ata_in_data = txn.get_token_account_data_by_key(ata_in)?;
                    let ata_out_data = txn.get_token_account_data_by_key(ata_out)?;
                    let mint_in = ata_in_data.mint;
                    let mint_out = ata_out_data.mint;
                    let txfer_amounts = txn.get_total_transfer_amounts_for_pubkey_ui_ixs(
                        ixs_scan,
                        &[user],
                        &[&mint_in],
                        &[&mint_out],
                    )?;

                    let amount_in = txfer_amounts.get_total_in_amount_i128(&mint_in);
                    let amount_out = txfer_amounts.get_total_out_amount_i128(&mint_out);

                    let swap_row = Dooot::SwapEvent(SwapEventDooot {
                        time,
                        in_amount: amount_in.into(),
                        out_amount: amount_out.into(),
                        in_mint_pubkey: mint_in.to_string(),
                        out_mint_pubkey: mint_out.to_string(),
                        program_pubkey: RAYDIUM_V6.to_string(),
                        swap_auth_pubkey: user.to_string(),
                        market_pubkey: Some(pool.to_string()),
                        parent_amm_program_pubkey: parent_id,
                        fee_mint_is_in_mint: None,
                        referring_platform_account_pubkey: None,
                        platform_referral_fee: None,
                        fees: if let Some(swap_e) = swap_events.pop_front() {
                            vec![SwapFees {
                                amount: Decimal::from(swap_e.output_amount),
                                mint: mint_out.to_string(),
                            }]
                        } else {
                            vec![]
                        },
                        slippage_bps: None,
                        tx_sig: txn.get_signature().to_string(),
                    });

                    dooot.push(swap_row);
                }

                _ => continue,
            }
        }

        Ok(())
    }
}

impl StateSource for RaydiumV6 {
    fn get_name(&self) -> &'static str {
        "Raydium V6"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let blocktime = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let raydium_pk = RAYDIUM_V6.parse::<Pubkey>()?;
        let raydium_accts_iter = acct_provider.program_accounts(raydium_pk, None);

        let mut raydium_accounts_lookup = HashMap::new();
        let mut vault_accounts = HashSet::new();
        let mut lp_mints = HashSet::new();
        let mut balances = HashMap::new();
        let mut supply_amts = HashMap::new();

        for acct in raydium_accts_iter {
            let pk = acct.pubkey;
            let data = acct.account.data();
            if data[0..8] != *PoolState::discriminator().1 {
                continue;
            }

            let pool_info = PoolState::from_bytes(&data[8..]).map_err(|e| anyhow!(e))?;
            let a_vault_pk = pool_info.token0_vault;
            let b_vault_pk = pool_info.token1_vault;
            let lp_mint_pk = pool_info.lp_mint;
            vault_accounts.insert(a_vault_pk);
            vault_accounts.insert(b_vault_pk);
            lp_mints.insert(lp_mint_pk);

            let lp_info_dooot = Dooot::LPInfo(LPInfoDooot {
                time: blocktime,
                curve_type: step_ingestooor_sdk::dooot::CurveType::ConstantProduct,
                lp_mint: Some(lp_mint_pk.to_string()),
                pool: pk.to_string(),
                program: RAYDIUM_V6.into(),
                underlyings: vec![
                    LPInfoUnderlyingMintVault {
                        mint: pool_info.token0_mint.to_string(),
                        vault: a_vault_pk.to_string(),
                    },
                    LPInfoUnderlyingMintVault {
                        mint: pool_info.token1_mint.to_string(),
                        vault: b_vault_pk.to_string(),
                    },
                ],
            });

            tx.send(lp_info_dooot)?;

            raydium_accounts_lookup.insert(pk, *pool_info);
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

        for (pool_id, pool_info) in raydium_accounts_lookup {
            let a_reserve_pk = pool_info.token0_vault;
            let b_reserve_pk = pool_info.token1_vault;
            let lp_mint_pk = pool_info.lp_mint;

            let a_reserve_amt = balances.get(&a_reserve_pk).context("Missing a reserve")?;
            let b_reserve_amt = balances.get(&b_reserve_pk).context("Missing b reserve")?;
            let supply = supply_amts.get(&lp_mint_pk).context("Missing supply")?;

            let (a_per_parent, b_per_parent) = if *supply == 0 {
                (0.0, 0.0)
            } else {
                let a_per_parent = (*a_reserve_amt as f64) / (*supply as f64);
                let b_per_parent = (*b_reserve_amt as f64) / (*supply as f64);
                (a_per_parent, b_per_parent)
            };

            let mint_underlying_dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                mint_pubkey: lp_mint_pk.to_string(),
                platform_program_pubkey: RAYDIUM_V6.into(),
                mints: vec![
                    pool_info.token0_mint.to_string(),
                    pool_info.token1_mint.to_string(),
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
