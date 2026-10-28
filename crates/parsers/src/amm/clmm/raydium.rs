use super::utils::mul_rsh;
use crate::state_source::{AccountProvider, StateSource};
use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use borsh::BorshDeserialize;
use primitive_types::U256;
use rust_decimal::Decimal;
use solana_sdk::account::ReadableAccount;
use std::result::Result::Ok;
use step_ingestooor_idl_gens::raydium_clmm_gen::raydium_clmm::accounts::TickArrayState;

use std::collections::HashMap;
use std::sync::mpsc::SyncSender;

use step_ingestooor_idl_gens::raydium_clmm_gen::raydium_clmm::{
    accounts::{PersonalPositionState, PoolState},
    utils::InstructionUnion,
};
use step_ingestooor_sdk::dooot::{
    ClmmGlobalDooot, ClmmTickGlobalDooot, ClmmUserDooot, CurveType, Dooot, LPInfoDooot,
    LPInfoUnderlyingMintVault, LiquidityChangeEventDooot, SwapEventDooot,
};
use step_ingestooor_sdk::trait_defs::transaction::TransactionLike;
use step_ingestooor_sdk::utils::step_utils::StepUtils;
use step_ingestooor_sdk::utils::util_fns::{
    convert_blocktime_to_micros, get_partially_decoded_variant, get_program_id_of_ix,
};

pub struct RaydiumCLMM;

pub const RAYDIUM_CLMM_PROGRAM_ID: &str = "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK";
pub const RAYDIUM_CLMM_PERSONAL_POSITION_DISC: &[u8] =
    &[0x46, 0x6f, 0x96, 0x7e, 0xe6, 0x0f, 0x19, 0x75];
pub const RAYDIUM_CLMM_POOL_STATE_DISC: &[u8] = &[0xf7, 0xed, 0xe3, 0xf5, 0xd7, 0xc3, 0xde, 0x46];
pub const RAYDIUM_CLMM_TICK_ARRAY_DISC: &[u8] = &[0xc0, 0x9b, 0x55, 0xcd, 0x31, 0xf9, 0x81, 0x2a];

impl RaydiumCLMM {
    /// See https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/libraries/liquidity_math.rs
    /// and https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/libraries/tick_math.rs
    #[inline]
    pub fn get_token_amounts_from_position(
        pool: &PoolState,
        position: &PersonalPositionState,
    ) -> (u64, u64) {
        let lower_sqrt_price_x64 = Self::get_sqrt_price_x64_from_tick(position.tick_lower_index);
        let upper_sqrt_price_x64 = Self::get_sqrt_price_x64_from_tick(position.tick_upper_index);
        let current_sqrt_price_x64 = pool.sqrt_price_x64;
        let total_liquidity = position.liquidity;

        Self::get_token_amounts_from_liquidity(
            current_sqrt_price_x64,
            lower_sqrt_price_x64,
            upper_sqrt_price_x64,
            total_liquidity,
        )
    }

    #[inline]
    fn get_token_amounts_from_liquidity(
        current_sqrt_price_x64: u128,
        mut sqrt_price_x64_a: u128,
        mut sqrt_price_x64_b: u128,
        liquidity: u128,
    ) -> (u64, u64) {
        if sqrt_price_x64_a > sqrt_price_x64_b {
            std::mem::swap(&mut sqrt_price_x64_a, &mut sqrt_price_x64_b)
        }

        let (amount_a, amount_b) = if current_sqrt_price_x64 < sqrt_price_x64_a {
            let token_a = Self::get_token_amount_a_from_liquidity(
                sqrt_price_x64_a,
                sqrt_price_x64_b,
                liquidity,
            );

            (token_a, 0)
        } else if current_sqrt_price_x64 < sqrt_price_x64_b {
            let token_a = Self::get_token_amount_a_from_liquidity(
                current_sqrt_price_x64,
                sqrt_price_x64_b,
                liquidity,
            );
            let token_b = Self::get_token_amount_b_from_liquidity(
                sqrt_price_x64_a,
                current_sqrt_price_x64,
                liquidity,
            );

            (token_a, token_b)
        } else {
            let token_b = Self::get_token_amount_b_from_liquidity(
                sqrt_price_x64_a,
                sqrt_price_x64_b,
                liquidity,
            );

            (0, token_b)
        };

        (amount_a, amount_b)
    }

    #[inline]
    fn get_token_amount_a_from_liquidity(
        mut sqrt_price_a: u128,
        mut sqrt_price_b: u128,
        liquidity: u128,
    ) -> u64 {
        if sqrt_price_a > sqrt_price_b {
            std::mem::swap(&mut sqrt_price_a, &mut sqrt_price_b)
        }

        let shifted_liq = U256::from(liquidity) << 64;
        let diff = U256::from(sqrt_price_b - sqrt_price_a);

        (((shifted_liq * diff) / sqrt_price_b) / sqrt_price_a).as_u64()
    }

