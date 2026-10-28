/**
 * Good doc:
 * https://github.com/raydium-io/raydium-docs/blob/master/dev-resources/raydium-hybrid-amm-dev-doc.pdf
 */
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc::SyncSender,
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::NaiveDateTime;

use crate::state_source::{AccountProvider, StateSource};
use borsh::BorshDeserialize;
use log::{debug, error, trace};
use rust_decimal::Decimal;
use slab_tree::NodeRef;
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
use solana_transaction_status::{
    UiInstruction, UiParsedInstruction, UiPartiallyDecodedInstruction,
};
use step_ingestooor_sdk::{
    dooot::{
        CurveType, Dooot, LPInfoDooot, LPInfoUnderlyingMintVault, MintUnderlyingsGlobalDooot,
        SwapEventDooot, SwapFees,
    },
    trait_defs::transaction::TransactionLike,
    utils::{
        constants::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID},
        step_utils::StepUtils,
        util_fns::convert_blocktime_to_micros,
    },
};

#[derive(BorshDeserialize, Debug)]
struct RaydiumAmmPoolV4 {
    pub status: u64,
    pub _nonce: u64,
    pub _order_num: u64,
    pub _depth: u64,
    pub _coin_decimals: u64,
    pub _pc_decimals: u64,
    pub _state: u64,
    pub _reset_flag: u64,
    pub _min_size: u64,
    pub _vol_max_cut_ratio: u64,
    pub _amount_wave_ratio: u64,
    pub _coin_lot_size: u64,
    pub _pc_lot_size: u64,
    pub _min_price_multiplier: u64,
    pub _max_price_multiplier: u64,
    pub _system_decimals_value: u64,
    // Fees
    pub _min_separate_numerator: u64,
    pub _min_separate_denominator: u64,
    pub _trade_fee_numerator: u64,
    pub _trade_fee_denominator: u64,
    pub _pnl_numerator: u64,
    pub _pnl_denominator: u64,
    pub swap_fee_numerator: u64,
    pub swap_fee_denominator: u64,
    // OutPutData
    pub need_take_pnl_coin: u64,
    pub need_take_pnl_pc: u64,
    pub _total_pnl_pc: u64,
    pub _total_pnl_coin: u64,
    pub _pool_total_deposit_pc: u128,
    pub _pool_total_deposit_coin: u128,
    pub _swap_coin_in_amount: u128,
    pub _swap_pc_out_amount: u128,
    pub _swap_coin2_pc_fee: u64,
    pub _swap_pc_in_amount: u128,
    pub _swap_coin_out_amount: u128,
    pub _swap_pc2_coin_fee: u64,
    // Accounts
    pub pool_coin_token_account: Pubkey,
    pub pool_pc_token_account: Pubkey,
    pub coin_mint_address: Pubkey,
    pub pc_mint_address: Pubkey,
    pub lp_mint_address: Pubkey,
    pub amm_open_orders: Pubkey,
    pub serum_market: Pubkey,
    pub _serum_program_id: Pubkey,
    pub _amm_target_orders: Pubkey,
    pub _pool_withdraw_queue: Pubkey,
    pub _pool_temp_lp_token_account: Pubkey,
    pub _amm_owner: Pubkey,
    pub lp_reserve: u64,
    pub _padding: [u64; 3],
}

enum LiquidityPoolStatus {
    Uninitialized,
    _Initialized,
    Disabled,
    _RemoveLiquidityOnly,
    _LiquidityOnly,
    _OrderBook,
    _Swap,
    _WaitingForStart,
}
pub struct RaydiumAmmV4;

const RAYDIUM_AMM_V4_PROGRAM: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";