    #[inline]
    fn get_token_amount_b_from_liquidity(
        mut sqrt_price_a: u128,
        mut sqrt_price_b: u128,
        liquidity: u128,
    ) -> u64 {
        if sqrt_price_a > sqrt_price_b {
            std::mem::swap(&mut sqrt_price_a, &mut sqrt_price_b)
        }

        // Shift by 64 to do a faster divide by 2^64
        ((liquidity * (sqrt_price_b - sqrt_price_a)) >> 64) as u64
    }

    #[inline]
    pub fn get_sqrt_price_x64_from_tick(tick: i32) -> u128 {
        let tick_abs = tick.abs();

        let mut ratio: U256 = if tick_abs & 0x1 != 0 {
            U256::from(18445821805675395072u128)
        } else {
            U256::from(u64::MAX) + 1
        };

        if tick_abs & 0x2 != 0 {
            ratio = mul_rsh(ratio, 18444899583751176192u128, 64);
        }
        if tick_abs & 0x4 != 0 {
            ratio = mul_rsh(ratio, 18443055278223355904u128, 64);
        }
        if tick_abs & 0x8 != 0 {
            ratio = mul_rsh(ratio, 18439367220385607680u128, 64);
        }
        if tick_abs & 0x10 != 0 {
            ratio = mul_rsh(ratio, 18431993317065453568u128, 64);
        }
        if tick_abs & 0x20 != 0 {
            ratio = mul_rsh(ratio, 18417254355718170624u128, 64);
        }
        if tick_abs & 0x40 != 0 {
            ratio = mul_rsh(ratio, 18387811781193609216u128, 64);
        }
        if tick_abs & 0x80 != 0 {
            ratio = mul_rsh(ratio, 18329067761203558400u128, 64);
        }
        if tick_abs & 0x100 != 0 {
            ratio = mul_rsh(ratio, 18212142134806163456u128, 64);
        }
        if tick_abs & 0x200 != 0 {
            ratio = mul_rsh(ratio, 17980523815641700352u128, 64);
        }
        if tick_abs & 0x400 != 0 {
            ratio = mul_rsh(ratio, 17526086738831433728u128, 64);
        }
        if tick_abs & 0x800 != 0 {
            ratio = mul_rsh(ratio, 16651378430235570176u128, 64);
        }
        if tick_abs & 0x1000 != 0 {
            ratio = mul_rsh(ratio, 15030750278694412288u128, 64);
        }
        if tick_abs & 0x2000 != 0 {
            ratio = mul_rsh(ratio, 12247334978884435968u128, 64);
        }
        if tick_abs & 0x4000 != 0 {
            ratio = mul_rsh(ratio, 8131365268886854656u128, 64);
        }
        if tick_abs & 0x8000 != 0 {
            ratio = mul_rsh(ratio, 3584323654725218816u128, 64);
        }
        if tick_abs & 0x10000 != 0 {
            ratio = mul_rsh(ratio, 696457651848324352u128, 64);
        }
        if tick_abs & 0x20000 != 0 {
            ratio = mul_rsh(ratio, 26294789957507116u128, 64);
        }
        if tick_abs & 0x40000 != 0 {
            ratio = mul_rsh(ratio, 37481735321082u128, 64);
        }

        if tick > 0 {
            ratio = U256::from(u128::MAX) / ratio;
        }

        ratio.as_u128()
    }
}