#[async_trait]
impl crate::TransactionParserTrait for RaydiumAmmV4 {
    fn get_name(&self) -> &'static str {
        "Raydium AMM"
    }
    fn get_filters(&self) -> Vec<crate::TransactionParserFilter> {
        vec![crate::TransactionParserFilter::AccountId(
            RAYDIUM_AMM_V4_PROGRAM.to_string(),
        )]
    }
    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = decoded_transaction.get_block_time();
        let accts = decoded_transaction.get_account_keys();
        let ixs = decoded_transaction.get_ix_tree()?;

        debug!(
            "Raydium AMM V4 reviewing {} ixs in {}",
            ixs.root().iter().len(),
            decoded_transaction.get_signature()
        );

        // let test = decoded_transaction.get_ix_tree()?;
        // trace!("Raydium AMM V4 ix tree: {:#?}", test);

        //the pubkeys of affected pools
        let mut pool_pubkeys = HashSet::new();
        //the raydium ixs, and their parent program id
        let mut ray_ixs = vec![];
        let mut pool_map: HashMap<String, (u64, u64)> = HashMap::new();

        //iterate the ixs and build our 2 lists
        let root = ixs
            .root()
            .context("UNREACHABLE - Tree should have a root")?;

        for a in root.traverse_level_order().skip(1) {
            match a
                .data()
                .context("UNREACHABLE - Node data should be Some(..)")?
            {
                UiInstruction::Parsed(UiParsedInstruction::Parsed(parsed)) => {
                    //raydium ixs aren't parsed
                    if parsed.program_id == RAYDIUM_AMM_V4_PROGRAM {
                        return Err(anyhow::anyhow!(
                            "Raydium AMM V4 instruction was fully parsed?!"
                        ));
                    }
                }
                UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(partial_dec)) => {
                    if partial_dec.program_id == RAYDIUM_AMM_V4_PROGRAM {
                        //grab the pool account
                        let pool_account = get_pool_account(partial_dec);
                        if let Some(pool_account) = pool_account {
                            pool_pubkeys.insert(pool_account);
                        }
                        //add the ix and its parent program id
                        ray_ixs.push(a);
                    }
                }
                UiInstruction::Compiled(comp) => {
                    if accts[comp.program_id_index as usize].pubkey == RAYDIUM_AMM_V4_PROGRAM {
                        return Err(anyhow::anyhow!("Raydium AMM V4 instruction was compiled?!"));
                    }
                }
            }
        }

        //create the underlyings for each pool
        for pool_pubkey in pool_pubkeys {
            let pool = decoded_transaction
                .get_account_data_after(pool_pubkey.as_str())?
                .map(|data| RaydiumAmmPoolV4::try_from_slice(&data));
            match pool {
                None => {
                    error!(
                        "Failed to find Raydium AMM V4 pool data for {pool_pubkey} (tx {})",
                        decoded_transaction.get_signature()
                    );
                }
                Some(Err(e)) => {
                    error!(
                        "Failed to decode Raydium AMM V4 pool data for {pool_pubkey} (tx {}): {e}",
                        decoded_transaction.get_signature()
                    );
                }
                Some(Ok(pool)) => {
                    let pool_info = get_lp_info(pool_pubkey, &pool, time);
                    dooot.push(Dooot::LPInfo(pool_info));
                    pool_map.insert(
                        pool_pubkey.to_string(),
                        (pool.swap_fee_numerator, pool.swap_fee_denominator),
                    );

                    //get the underlying info
                    let u = get_underlying_info(pool_pubkey, &pool, decoded_transaction).await;
                    if let Ok(Some(u)) = u {
                        dooot.push(Dooot::MintUnderlyingsGlobal(u));
                    } else if let Err(e) = u {
                        error!("Failed to get underlying info for Raydium AMM V4 pool {pool_pubkey} (tx {}): {e}", decoded_transaction.get_signature());
                    }
                }
            }
        }

        //create the token swap for any swap ixs
        for node in ray_ixs {
            let swap = get_token_swap(decoded_transaction, &node, &pool_map);
            if let Ok(Some(swap)) = swap {
                dooot.push(Dooot::SwapEvent(swap));
            } else if let Err(e) = swap {
                error!(
                    "Failed to get token swap for Raydium AMM V4 instruction (tx {}): {e}",
                    decoded_transaction.get_signature()
                );
            }
        }

        Ok(())
    }
}

fn get_pool_account(partial_dec: &UiPartiallyDecodedInstruction) -> Option<&String> {
    if partial_dec.program_id == RAYDIUM_AMM_V4_PROGRAM {
        let disc = bs58::decode(&partial_dec.data).into_vec().ok()?[0];
        match disc {
            //create
            1 => Some(&partial_dec.accounts[4]),
            //add, remove, swaps
            3 | 4 | 9 | 11 => Some(&partial_dec.accounts[1]),
            _ => None,
        }
    } else {
        None
    }
}