#[async_trait]
impl TransactionParserTrait for RaydiumCLMM {
    fn get_name(&self) -> &'static str {
        "RaydiumCLMM"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            RAYDIUM_CLMM_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> anyhow::Result<()> {
        let time = txn.get_block_time();

        for acct in txn.iter_accounts().filter(|a| {
            a.datum_changed
                && (a.post_owner_program == Some(RAYDIUM_CLMM_PROGRAM_ID)
                    || a.pre_owner_program == Some(RAYDIUM_CLMM_PROGRAM_ID))
        }) {
            let pre_data = acct.unpack_pre_datum()?.map(|d| (d.is_empty(), d));
            let post_data = acct.unpack_post_datum()?.map(|d| (d.is_empty(), d));

            let (closed, data) = match (pre_data, post_data) {
                (_, Some((false, post))) => (false, post),
                (Some((false, pre)), None | Some((true, _))) => (true, pre),
                _ => continue,
            };

            if data.starts_with(RAYDIUM_CLMM_PERSONAL_POSITION_DISC) {
                let personal_position_state = PersonalPositionState::try_from_slice(&data[8..])
                    .context("Failed to parse personal position state")?;
                let upper_price =
                    Self::get_sqrt_price_x64_from_tick(personal_position_state.tick_upper_index);
                let lower_price =
                    Self::get_sqrt_price_x64_from_tick(personal_position_state.tick_lower_index);

                let position_mint = personal_position_state.nft_mint.to_string();

                if let Some(nft_owner) = txn.get_position_nft_owner(&position_mint) {
                    let user_dooot = Dooot::ClmmUser(ClmmUserDooot {
                        time,
                        pool_pubkey: personal_position_state.pool_id.to_string(),
                        user_pubkey: nft_owner.clone(),
                        position_mint: Some(position_mint),
                        upper_price: upper_price.to_string(),
                        lower_price: lower_price.to_string(),
                        liquidity_shares: if closed {
                            "0".to_string()
                        } else {
                            personal_position_state.liquidity.to_string()
                        },
                        closed,
                    });
                    dooot.push(user_dooot);
                } else {
                    return Ok(());
                }
            } else if data.starts_with(RAYDIUM_CLMM_POOL_STATE_DISC) {
                let pool_state =
                    PoolState::try_from_slice(&data[8..]).context("Failed to parse pool state")?;

                if acct.was_created {
                    let lp_info = Dooot::LPInfo(LPInfoDooot {
                        time,
                        pool: acct.pubkey.to_string(),
                        program: RAYDIUM_CLMM_PROGRAM_ID.to_string(),
                        lp_mint: None,
                        underlyings: vec![
                            LPInfoUnderlyingMintVault {
                                mint: pool_state.token_mint0.to_string(),
                                vault: pool_state.token_vault0.to_string(),
                            },
                            LPInfoUnderlyingMintVault {
                                mint: pool_state.token_mint1.to_string(),
                                vault: pool_state.token_vault1.to_string(),
                            },
                        ],
                        curve_type: CurveType::ConcentratedLiquidity,
                    });
                    dooot.push(lp_info);
                }

                let global_dooot = Dooot::ClmmGlobal(ClmmGlobalDooot {
                    time,
                    pool_pubkey: acct.pubkey.to_string(),
                    total_liquidity_shares: Some(pool_state.liquidity.to_string()),
                    current_price: pool_state.sqrt_price_x64.to_string(),
                    current_tick_index: pool_state.tick_current,
                    tick_spacing: pool_state.tick_spacing as u32,
                });

                dooot.push(global_dooot);

                let vault_a_pre =
                    match txn.get_pre_token_account_balance(&pool_state.token_vault0.to_string()) {
                        Ok(Some(pre)) => pre,
                        _ => continue,
                    };
                let vault_a_post = match txn
                    .get_post_token_account_balance(&pool_state.token_vault0.to_string())
                {
                    Ok(Some(post)) => post,
                    _ => continue,
                };
                let vault_b_pre =
                    match txn.get_pre_token_account_balance(&pool_state.token_vault1.to_string()) {
                        Ok(Some(pre)) => pre,
                        _ => continue,
                    };
                let vault_b_post = match txn
                    .get_post_token_account_balance(&pool_state.token_vault1.to_string())
                {
                    Ok(Some(post)) => post,
                    _ => continue,
                };

                let change_a = i128::from(vault_a_post) - i128::from(vault_a_pre);
                let change_b = i128::from(vault_b_post) - i128::from(vault_b_pre);

                // skip liquidity event if one vault increases and the other decreases (swap pattern)
                if change_a * change_b < 0 {
                    continue;
                }

                if let Some(nft_owner) = txn.get_first_signer() {
                    let liq_change = Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                        time,
                        owner: nft_owner,
                        pool: acct.pubkey.to_string(),
                        mints: vec![
                            pool_state.token_mint0.to_string(),
                            pool_state.token_mint1.to_string(),
                        ],
                        amounts: vec![Decimal::from(change_a), Decimal::from(change_b)],
                    });
                    dooot.push(liq_change);
                } else {
                    return Ok(());
                }
            } else if data.starts_with(RAYDIUM_CLMM_TICK_ARRAY_DISC) {
                let tick_array = TickArrayState::deserialize(&mut &data[8..])
                    .context("Failed to parse tick array")?;
                let tick_dooot = Dooot::ClmmTickGlobal(ClmmTickGlobalDooot {
                    time,
                    pool_pubkey: tick_array.pool_id.to_string(),
                    tick_account_pubkey: acct.pubkey.to_string(),
                    tick_index: tick_array.start_tick_index,
                    ticks: tick_array.ticks.into_iter().map(|t| t.into()).collect(),
                });

                dooot.push(tick_dooot);
            }
        }

        let ix_tree = txn.get_ix_tree()?;
        let root = ix_tree.root().context("No root in ix tree")?;
        for node in root.traverse_level_order().skip(1) {
            let part_dec = match node.data().and_then(get_partially_decoded_variant) {
                Some(pd) => pd,
                None => continue,
            };
            if part_dec.program_id != RAYDIUM_CLMM_PROGRAM_ID {
                continue;
            }
            let bytes = bs58::decode(&part_dec.data).into_vec()?;
            let decoded_ix = match InstructionUnion::try_from_slice(&bytes) {
                Result::Ok(u) => u,
                Err(_) => {
                    log::warn!(
                        "Unknown ix variant for Raydium vaults. Bytes: {:?} Sig: {}",
                        &bytes[..8],
                        txn.get_signature(),
                    );
                    continue;
                }
            };
            match decoded_ix {
                InstructionUnion::Swap(_) => {
                    let pool = &part_dec.accounts[2];
                    let in_mint_acc = &part_dec.accounts[3];
                    let token_in = txn.get_token_account_data_by_key(in_mint_acc)?;
                    let in_mint = token_in.mint;
                    let out_mint_acc = &part_dec.accounts[4];
                    let token_out = txn.get_token_account_data_by_key(out_mint_acc)?;
                    let out_mint = token_out.mint;
                    let auth = &part_dec.accounts[0];
                    let ixs_scan = node.children().filter_map(|kid| *kid.data());
                    let txfer_amounts = txn.get_total_transfer_amounts_for_pubkey_ui_ixs(
                        ixs_scan,
                        &[auth],
                        &[&in_mint.to_string()],
                        &[&out_mint.to_string()],
                    )?;
                    let in_amount =
                        Decimal::from(txfer_amounts.get_in_amount_u64(&in_mint.to_string(), auth));
                    let out_amount = Decimal::from(
                        txfer_amounts.get_out_amount_u64(&out_mint.to_string(), auth),
                    );
                    let parent_amm = node
                        .parent()
                        .and_then(|p| *p.data())
                        .and_then(get_program_id_of_ix);
                    let swap_event = Dooot::SwapEvent(SwapEventDooot {
                        time,
                        program_pubkey: RAYDIUM_CLMM_PROGRAM_ID.to_string(),
                        swap_auth_pubkey: auth.to_string(),
                        in_mint_pubkey: in_mint.to_string(),
                        out_mint_pubkey: out_mint.to_string(),
                        in_amount,
                        out_amount,
                        fee_mint_is_in_mint: None,
                        referring_platform_account_pubkey: None,
                        platform_referral_fee: None,
                        parent_amm_program_pubkey: parent_amm,
                        market_pubkey: Some(pool.to_string()),
                        fees: vec![],
                        slippage_bps: None,
                        tx_sig: txn.get_signature().to_string(),
                    });
                    dooot.push(swap_event);
                }
                InstructionUnion::SwapV2(_) => {
                    let pool = &part_dec.accounts[2];
                    let in_mint = &part_dec.accounts[11];
                    let out_mint = &part_dec.accounts[12];
                    let out_token_acc = &part_dec.accounts[4];
                    let token_out = txn.get_token_account_data_by_key(out_token_acc)?;
                    let auth_owner = token_out
                        .owner
                        .context("Failed to get owner of out token account")?;
                    let auth = &part_dec.accounts[0];
                    let ixs_scan = node.children().filter_map(|kid| *kid.data());
                    let txfer_amounts = txn.get_total_transfer_amounts_for_pubkey_ui_ixs(
                        ixs_scan,
                        &[&auth_owner, auth],
                        &[&in_mint.to_string()],
                        &[&out_mint.to_string()],
                    )?;
                    let in_amount =
                        Decimal::from(txfer_amounts.get_total_in_amount_u64(&in_mint.to_string()));
                    let out_amount = Decimal::from(
                        txfer_amounts.get_total_out_amount_u64(&out_mint.to_string()),
                    );
                    let parent_amm = node
                        .parent()
                        .and_then(|p| *p.data())
                        .and_then(get_program_id_of_ix);
                    let swap_event = Dooot::SwapEvent(SwapEventDooot {
                        time,
                        program_pubkey: RAYDIUM_CLMM_PROGRAM_ID.to_string(),
                        swap_auth_pubkey: auth.to_string(),
                        in_mint_pubkey: in_mint.to_string(),
                        out_mint_pubkey: out_mint.to_string(),
                        in_amount,
                        out_amount,
                        fee_mint_is_in_mint: None,
                        referring_platform_account_pubkey: None,
                        platform_referral_fee: None,
                        parent_amm_program_pubkey: parent_amm,
                        market_pubkey: Some(pool.to_string()),
                        fees: vec![],
                        slippage_bps: None,
                        tx_sig: txn.get_signature().to_string(),
                    });
                    dooot.push(swap_event);
                }
                _ => continue,
            }
        }

        Ok(())
    }
}

impl StateSource for RaydiumCLMM {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        let mut pools: HashMap<String, PoolState> = HashMap::new();
        let blocktime = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let all_acs = acct_provider.program_accounts(RAYDIUM_CLMM_PROGRAM_ID.try_into()?, None);
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)
            .map_err(|e| anyhow!("Failed to convert blocktime: {}", e))?;
        let mut deferred_positions: Vec<(String, Vec<u8>)> = Vec::new();

        for acct in all_acs {
            let data = acct.account.data();
            if data.starts_with(RAYDIUM_CLMM_POOL_STATE_DISC) {
                let pool = PoolState::try_from_slice(&data[8..])?;

                let lp_info_dooot = Dooot::LPInfo(LPInfoDooot {
                    time,
                    pool: acct.pubkey.to_string(),
                    program: RAYDIUM_CLMM_PROGRAM_ID.to_string(),
                    lp_mint: None,
                    underlyings: vec![
                        LPInfoUnderlyingMintVault {
                            mint: pool.token_mint0.to_string(),
                            vault: pool.token_vault0.to_string(),
                        },
                        LPInfoUnderlyingMintVault {
                            mint: pool.token_mint1.to_string(),
                            vault: pool.token_vault1.to_string(),
                        },
                    ],
                    curve_type: CurveType::ConcentratedLiquidity,
                });
                tx.send(lp_info_dooot)?;

                let global_dooot = Dooot::ClmmGlobal(ClmmGlobalDooot {
                    time: blocktime,
                    pool_pubkey: acct.pubkey.to_string(),
                    total_liquidity_shares: Some(pool.liquidity.to_string()),
                    current_price: pool.sqrt_price_x64.to_string(),
                    current_tick_index: pool.tick_current,
                    tick_spacing: pool.tick_spacing as u32,
                });
                tx.send(global_dooot)?;
                pools.insert(acct.pubkey.to_string(), pool);
            } else if data.starts_with(RAYDIUM_CLMM_PERSONAL_POSITION_DISC) {
                let personal_position = PersonalPositionState::try_from_slice(&data[8..])?;
                let pool_id = personal_position.pool_id.to_string();
                let upper_price =
                    Self::get_sqrt_price_x64_from_tick(personal_position.tick_upper_index);
                let lower_price =
                    Self::get_sqrt_price_x64_from_tick(personal_position.tick_lower_index);

                if let Some(pool) = pools.get(&pool_id) {
                    let user_dooot = Dooot::ClmmUser(ClmmUserDooot {
                        time: blocktime,
                        pool_pubkey: pool_id.to_string(),
                        user_pubkey: pool.owner.to_string(),
                        position_mint: Some(personal_position.nft_mint.to_string()),
                        upper_price: upper_price.to_string(),
                        lower_price: lower_price.to_string(),
                        liquidity_shares: personal_position.liquidity.to_string(),
                        closed: false,
                    });
                    tx.send(user_dooot)?;
                } else {
                    // for second pass
                    deferred_positions.push((pool_id.to_string(), data[8..].to_vec()));
                }
            } else if data.starts_with(RAYDIUM_CLMM_TICK_ARRAY_DISC) {
                let tick_array = TickArrayState::deserialize(&mut &data[8..])
                    .context("Failed to parse tick array")?;
                let tick_dooot = Dooot::ClmmTickGlobal(ClmmTickGlobalDooot {
                    time,
                    pool_pubkey: tick_array.pool_id.to_string(),
                    tick_account_pubkey: acct.pubkey.to_string(),
                    tick_index: tick_array.start_tick_index,
                    ticks: tick_array.ticks.into_iter().map(|t| t.into()).collect(),
                });

                tx.send(tick_dooot)?;
            }
        }

        // deferred positions (if any)
        for (pool_id, raw_data) in deferred_positions {
            if let Some(pool) = pools.get(&pool_id) {
                let personal_position = PersonalPositionState::try_from_slice(&raw_data)?;
                let upper_price =
                    Self::get_sqrt_price_x64_from_tick(personal_position.tick_upper_index);
                let lower_price =
                    Self::get_sqrt_price_x64_from_tick(personal_position.tick_lower_index);
                let user_dooot = Dooot::ClmmUser(ClmmUserDooot {
                    time: blocktime,
                    pool_pubkey: pool_id.to_string(),
                    user_pubkey: pool.owner.to_string(),
                    position_mint: Some(personal_position.nft_mint.to_string()),
                    upper_price: upper_price.to_string(),
                    lower_price: lower_price.to_string(),
                    liquidity_shares: personal_position.liquidity.to_string(),
                    closed: false,
                });
                tx.send(user_dooot)?;
            }
        }

        Ok(())
    }
    fn get_name(&self) -> &'static str {
        "RaydiumCLMM"
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, rc::Rc};

    use super::*;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;

    #[tokio::test]
    async fn failing() {
        let path = "tests/raydium/failing.json";
        let message_json = fs::read_to_string(path).unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser: RaydiumCLMM = RaydiumCLMM;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn invoke_increase_liq() {
        let txn_json = fs::read_to_string("tests/raydium/txn_increase_liquidity_3z7WC7PfXcCLaQP3eLMSLZjd5WmpkGcWVnrK6UDXr7AGgkH36zmaxV5Dp97UFwY4zV8rzAwv8TDZP9pMSiHZFGuv.json").unwrap();
        let decoded_txn = serde_json::from_str::<DecodedTransaction>(&txn_json).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));
        let mut schema = vec![];
        let parser = RaydiumCLMM;
        parser
            .invoke(&decoded_txn, &mut schema, &step_utils)
            .await
            .unwrap();
        assert_eq!(schema.len(), 5);
        let Dooot::ClmmGlobal(clmm_global) = &schema[0] else {
            panic!("Expected a clmm global dooot");
        };
        let Dooot::LiquidityChangeEvent(liq_change) = &schema[1] else {
            panic!("Expected a liquidity change dooot");
        };
        let Dooot::ClmmUser(clmm_user) = &schema[2] else {
            panic!("Expected a clmm user dooot");
        };

        let pool_pk = "3BVNZ9vzUZ9ohoXbvwZXbmkv76vvbZSBSQjNK38YAFJp";
        assert_eq!(clmm_global.pool_pubkey, pool_pk);
        let current_price = "972235858850775968928";
        assert_eq!(clmm_global.current_price, current_price);

        // ---
        let expected_owner = "9cgv1eY5Yj9j8zCYqeot2p6K9WQC7Z3zWt6ogEnfjJno";
        let expected_pool = "3BVNZ9vzUZ9ohoXbvwZXbmkv76vvbZSBSQjNK38YAFJp";
        let expected_mints = vec![
            "So11111111111111111111111111111111111111112",
            "5LafQUrVco6o7KMz42eqVEJ9LW31StPyGjeeu5sKoMtA",
        ];
        let expected_amounts = vec![
            Decimal::from(1701812970u64),
            Decimal::from(3535124014552u64),
        ];
        let actual_owner = &liq_change.owner;
        let actual_pool = &liq_change.pool;
        let actual_mints = &liq_change.mints;
        let actual_amounts = &liq_change.amounts;
        assert_eq!(actual_owner, expected_owner);
        assert_eq!(actual_pool, expected_pool);
        assert_eq!(*actual_mints, *expected_mints);
        assert_eq!(*actual_amounts, *expected_amounts);
        // ---
        let pool_pubkey = "3BVNZ9vzUZ9ohoXbvwZXbmkv76vvbZSBSQjNK38YAFJp";
        let user_pk = "9cgv1eY5Yj9j8zCYqeot2p6K9WQC7Z3zWt6ogEnfjJno";
        let upper_price = "1027297550014845109028";
        let lower_price = "933267337357903737305";
        let liquidity_shares = "35932718098942";
        assert_eq!(clmm_user.pool_pubkey, pool_pubkey);
        assert_eq!(clmm_user.user_pubkey, user_pk);
        assert_eq!(clmm_user.upper_price, upper_price);
        assert_eq!(clmm_user.lower_price, lower_price);
        assert_eq!(clmm_user.liquidity_shares, liquidity_shares);
    }

    #[tokio::test]
    async fn invoke_decrease_liq() {
        let txn_json = fs::read_to_string("tests/raydium/txn_decrease_liquidity_3oZutjeSzMMUZpJ7YsPC8Dyfjr4JjEcuHABDpZgijbwAHZ7SboEDzDKphv1wwmWfRt4atJDLqmdrg1TTFKodcQ8y.json").unwrap();
        let decoded_txn = serde_json::from_str::<DecodedTransaction>(&txn_json).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));
        let mut schema = vec![];
        let parser = RaydiumCLMM;
        parser
            .invoke(&decoded_txn, &mut schema, &step_utils)
            .await
            .unwrap();
        assert_eq!(schema.len(), 3);
        let Dooot::ClmmGlobal(clmm_global) = &schema[0] else {
            panic!("Expected a liquidity change dooot");
        };
        let Dooot::LiquidityChangeEvent(liq_change) = &schema[1] else {
            panic!("Expected a liquidity change dooot");
        };

        let pool_pubkey = "E5X7mWprg8pdAqBT5HJ1ehu4crAsgUkUX5MwCjuTusZU";
        let current_price = "12982768620505383944";
        assert_eq!(clmm_global.pool_pubkey, pool_pubkey);
        assert_eq!(clmm_global.current_price, current_price);

        let expected_owner = "DDsVVof9vHURHutsgJEUkFhTFuJscpt81b5DxKEUt6YS";
        let expected_pool = "E5X7mWprg8pdAqBT5HJ1ehu4crAsgUkUX5MwCjuTusZU";
        let expected_mints = vec![
            "So11111111111111111111111111111111111111112",
            "ZEUS1aR7aX8DFFJf5QjWj2ftDDdNTroMNGo8YoQm3Gq",
        ];
        let expected_amounts = vec![Decimal::from(-3699310752i64), Decimal::from(-4521266i64)];
        let actual_owner = &liq_change.owner;
        let actual_pool = &liq_change.pool;
        let actual_mints = &liq_change.mints;
        let actual_amounts = &liq_change.amounts;
        assert_eq!(actual_owner, expected_owner);
        assert_eq!(actual_pool, expected_pool);
        assert_eq!(*actual_mints, *expected_mints);
        assert_eq!(*actual_amounts, *expected_amounts);
    }

    #[tokio::test]
    async fn invoke_remove_liquidity() {
        let txn_json = fs::read_to_string("tests/raydium/txn_remove_liquidity.json").unwrap();
        let decoded_txn = serde_json::from_str::<DecodedTransaction>(&txn_json).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooots = vec![];
        let parser = RaydiumCLMM;
        parser
            .invoke(&decoded_txn, &mut dooots, &step_utils)
            .await
            .unwrap();
        // expected ClmmUser dooot with liquidity shares 0
        let Dooot::ClmmUser(clmm_user) = &dooots[0] else {
            panic!("Expected a clmm user dooot");
        };
        assert_eq!(clmm_user.liquidity_shares, "0");
    }

    #[tokio::test]
    async fn invoke_swap() {
        let txn_json = fs::read_to_string("tests/raydium/txn_raydium_swap_nMno3Sw1M9NPvcwCPc7aawnQr1A9DY9heHB1fotbTEWRgToePawcR3NLtdWGJyoCABraBW4dXU6KuJquDxzSZb3.json").unwrap();
        let decoded_txn = serde_json::from_str::<DecodedTransaction>(&txn_json).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = RaydiumCLMM;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 2);

        let Dooot::ClmmGlobal(clmm_global) = &dooot[0] else {
            panic!("Expected a swap dooot");
        };
        let expected_pool_pk = "2QdhepnKRTLjjSqPL1PtKNwqrUkoLee5Gqs8bvZhRdMv";
        let expected_current_price = "7049644224379748764";
        assert_eq!(clmm_global.pool_pubkey, expected_pool_pk);
        assert_eq!(clmm_global.current_price, expected_current_price);

        let Dooot::SwapEvent(swap_dooot) = &dooot[1] else {
            panic!("Expected a swap dooot");
        };
        let expected_swap_auth_pubkey = "j1oAbxxiDUWvoHxEDhWE7THLjEkDQW2cSHYn2vttxTF";
        let expected_in_mint_pubkey = "So11111111111111111111111111111111111111112";
        let expected_out_mint_pubkey = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
        let expected_in_amount = Decimal::from(84183497u64);
        let expected_out_amount = Decimal::from(12288681u64);
        let expected_parent_amm_program_pubkey =
            Some("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".to_string());
        let expected_market_pubkey =
            Some("2QdhepnKRTLjjSqPL1PtKNwqrUkoLee5Gqs8bvZhRdMv".to_string());
        let actual_swap_auth_pubkey = &swap_dooot.swap_auth_pubkey;
        let actual_in_mint_pubkey = &swap_dooot.in_mint_pubkey;
        let actual_out_mint_pubkey = &swap_dooot.out_mint_pubkey;
        let actual_in_amount = &swap_dooot.in_amount;
        let actual_out_amount = &swap_dooot.out_amount;
        let actual_parent_amm_program_pubkey = &swap_dooot.parent_amm_program_pubkey;
        let actual_market_pubkey = &swap_dooot.market_pubkey;
        assert_eq!(actual_swap_auth_pubkey, expected_swap_auth_pubkey);
        assert_eq!(actual_in_mint_pubkey, expected_in_mint_pubkey);
        assert_eq!(actual_out_mint_pubkey, expected_out_mint_pubkey);
        assert_eq!(*actual_in_amount, expected_in_amount);
        assert_eq!(*actual_out_amount, expected_out_amount);
        assert_eq!(
            *actual_parent_amm_program_pubkey,
            expected_parent_amm_program_pubkey
        );
        assert_eq!(*actual_market_pubkey, expected_market_pubkey);
    }

    #[tokio::test]
    async fn invoke_swap_v2() {
        let txn_json = fs::read_to_string("tests/raydium/txn_raydium_swap_v2_4nycrrYq3T5QyzBmdBCaR8DhSGTCsqATiRKjdA7x5wBrNV9RZuwbiNnckQTvDp1LHPA9VZRJ9UbtwsrwDG92mTS3.json").unwrap();
        let decoded_txn = serde_json::from_str::<DecodedTransaction>(&txn_json).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = RaydiumCLMM;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooot.len(), 3);

        let Dooot::ClmmGlobal(clmm_global) = &dooot[1] else {
            panic!("Expected a swap dooot");
        };
        let expected_pool_pk = "8sLbNZoA1cfnvMJLPfp98ZLAnFSYCFApfJKMbiXNLwxj";
        let expected_current_price = "7048474916588695342";
        assert_eq!(clmm_global.pool_pubkey, expected_pool_pk);
        assert_eq!(clmm_global.current_price, expected_current_price);

        let Dooot::SwapEvent(swap_dooot) = &dooot[2] else {
            panic!("Expected a swap dooot");
        };
        let expected_swap_auth_pubkey = "MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa";
        let expected_in_mint_pubkey = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
        let expected_out_mint_pubkey = "So11111111111111111111111111111111111111112";
        let expected_in_amount = Decimal::from(744750901u64);
        let expected_out_amount = Decimal::from(5100957214u64);
        let expected_parent_amm_program_pubkey = None;
        let expected_market_pubkey =
            Some("8sLbNZoA1cfnvMJLPfp98ZLAnFSYCFApfJKMbiXNLwxj".to_string());
        let actual_swap_auth_pubkey = &swap_dooot.swap_auth_pubkey;
        let actual_in_mint_pubkey = &swap_dooot.in_mint_pubkey;
        let actual_out_mint_pubkey = &swap_dooot.out_mint_pubkey;
        let actual_in_amount = &swap_dooot.in_amount;
        let actual_out_amount = &swap_dooot.out_amount;
        let actual_parent_amm_program_pubkey = &swap_dooot.parent_amm_program_pubkey;
        let actual_market_pubkey = &swap_dooot.market_pubkey;
        assert_eq!(actual_swap_auth_pubkey, expected_swap_auth_pubkey);
        assert_eq!(actual_in_mint_pubkey, expected_in_mint_pubkey);
        assert_eq!(actual_out_mint_pubkey, expected_out_mint_pubkey);
        assert_eq!(*actual_in_amount, expected_in_amount);
        assert_eq!(*actual_out_amount, expected_out_amount);
        assert_eq!(
            *actual_parent_amm_program_pubkey,
            expected_parent_amm_program_pubkey
        );
        assert_eq!(*actual_market_pubkey, expected_market_pubkey);
    }

    #[tokio::test]
    async fn invoke_swap_v2_zero_amount() {
        let txn_json = fs::read_to_string("tests/raydium/txn_raydium_swap_v2_3btmrMCFSbzkxNe525ZBeFjwLKPNqZFti2ZUYCsRVKZwLJisPKKhEVkyChVLAMTcaNc7nqcQq7a4eW262ZuLeyUD.json").unwrap();
        let decoded_txn = serde_json::from_str::<DecodedTransaction>(&txn_json).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = RaydiumCLMM;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooot.len(), 2);
        let Dooot::ClmmGlobal(clmm_global) = &dooot[0] else {
            panic!("Expected a swap dooot");
        };
        let expected_pool_pk = "H6fxtvWLFYSJ66mPJqoz7cg6tk32Pcgc9vXrywu4LEWk";
        let expected_current_price = "18445922190218261570";
        assert_eq!(clmm_global.pool_pubkey, expected_pool_pk);
        assert_eq!(clmm_global.current_price, expected_current_price);

        let Dooot::SwapEvent(swap_dooot) = &dooot[1] else {
            panic!("Expected a swap dooot");
        };
        let expected_swap_auth_pubkey = "J99nkSn8AMnVgeHYR3N8s1tcb9s2g2mw8cNimvq3mu7q";
        let expected_in_mint_pubkey = "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo";
        let expected_out_mint_pubkey = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
        let expected_in_amount = Decimal::from(400000000u64);
        let expected_out_amount = Decimal::from(399924384u64);
        let expected_parent_amm_program_pubkey =
            Some("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".to_string());
        let expected_market_pubkey =
            Some("H6fxtvWLFYSJ66mPJqoz7cg6tk32Pcgc9vXrywu4LEWk".to_string());
        let actual_swap_auth_pubkey = &swap_dooot.swap_auth_pubkey;
        let actual_in_mint_pubkey = &swap_dooot.in_mint_pubkey;
        let actual_out_mint_pubkey = &swap_dooot.out_mint_pubkey;
        let actual_in_amount = &swap_dooot.in_amount;
        let actual_out_amount = &swap_dooot.out_amount;
        let actual_parent_amm_program_pubkey = &swap_dooot.parent_amm_program_pubkey;
        let actual_market_pubkey = &swap_dooot.market_pubkey;
        assert_eq!(actual_swap_auth_pubkey, expected_swap_auth_pubkey);
        assert_eq!(actual_in_mint_pubkey, expected_in_mint_pubkey);
        assert_eq!(actual_out_mint_pubkey, expected_out_mint_pubkey);
        assert_eq!(*actual_in_amount, expected_in_amount);
        assert_eq!(*actual_out_amount, expected_out_amount);
        assert_eq!(
            *actual_parent_amm_program_pubkey,
            expected_parent_amm_program_pubkey
        );
        assert_eq!(*actual_market_pubkey, expected_market_pubkey);
    }
}