fn get_token_swap(
    decoded_transaction: &impl TransactionLike,
    node: &NodeRef<Option<&UiInstruction>>,
    pool_map: &HashMap<String, (u64, u64)>,
) -> Result<Option<SwapEventDooot>> {
    let ix = match node
        .data()
        .context("UNREACHABLE - Node data should be Some(..)")?
    {
        UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(a)) => a,
        _ => {
            return Err(anyhow::anyhow!(
                "Raydium AMM V4 swap instruction was not partially decoded"
            ))
        }
    };

    trace!("Raydium AMM V4 swap decoding {}", ix.data);

    let ix_bytes = bs58::decode(&ix.data).into_vec()?;
    trace!(
        "Raydium AMM V4 swap instruction has discriminator: {}",
        ix_bytes[0],
    );
    let swap = match ix_bytes[0] {
        //these are the 2 discriminators for swapping
        9 | 11 => {
            //get the child ixs with token program id
            //these will be the token account transfers
            let child_ixs: Vec<_> = node
                .children()
                .filter_map(|i| {
                    let ix = i.data().as_deref()?;
                    match ix {
                        UiInstruction::Parsed(UiParsedInstruction::Parsed(a)) => {
                            if a.program_id == TOKEN_PROGRAM_ID
                                || a.program_id == TOKEN_2022_PROGRAM_ID
                            {
                                Some(ix)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                })
                .collect();
            debug!(
                "Raydium AMM V4 swap instruction  in {} has {} child ixs",
                decoded_transaction.get_signature(),
                child_ixs.len()
            );
            if child_ixs.len() != 2 {
                return Err(anyhow::anyhow!(
                    "Raydium AMM V4 swap instruction has invalid number of child ixs: {}",
                    child_ixs.len()
                ));
            }

            //get the parsed object for the token account transfer out
            let (in_mint, in_amt, auth) =
                get_xfer_mint_amount_and_authority(child_ixs[0], decoded_transaction)?;
            //get the parsed object for the token account transfer in
            let (out_mint, out_amt, _) =
                get_xfer_mint_amount_and_authority(child_ixs[1], decoded_transaction)?;

            let parent = node
                .parent()
                .and_then(|a| {
                    a.data().map(|a| match a {
                        UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(a)) => {
                            Some(a.program_id.clone())
                        }
                        _ => None,
                    })
                })
                .flatten();

            let pool_pubkey = get_pool_account(ix);

            let Some(pool_pubkey) = pool_pubkey else {
                return Err(anyhow::anyhow!(
                    "Raydium AMM V4 swap instruction has no poolId",
                ));
            };

            let fees = if let Some((num, denom)) = pool_map.get(pool_pubkey) {
                vec![SwapFees {
                    mint: in_mint.to_string(),
                    amount: Decimal::from(in_amt * num / denom),
                }]
            } else {
                vec![]
            };

            let swap = SwapEventDooot {
                time: decoded_transaction.get_block_time(),
                program_pubkey: RAYDIUM_AMM_V4_PROGRAM.to_string(),
                swap_auth_pubkey: auth,
                in_mint_pubkey: in_mint,
                out_mint_pubkey: out_mint,
                in_amount: Decimal::from(in_amt),
                out_amount: Decimal::from(out_amt),
                parent_amm_program_pubkey: parent,
                fee_mint_is_in_mint: None,
                platform_referral_fee: None,
                referring_platform_account_pubkey: None,
                market_pubkey: Some(pool_pubkey.to_string()),
                fees,
                slippage_bps: None,
                tx_sig: decoded_transaction.get_signature().to_string(),
            };

            Some(swap)
        }
        _ => None,
    };
    trace!(
        "Raydium AMM V4 tx {} has swap dooot: {:#?}",
        decoded_transaction.get_signature(),
        swap,
    );
    Ok(swap)
}

fn get_lp_info(pool_pubkey: &str, pool: &RaydiumAmmPoolV4, time: NaiveDateTime) -> LPInfoDooot {
    LPInfoDooot {
        pool: pool_pubkey.to_string(),
        time,
        program: RAYDIUM_AMM_V4_PROGRAM.to_string(),
        lp_mint: Some(pool.lp_mint_address.to_string()),
        underlyings: vec![
            LPInfoUnderlyingMintVault {
                mint: pool.coin_mint_address.to_string(),
                vault: pool.pool_coin_token_account.to_string(),
            },
            LPInfoUnderlyingMintVault {
                mint: pool.pc_mint_address.to_string(),
                vault: pool.pool_pc_token_account.to_string(),
            },
        ],
        curve_type: CurveType::ConstantProduct,
    }
}

fn get_xfer_mint_amount_and_authority(
    ix: &UiInstruction,
    decoded_transaction: &impl TransactionLike,
) -> Result<(String, u64, String)> {
    let obj = match ix {
        UiInstruction::Parsed(UiParsedInstruction::Parsed(a)) => Some(a),
        _ => None,
    }
    .and_then(|p| p.parsed.as_object())
    .and_then(|p| p.get("info"))
    .and_then(|info| info.as_object())
    .ok_or_else(|| anyhow::anyhow!("Cannot get info from parsed ix: {:#?}", ix))?;
    //get the amount for the xfer
    let amt = obj
        .get("amount")
        .and_then(|a| a.as_str())
        .map(|amt| amt.parse::<u64>())
        .ok_or_else(|| anyhow::anyhow!("Cannot get amount from parsed ix: {:#?}", ix))??;
    debug!("Raydium AMM V4 swap instruction has xfer: {:#?}", amt);
    //get the mint for the xfer
    let token_acct = obj
        .get("source")
        .and_then(|s| s.as_str())
        .ok_or_else(|| anyhow::anyhow!("Cannot get token account from parsed ix: {:#?}", ix))?;
    //get the authority for the xfer
    let auth = obj
        .get("authority")
        .and_then(|a| a.as_str().map(|a| a.to_string()))
        .ok_or_else(|| anyhow::anyhow!("Cannot get authority from parsed ix: {:#?}", ix))?;
    let mint = decoded_transaction
        .get_token_account_data_by_key(token_acct)?
        .mint;
    Ok((mint, amt, auth))
}

async fn get_underlying_info(
    pool_pubkey: &String,
    pool: &RaydiumAmmPoolV4,
    decoded_transaction: &impl TransactionLike,
) -> Result<Option<MintUnderlyingsGlobalDooot>> {
    //some invalid state checks
    if pool.status == LiquidityPoolStatus::Disabled as u64 {
        return Ok(None);
    };
    if pool.status == LiquidityPoolStatus::Uninitialized as u64 {
        return Ok(None);
    };
    if pool.lp_mint_address == Pubkey::default() {
        return Ok(None);
    };
    if pool.pc_mint_address == pool.serum_market {
        return Ok(None);
    };

    //get the amount of token in the pool
    let pool_base_token_total = match decoded_transaction
        .get_post_token_account_balance(pool.pool_coin_token_account.to_string().as_str())
    {
        Ok(Some(b)) => b,
        Ok(None) => {
            error!(
                "Failed to find pool coin token account {} for Raydium AMM V4 pool {}",
                pool.pool_coin_token_account, pool_pubkey,
            );
            return Ok(None);
        }
        Err(e) => {
            error!(
                "Failed to get pool coin token account {} balance for Raydium AMM V4 pool {}: {e}",
                pool.pool_coin_token_account, pool_pubkey,
            );
            return Ok(None);
        }
    };
    let pool_quote_token_total = match decoded_transaction
        .get_post_token_account_balance(pool.pool_pc_token_account.to_string().as_str())
    {
        Ok(Some(b)) => b,
        Ok(None) => {
            error!(
                "Failed to find pool pc token account {} for Raydium AMM V4 pool {}",
                pool.pool_pc_token_account, pool_pubkey,
            );
            return Ok(None);
        }
        Err(e) => {
            error!(
                "Failed to get pool pc token account {} balance for Raydium AMM V4 pool {}: {e}",
                pool.pool_pc_token_account, pool_pubkey,
            );
            return Ok(None);
        }
    };
    debug!(
        "Raydium AMM V4 pool {} has base token total of {} and quote token total of {}",
        pool_pubkey, pool_base_token_total, pool_quote_token_total
    );

    let (oo_base_token_total, oo_quote_token_total) = {
        //get the orderbook orders if the pool is in state Initialized or OrderBookOnly
        //ref: https://github.com/raydium-io/raydium-amm/blob/127cb46d4030b0749459507610064326830efb87/program/src/state.rs#L296
        if pool.status != 1 && pool.status != 5 {
            debug!(
                "Raydium AMM V4 pool {} has no need to use an open orders account",
                pool_pubkey
            );
            //return our tuple of base and quote token totals
            (0_u64, 0_u64)
        } else {
            //get the amount of token in open orders
            let open_orders_bytes = decoded_transaction
                .get_account_data_after(pool.amm_open_orders.to_string().as_str());

            let open_orders_bytes = match open_orders_bytes {
                Ok(None) => {
                    error!(
                        "Failed to find open orders account {} for Raydium AMM V4 pool {}",
                        pool.amm_open_orders, pool_pubkey
                    );
                    return Ok(None);
                }
                Err(e) => {
                    error!(
                        "Failed to decode open orders account {} on market {} for Raydium AMM V4 pool {}: {}",
                        pool.amm_open_orders, pool.serum_market, pool_pubkey, e,
                    );
                    error!(
                        "acct {} didn't match {}",
                        pool.amm_open_orders.to_string(),
                        pool_pubkey
                    );
                    return Ok(None);
                }
                Ok(Some(a)) => a,
            };

            //in code this appears to just be the text "serum", but also in code
            //raydium sets it to 6 from what I could see.  but makes no sense like that.
            let header_byte_count = 5;

            //return our tuple of base and quote token totals
            (
                u64::from_le_bytes(
                    open_orders_bytes[header_byte_count + 80..header_byte_count + 88].try_into()?,
                ),
                u64::from_le_bytes(
                    open_orders_bytes[header_byte_count + 96..header_byte_count + 104]
                        .try_into()?,
                ),
            )
        }
    };

    debug!(
        "Raydium AMM V4 pool {} has oo base token total of {} and oo quote token total of {}",
        pool_pubkey, oo_base_token_total, oo_quote_token_total
    );

    let mut quote_token_total = pool_quote_token_total + oo_quote_token_total;
    let mut base_token_total = pool_base_token_total + oo_base_token_total;
    if quote_token_total == 0 || base_token_total == 0 {
        debug!(
            "Raydium AMM V4 pool {} has 0 base or quote token total",
            pool_pubkey
        );
        return Ok(None);
    }

    //subtract out the pnl
    base_token_total -= pool.need_take_pnl_coin;
    quote_token_total -= pool.need_take_pnl_pc;
    debug!(
        "Raydium AMM V4 pool {} has base token total of {} and quote token total of {} after pnl",
        pool_pubkey, base_token_total, quote_token_total
    );

    let lp_supply = pool.lp_reserve;

    debug!(
        "Raydium AMM V4 pool {} has LP token supply of {}",
        pool_pubkey, lp_supply
    );

    let mints = vec![
        pool.coin_mint_address.to_string(),
        pool.pc_mint_address.to_string(),
    ];
    let mints_qty_per_one_parent = vec![
        base_token_total as f64 / lp_supply as f64,
        quote_token_total as f64 / lp_supply as f64,
    ];
    let total_underlying_amounts = vec![
        Decimal::from(base_token_total),
        Decimal::from(quote_token_total),
    ];

    let dooot = MintUnderlyingsGlobalDooot {
        time: decoded_transaction.get_block_time(),
        mint_pubkey: pool.lp_mint_address.to_string(),
        platform_program_pubkey: RAYDIUM_AMM_V4_PROGRAM.to_string(),
        mints,
        mints_qty_per_one_parent,
        total_underlying_amounts,
        discriminant_id: pool_pubkey.to_string(),
    };
    trace!(
        "Raydium AMM V4 pool {} has underlying dooot: {:#?}",
        pool_pubkey,
        dooot
    );
    Ok(Some(dooot))
}

impl StateSource for RaydiumAmmV4 {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        let blocktime = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let raydium_pk = RAYDIUM_AMM_V4_PROGRAM.parse::<Pubkey>()?;
        let raydium_accts_iter = acct_provider.program_accounts(raydium_pk, None);

        let mut raydium_accounts_lookup = HashMap::new();
        let mut vault_accounts = HashSet::new();
        let mut lp_mints = HashSet::new();

        for acct in raydium_accts_iter {
            let data = acct.account.data();
            let pool_pk = acct.pubkey;

            if data.len() != 752 {
                continue;
            }

            let pool = match RaydiumAmmPoolV4::deserialize(&mut &data[..]) {
                Ok(pool) => pool,
                Err(e) => {
                    error!("Failed to decode Raydium AMM V4 pool data: {e}");
                    continue;
                }
            };

            let pool_coin_mint = pool.coin_mint_address;
            let pool_pc_mint = pool.pc_mint_address;
            let pool_lp_mint = pool.lp_mint_address;
            let pool_coin_vault = pool.pool_coin_token_account;
            let pool_pc_vault = pool.pool_pc_token_account;

            vault_accounts.insert(pool_coin_vault);
            vault_accounts.insert(pool_pc_vault);
            lp_mints.insert(pool_lp_mint);

            let lp_info_dooot = Dooot::LPInfo(LPInfoDooot {
                time: blocktime,
                pool: pool_pk.to_string(),
                program: RAYDIUM_AMM_V4_PROGRAM.to_string(),
                lp_mint: Some(pool_lp_mint.to_string()),
                underlyings: vec![
                    LPInfoUnderlyingMintVault {
                        mint: pool_coin_mint.to_string(),
                        vault: pool_coin_vault.to_string(),
                    },
                    LPInfoUnderlyingMintVault {
                        mint: pool_pc_mint.to_string(),
                        vault: pool_pc_vault.to_string(),
                    },
                ],
                curve_type: CurveType::ConstantProduct,
            });
            tx.send(lp_info_dooot)?;

            raydium_accounts_lookup.insert(pool_pk, pool);
        }

        let all_accounts_iter = acct_provider.all_accounts();
        let mut balances = HashMap::new();
        let mut supply_amts = HashMap::new();

        for acct in all_accounts_iter {
            let pk = acct.pubkey;
            let data = acct.account.data();

            if vault_accounts.contains(&pk) {
                if data.len() >= 165 {
                    let amount = u64::from_le_bytes(data[64..72].try_into()?);
                    balances.insert(pk, amount);
                } else {
                    error!("Vault account data too short: {}", data.len());
                }
            }

            if lp_mints.contains(&pk) {
                if data.len() >= 82 {
                    let supply = u64::from_le_bytes(data[36..44].try_into()?);
                    supply_amts.insert(pk, supply);
                } else {
                    error!("Mint account data too short: {}", data.len());
                }
            }
        }

        for (pool_pk, pool) in raydium_accounts_lookup.into_iter() {
            let pool_lp_mint = pool.lp_mint_address;
            let pool_coin_vault = pool.pool_coin_token_account;
            let pool_pc_vault = pool.pool_pc_token_account;

            let lp_supply = pool.lp_reserve;

            let coin_vault_balance = match balances.get(&pool_coin_vault) {
                Some(amount) => *amount,
                None => {
                    error!("Missing balance for vault: {}", pool_coin_vault);
                    continue;
                }
            };

            let pc_vault_balance = match balances.get(&pool_pc_vault) {
                Some(amount) => *amount,
                None => {
                    error!("Missing balance for vault: {}", pool_pc_vault);
                    continue;
                }
            };

            let (coin_per_parent, pc_per_parent) = if lp_supply == 0 {
                (0.0, 0.0)
            } else {
                (
                    (coin_vault_balance as f64) / (lp_supply as f64),
                    (pc_vault_balance as f64) / (lp_supply as f64),
                )
            };

            if coin_per_parent.is_nan()
                || pc_per_parent.is_nan()
                || coin_per_parent.is_infinite()
                || pc_per_parent.is_infinite()
            {
                log::warn!("Invalid per-parent amounts for pool: {}", pool_pk);
                continue;
            }

            let mint_underlying_dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                mint_pubkey: pool_lp_mint.to_string(),
                platform_program_pubkey: RAYDIUM_AMM_V4_PROGRAM.to_string(),
                mints: vec![
                    pool.coin_mint_address.to_string(),
                    pool.pc_mint_address.to_string(),
                ],
                time: blocktime,
                total_underlying_amounts: vec![coin_vault_balance.into(), pc_vault_balance.into()],
                mints_qty_per_one_parent: vec![coin_per_parent, pc_per_parent],
                discriminant_id: pool_pk.to_string(),
            });
            tx.send(mint_underlying_dooot)?;
        }

        Ok(())
    }

    fn get_name(&self) -> &'static str {
        "RaydiumAMMV4"
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, rc::Rc};

    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;

    use crate::TransactionParserTrait;

    use super::*;

    #[tokio::test]
    async fn raydium_swaps() {
        let txn_json = fs::read("./tests/raydium/swap.json").unwrap();
        let txn = serde_json::from_slice::<DecodedTransaction>(&txn_json).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));

        let mut dooot = vec![];
        let parser = RaydiumAmmV4;

        parser.invoke(&txn, &mut dooot, &step_utils).await.unwrap();

        assert_eq!(dooot.len(), 3);
    }
}
