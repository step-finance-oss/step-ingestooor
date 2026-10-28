use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use borsh::BorshDeserialize;
use solana_sdk::account::ReadableAccount;
use std::{collections::HashMap, str::FromStr, vec};
use step_ingestooor_sdk::utils::step_account_reader::StepAccountReader;
use step_ingestooor_sdk::{
    trait_defs::pod_types::RefFromByteRef, utils::util_fns::convert_blocktime_to_micros,
};

use crate::{state_source::StateSource, TransactionParserFilter, TransactionParserTrait};
use solana_sdk::pubkey::Pubkey;
use step_ingestooor_sdk::{
    dooot::{
        CurveType, DLMMPart, DlmmGlobalDooot, DlmmUserDooot, Dooot, LPInfoDooot,
        LPInfoUnderlyingMintVault,
    },
    trait_defs::transaction::TransactionLike,
    utils::step_utils::StepUtils,
};

pub const METEORA_DLMM_PROGRAM_PK: &str = "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo";

pub const POSITION_V2_DISCRIMINATOR: &[u8] = &[0x75, 0xB0, 0xD4, 0xC7, 0xF5, 0xB4, 0x85, 0xB6];
pub const LB_PAIR_DISCRIMINATOR: &[u8] = &[0x21, 0x0B, 0x31, 0x62, 0xB5, 0x65, 0xB1, 0x0D];
pub const BIN_ARRAY_DISCRIMINATOR: &[u8] = &[0x5C, 0x8E, 0x5C, 0xDC, 0x05, 0x94, 0x46, 0xB5];

/// See https://github.com/hubbleprotocol/hubble-common/blob/5409774e24bcf9a0041a5bfa7a4728b015db0730/packages/kamino-sdk/src/utils/meteora.ts#L49
pub struct MeteoraDLMM;

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    bytemuck::Pod,
    bytemuck::Zeroable,
    borsh::BorshDeserialize,
)]
#[repr(C)]
pub struct Bin {
    pub amount_x: u64,
    pub amount_y: u64,
    pub price: u128,
    pub liquidity_supply: u128,
    pub reward_per_token_stored: [u128; 2],
    pub fee_amount_x_per_token_stored: u128,
    pub fee_amount_y_per_token_stored: u128,
    pub amount_x_in: u128,
    pub amount_y_in: u128,
}

#[derive(Debug, Clone, BorshDeserialize)]
#[repr(C)]
pub struct BinArray {
    pub index: i64,
    pub version: u8,
    pub _padding: [u8; 7],
    pub lb_pair: Pubkey,
    pub bins: [Bin; 70],
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    bytemuck::Pod,
    bytemuck::Zeroable,
    borsh::BorshDeserialize,
)]
#[repr(C)]
pub struct UserRewardInfo {
    pub reward_per_token_completes: [u128; 2],
    pub reward_pendings: [u64; 2],
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    bytemuck::Pod,
    bytemuck::Zeroable,
    borsh::BorshDeserialize,
)]
#[repr(C)]
pub struct FeeInfo {
    pub fee_x_per_token_complete: u128,
    pub fee_y_per_token_complete: u128,
    pub fee_x_pending: u64,
    pub fee_y_pending: u64,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PositionV2 {
    pub lb_pair: Pubkey,
    pub owner: Pubkey,
    pub liquidity_shares: [u128; 70],
    pub reward_infos: [UserRewardInfo; 70],
    pub fee_infos: [FeeInfo; 70],
    pub lower_bin_id: i32,
    pub upper_bin_id: i32,
    pub last_updated_at: i64,
    pub total_claimed_fee_x_amount: u64,
    pub total_claimed_fee_y_amount: u64,
    pub total_claimed_rewards: [u64; 2],
    pub operator: Pubkey,
    pub lock_release_point: u64,
    pub _padding_0: u8,
    pub fee_owner: Pubkey,
    pub _reserved: [u8; 87],
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable, borsh::BorshDeserialize,
)]
#[repr(C)]
pub struct ProtocolFee {
    pub amount_x: u64,
    pub amount_y: u64,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable, borsh::BorshDeserialize,
)]
#[repr(C)]
pub struct StaticParameters {
    pub base_factor: u16,
    pub filter_period: u16,
    pub decay_period: u16,
    pub reduction_factor: u16,
    pub variable_fee_control: u32,
    pub max_volatility_accumulator: u32,
    pub min_bin_id: i32,
    pub max_bin_id: i32,
    pub protocol_share: u16,
    pub base_fee_power_factor: u8,
    pub _padding: [u8; 5],
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable, borsh::BorshDeserialize,
)]
#[repr(C)]
pub struct VariableParameters {
    pub volatility_accumulator: u32,
    pub volatility_reference: u32,
    pub index_reference: i32,
    pub _padding: [u8; 4],
    pub last_update_timestamp: i64,
    pub _padding_1: [u8; 8],
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    bytemuck::Pod,
    bytemuck::Zeroable,
    borsh::BorshDeserialize,
)]
#[repr(C)]
pub struct RewardInfo {
    pub mint: Pubkey,
    pub vault: Pubkey,
    pub funder: Pubkey,
    pub reward_duration: u64,
    pub reward_duration_end: u64,
    pub reward_rate: u128,
    pub last_update_time: u64,
    pub cumulative_seconds_with_empty_liquidity_reward: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, bytemuck::Zeroable, borsh::BorshDeserialize)]
#[repr(C)]
pub struct LbPair {
    pub parameters: StaticParameters,
    pub v_parameters: VariableParameters,
    pub bump_seed: [u8; 1],
    pub bin_step_seed: [u8; 2],
    pub pair_type: u8,
    pub active_id: i32,
    pub bin_step: u16,
    pub status: u8,
    pub require_base_factor_seed: u8,
    pub base_factor_seed: [u8; 2],
    pub activation_type: u8,
    pub creator_pool_on_off_control: u8,
    pub token_x_mint: Pubkey,
    pub token_y_mint: Pubkey,
    pub reserve_x: Pubkey,
    pub reserve_y: Pubkey,
    pub protocol_fee: ProtocolFee,
    pub _padding_1: [u8; 32],
    pub reward_infos: [RewardInfo; 2],
    pub oracle: Pubkey,
    pub bin_array_bitmap: [u64; 16],
    pub last_updated_at: i64,
    pub _padding_2: [u8; 32],
    pub pre_activation_swap_address: Pubkey,
    pub base_key: Pubkey,
    pub activation_point: u64,
    pub pre_activation_duration: u64,
    pub _padding_3: [u8; 8],
    pub _padding_4: u64,
    pub creator: Pubkey,
    pub token_mint_x_program_flag: u8,
    pub token_mint_y_program_flag: u8,
    pub _reserved: [u8; 22],
}

#[async_trait]
impl TransactionParserTrait for MeteoraDLMM {
    fn get_name(&self) -> &'static str {
        "MeteoraDLMM"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            METEORA_DLMM_PROGRAM_PK.into(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let mut pool_lookup = HashMap::new();
        let mut positions = HashMap::new();
        let mut bin_arrays = HashMap::new();

        for account in txn.iter_accounts() {
            if !account.datum_changed {
                continue;
            }

            let pre = account
                .unpack_pre_datum()
                .context("Failed to unpack pre datum")?;
            let post = account
                .unpack_post_datum()
                .context("Failed to unpack post datum")?;

            let (is_closed, account_data) = match (pre, post) {
                (_, Some(post)) => (false, post),
                (Some(pre), None) => (true, pre),
                _ => continue,
            };

            if account_data.len() < 8 {
                continue;
            }

            let discriminator = &account_data[..8];
            let pubkey = account.pubkey;

            if discriminator == LB_PAIR_DISCRIMINATOR {
                if let Ok(pool) = Self::lb_pair_reader(&account_data[8..]) {
                    pool_lookup.insert(pubkey, (account.was_created, pool));
                }
            } else if discriminator == POSITION_V2_DISCRIMINATOR {
                if let Ok(position) = Self::position_v2_reader(&account_data[8..]) {
                    positions.insert(pubkey, (position, is_closed));
                }
            } else if discriminator == BIN_ARRAY_DISCRIMINATOR {
                if let Ok(bin_array) = Self::bin_array_reader(&account_data[8..]) {
                    bin_arrays.insert(pubkey, (bin_array, is_closed));
                }
            }
        }

        let time = txn.get_block_time();

        for pool_key in pool_lookup.keys() {
            let entry = pool_lookup
                .get(pool_key)
                .ok_or_else(|| anyhow!("UNREACHABLE - should find pool in lookup"))?;
            let pool = &entry.1;
            let was_created = entry.0;

            let token_x = pool.token_x_mint.to_string();
            let token_y = pool.token_y_mint.to_string();
            let vault_x = pool.reserve_x.to_string();
            let vault_y = pool.reserve_y.to_string();

            if was_created {
                let info_dooot = Dooot::LPInfo(LPInfoDooot {
                    time,
                    curve_type: CurveType::DynamicLiquidity,
                    lp_mint: None,
                    pool: pool_key.to_string(),
                    program: METEORA_DLMM_PROGRAM_PK.to_string(),
                    underlyings: vec![
                        LPInfoUnderlyingMintVault {
                            mint: token_x.clone(),
                            vault: vault_x.clone(),
                        },
                        LPInfoUnderlyingMintVault {
                            mint: token_y.clone(),
                            vault: vault_y.clone(),
                        },
                    ],
                });

                dooot.push(info_dooot);
            }
        }

        for (key, (bin_array_ref, is_closed)) in bin_arrays {
            let parsed_parts = bin_array_ref
                .bins
                .iter()
                .map(Self::bin_to_dlmm_part)
                .collect();
            let parts_dooot = Dooot::DlmmGlobal(DlmmGlobalDooot {
                time,
                pool_pubkey: bin_array_ref.lb_pair.to_string(),
                parts_account_pubkey: key.to_string(),
                part_index: bin_array_ref.index as i32,
                parts: parsed_parts,
                closed: is_closed,
            });
            dooot.push(parts_dooot);
        }

        for (key, (position_ref, is_closed)) in positions {
            let position = position_ref;
            let user_pubkey = position.owner.to_string();

            let (lower_pk, upper_pk) = Self::derive_lower_and_upper_pubkeys(&position);

            // Now create user position dooot to insert
            let dlmm_position_dooot: Dooot = Dooot::DlmmUser(DlmmUserDooot {
                time,
                lower_part_account_pubkey: lower_pk.to_string(),
                upper_part_account_pubkey: upper_pk.to_string(),
                user_pubkey: user_pubkey.clone(),
                shares: position
                    .liquidity_shares
                    .into_iter()
                    .map(|x| x.to_string())
                    .collect(),
                position_mint_pubkey: Some(key.to_string()),
                // `Lower` binarray is user's "primary", so we'll put fees here
                fees_claimed: vec![
                    position.total_claimed_fee_x_amount.into(),
                    position.total_claimed_fee_y_amount.into(),
                ],
                lower_inner_id: position.lower_bin_id,
                upper_inner_id: position.upper_bin_id,
                closed: is_closed,
            });

            dooot.push(dlmm_position_dooot);
        }

        Ok(())
    }
}

impl StateSource for MeteoraDLMM {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let blocktime = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;

        let accts =
            acct_provider.program_accounts(Pubkey::from_str(METEORA_DLMM_PROGRAM_PK)?, None);

        let mut pool_lookup = HashMap::new();
        let mut bin_arrays = HashMap::new();
        let mut positions = HashMap::new();

        for account in accts {
            let account_data = account.account.data();
            if account_data.len() < 8 {
                continue;
            }

            let discriminator = &account_data[..8];
            let pubkey = account.pubkey;

            if discriminator == LB_PAIR_DISCRIMINATOR {
                if let Ok(pool) = Self::lb_pair_reader(&account_data[8..]) {
                    pool_lookup.insert(pubkey, pool);
                }
            } else if discriminator == POSITION_V2_DISCRIMINATOR {
                if let Ok(position) = Self::position_v2_reader(&account_data[8..]) {
                    positions.insert(pubkey, position);
                }
            } else if discriminator == BIN_ARRAY_DISCRIMINATOR {
                if let Ok(bin_array) = Self::bin_array_reader(&account_data[8..]) {
                    bin_arrays.insert(pubkey, bin_array);
                }
            }
        }

        for (pk, pool) in pool_lookup {
            let info_dooot = Dooot::LPInfo(LPInfoDooot {
                time: blocktime,
                curve_type: CurveType::DynamicLiquidity,
                lp_mint: None,
                pool: pk.to_string(),
                program: METEORA_DLMM_PROGRAM_PK.to_string(),
                underlyings: vec![
                    LPInfoUnderlyingMintVault {
                        mint: pool.token_x_mint.to_string(),
                        vault: pool.reserve_x.to_string(),
                    },
                    LPInfoUnderlyingMintVault {
                        mint: pool.token_y_mint.to_string(),
                        vault: pool.reserve_y.to_string(),
                    },
                ],
            });

            tx.send(info_dooot)?;
        }

        for (pk, bin_array) in bin_arrays.iter() {
            let bin_dooot = Dooot::DlmmGlobal(DlmmGlobalDooot {
                time: blocktime,
                part_index: bin_array.index as i32,
                parts: bin_array.bins.iter().map(Self::bin_to_dlmm_part).collect(),
                parts_account_pubkey: pk.to_string(),
                pool_pubkey: bin_array.lb_pair.to_string(),
                closed: false,
            });

            tx.send(bin_dooot)?;
        }

        for (key, position) in positions {
            let (lower_pk, upper_pk) = Self::derive_lower_and_upper_pubkeys(&position);

            let dlmm_position_dooot = Dooot::DlmmUser(DlmmUserDooot {
                time: blocktime,
                lower_part_account_pubkey: lower_pk.to_string(),
                upper_part_account_pubkey: upper_pk.to_string(),
                user_pubkey: position.owner.to_string(),
                shares: position
                    .liquidity_shares
                    .into_iter()
                    .map(|x| x.to_string())
                    .collect(),
                position_mint_pubkey: Some(key.to_string()),
                fees_claimed: vec![
                    position.total_claimed_fee_x_amount.into(),
                    position.total_claimed_fee_y_amount.into(),
                ],
                lower_inner_id: position.lower_bin_id,
                upper_inner_id: position.upper_bin_id,
                closed: false,
            });

            tx.send(dlmm_position_dooot)?;
        }

        Ok(())
    }

    fn get_name(&self) -> &'static str {
        "MeteoraDLMM"
    }
}

impl MeteoraDLMM {
    pub const MAX_BIN_ARRAY_SIZE: i64 = 70;
    pub const SCALE_OFFSET: u8 = 64;

    #[inline]
    pub fn bin_to_dlmm_part(bin: &Bin) -> DLMMPart {
        let price = bin.price;
        let fee_per_x = bin.fee_amount_x_per_token_stored;
        let fee_per_y = bin.fee_amount_y_per_token_stored;
        let liq = bin.liquidity_supply;
        let liq_sh = liq >> Self::SCALE_OFFSET;
        let fees_x = ((liq_sh * fee_per_x) >> Self::SCALE_OFFSET) as u64;
        let fees_y = ((liq_sh * fee_per_y) >> Self::SCALE_OFFSET) as u64;

        DLMMPart {
            shares: liq.to_string(),
            token_amounts: vec![bin.amount_x.into(), bin.amount_y.into()],
            fee_amounts: vec![fees_x.into(), fees_y.into()],
            price: price.to_string(),
        }
    }

    #[inline]
    pub fn get_token_holdings_for_position(
        bin_arrays: &[&BinArray],
        position: &PositionV2,
    ) -> (u64, u64) {
        let lower_bin_id = position.lower_bin_id;
        let upper_bin_id = position.upper_bin_id;

        let mut total_token_a: u64 = 0;
        let mut total_token_b: u64 = 0;
        for idx in lower_bin_id..=upper_bin_id {
            let Some(bin) = Self::get_bin_from_bin_arrays(idx, bin_arrays) else {
                continue;
            };

            let bin_x = bin.amount_x;
            let bin_y = bin.amount_y;
            let bin_liq = bin.liquidity_supply;
            let pos_liq_index = idx - lower_bin_id;
            let position_liq = position.liquidity_shares[pos_liq_index as usize];
            let liq_ratio = (position_liq as f64) / (bin_liq as f64);
            let amount_a = (bin_x as f64) * liq_ratio;
            let amount_b = (bin_y as f64) * liq_ratio;
            total_token_a += amount_a as u64;
            total_token_b += amount_b as u64;
        }

        (total_token_a, total_token_b)
    }

    #[inline]
    fn get_bin_array_lower_upper_bin_id(bin_array_index: i64) -> (i64, i64) {
        let lower_id = bin_array_index * Self::MAX_BIN_ARRAY_SIZE;
        let upper_id = lower_id + Self::MAX_BIN_ARRAY_SIZE - 1;

        (lower_id, upper_id)
    }

    #[inline]
    fn get_bin_from_bin_array(bin_idx: i32, bin_array: &BinArray) -> Option<&Bin> {
        let (lower_id, ..) = Self::get_bin_array_lower_upper_bin_id(bin_array.index);
        let offset = bin_idx as i64 - lower_id;
        if offset < 0 {
            return None;
        }

        bin_array.bins.get(offset as usize)
    }

    #[inline]
    pub fn get_bin_from_bin_arrays<'a>(
        bin_idx: i32,
        bin_arrays: &[&'a BinArray],
    ) -> Option<&'a Bin> {
        bin_arrays
            .iter()
            .find_map(|bin_array| Self::get_bin_from_bin_array(bin_idx, bin_array))
    }

    #[inline]
    pub fn bin_id_to_bin_array_index(bin_id: i32) -> i32 {
        let idx = bin_id / (Self::MAX_BIN_ARRAY_SIZE as i32);
        let rem = bin_id % (Self::MAX_BIN_ARRAY_SIZE as i32);

        if bin_id.is_negative() && rem != 0 {
            idx - 1
        } else {
            idx
        }
    }

    #[inline]
    pub fn derive_bin_array_addr(lb_pair: &Pubkey, index: i32, program_id: &Pubkey) -> Pubkey {
        let index_bytes = (index as i64).to_le_bytes();

        Pubkey::find_program_address(
            &["bin_array".as_ref(), lb_pair.as_ref(), &index_bytes],
            program_id,
        )
        .0
    }

    #[inline]
    pub fn derive_lower_and_upper_pubkeys(pos: &PositionV2) -> (Pubkey, Pubkey) {
        let lower_idx = Self::bin_id_to_bin_array_index(pos.lower_bin_id);
        let upper_idx = Self::bin_id_to_bin_array_index(pos.upper_bin_id);

        let lower_pk = Self::derive_bin_array_addr(
            &pos.lb_pair,
            lower_idx,
            &Pubkey::from_str(METEORA_DLMM_PROGRAM_PK).expect("MeteoraDLMM:Invalid program id"),
        );
        let upper_pk = Self::derive_bin_array_addr(
            &pos.lb_pair,
            upper_idx,
            &Pubkey::from_str(METEORA_DLMM_PROGRAM_PK).expect("MeteoraDLMM: Invalid program id"),
        );

        (lower_pk, upper_pk)
    }
    pub fn position_v2_reader(account: &[u8]) -> Result<PositionV2> {
        let mut reader = StepAccountReader::new(account);
        let lb_pair = *reader.read_next_pod_type::<Pubkey>()?;
        let owner = *reader.read_next_pod_type::<Pubkey>()?;

        let mut liquidity_shares = [0u128; 70];
        for i in &mut liquidity_shares {
            *i = reader.read_next_le_u128()?;
        }

        let mut reward_infos = [UserRewardInfo::default(); 70];
        for i in &mut reward_infos {
            let reward_per_token_completes_1 = reader.read_next_le_u128()?;
            let reward_per_token_completes_2 = reader.read_next_le_u128()?;
            let reward_pendings_1 = reader.read_next_le_u64()?;
            let reward_pendings_2 = reader.read_next_le_u64()?;
            *i = UserRewardInfo {
                reward_per_token_completes: [
                    reward_per_token_completes_1,
                    reward_per_token_completes_2,
                ],
                reward_pendings: [reward_pendings_1, reward_pendings_2],
            };
        }

        let mut fee_infos = [FeeInfo::default(); 70];
        for i in &mut fee_infos {
            let fee_x_per_token_complete = reader.read_next_le_u128()?;
            let fee_y_per_token_complete = reader.read_next_le_u128()?;
            let fee_x_pending = reader.read_next_le_u64()?;
            let fee_y_pending = reader.read_next_le_u64()?;
            *i = FeeInfo {
                fee_x_per_token_complete,
                fee_y_per_token_complete,
                fee_x_pending,
                fee_y_pending,
            };
        }

        let lower_bin_id = reader.read_next_le_i32()?;
        let upper_bin_id = reader.read_next_le_i32()?;
        let last_updated_at = reader.read_next_le_i64()?;
        let total_claimed_fee_x_amount = reader.read_next_le_u64()?;
        let total_claimed_fee_y_amount = reader.read_next_le_u64()?;
        let mut total_claimed_rewards = [0u64; 2];
        total_claimed_rewards[0] = reader.read_next_le_u64()?;
        total_claimed_rewards[1] = reader.read_next_le_u64()?;

        let operator = *reader.read_next_pod_type::<Pubkey>()?;
        let lock_release_point = reader.read_next_le_u64()?;
        let _padding_0 = reader.read_next_single_byte()?;
        let fee_owner = *reader.read_next_pod_type::<Pubkey>()?;
        let mut _reserved = [0u8; 87];
        let reserved_bytes = reader.read_next_bytes(87)?;
        _reserved.copy_from_slice(reserved_bytes);

        Ok(PositionV2 {
            lb_pair,
            owner,
            liquidity_shares,
            reward_infos,
            fee_infos,
            lower_bin_id,
            upper_bin_id,
            last_updated_at,
            total_claimed_fee_x_amount,
            total_claimed_fee_y_amount,
            total_claimed_rewards,
            operator,
            lock_release_point,
            _padding_0,
            fee_owner,
            _reserved,
        })
    }

    pub fn bin_array_reader(data: &[u8]) -> Result<BinArray> {
        let mut reader = StepAccountReader::new(data);
        let index = reader.read_next_le_i64()?;
        let version = reader.read_next_single_byte()?;
        let mut _padding = [0u8; 7];
        let pad_bytes = reader.read_next_bytes(7)?;
        _padding.copy_from_slice(pad_bytes);

        let lb_pair = *reader.read_next_pubkey()?;
        reader.move_index_to(48);
        let mut bins = [Bin::default(); 70];
        for i in &mut bins {
            let amount_x = reader.read_next_le_u64()?;
            let amount_y = reader.read_next_le_u64()?;
            let price = reader.read_next_le_u128()?;
            let liquidity_supply = reader.read_next_le_u128()?;

            let mut reward_per_token_stored = [0u128; 2];
            reward_per_token_stored[0] = reader.read_next_le_u128()?;
            reward_per_token_stored[1] = reader.read_next_le_u128()?;

            let fee_amount_x_per_token_stored = reader.read_next_le_u128()?;
            let fee_amount_y_per_token_stored = reader.read_next_le_u128()?;
            let amount_x_in = reader.read_next_le_u128()?;
            let amount_y_in = reader.read_next_le_u128()?;

            *i = Bin {
                amount_x,
                amount_y,
                price,
                liquidity_supply,
                reward_per_token_stored,
                fee_amount_x_per_token_stored,
                fee_amount_y_per_token_stored,
                amount_x_in,
                amount_y_in,
            };
        }
        let bin_array = BinArray {
            index,
            version,
            _padding,
            lb_pair,
            bins,
        };
        Ok(bin_array)
    }

    pub fn lb_pair_reader(data: &[u8]) -> Result<LbPair> {
        let mut reader = StepAccountReader::new(data);
        let offset = reader.current_index();
        let parameters = StaticParameters::from_bytes(&data[offset..])
            .context("Failed to decode StaticParameters")?;
        reader.advance_unclamped(32);

        let offset = reader.current_index();
        let v_parameters = VariableParameters::from_bytes(&data[offset..])
            .context("Failed to decode VariableParameters")?;
        reader.advance_unclamped(32);

        let bump_seed = *reader.read_next_pod_type::<[u8; 1]>()?;
        let bin_step_seed = *reader.read_next_pod_type::<[u8; 2]>()?;
        let pair_type = reader.read_next_single_byte()?;
        let active_id = reader.read_next_le_i32()?;
        let bin_step = reader.read_next_le_u16()?;
        let status = reader.read_next_single_byte()?;
        let require_base_factor_seed = reader.read_next_single_byte()?;
        let base_factor_seed = *reader.read_next_pod_type::<[u8; 2]>()?;
        let activation_type = reader.read_next_single_byte()?;
        let creator_pool_on_off_control = reader.read_next_single_byte()?;
        let token_x_mint = *reader.read_next_pod_type::<Pubkey>()?;
        let token_y_mint = *reader.read_next_pod_type::<Pubkey>()?;
        let reserve_x = *reader.read_next_pod_type::<Pubkey>()?;
        let reserve_y = *reader.read_next_pod_type::<Pubkey>()?;
        let protocol_fee = *reader.read_next_pod_type::<ProtocolFee>()?;
        let mut _padding_1 = [0u8; 32];
        _padding_1.copy_from_slice(reader.read_next_bytes(32)?);

        let mut reward_infos = [RewardInfo::default(); 2];

        for (i, slot) in reward_infos.iter_mut().enumerate() {
            let start = reader.current_index();
            let end = start + 144;
            let info_ref = RewardInfo::from_bytes(&data[start..end])
                .context(format!("Failed to read reward info {}", i))?;
            *slot = *info_ref;
            reader.advance_unclamped(144);
        }

        let mut bin_array_bitmap = [0u64; 16];
        for i in &mut bin_array_bitmap {
            *i = reader.read_next_le_u64()?;
        }

        let oracle = *reader.read_next_pod_type::<Pubkey>()?;
        let last_updated_at = reader.read_next_le_i64()?;

        let mut _padding_2 = [0u8; 32];
        _padding_2.copy_from_slice(reader.read_next_bytes(32)?);

        let pre_activation_swap_address = *reader.read_next_pod_type::<Pubkey>()?;
        let base_key = *reader.read_next_pod_type::<Pubkey>()?;

        let activation_point = reader.read_next_le_u64()?;
        let pre_activation_duration = reader.read_next_le_u64()?;

        let mut _padding_3 = [0u8; 8];
        _padding_3.copy_from_slice(reader.read_next_bytes(8)?);

        let mut _padding_4 = reader.read_next_le_u64()?;

        let creator = *reader.read_next_pod_type::<Pubkey>()?;
        let token_mint_x_program_flag = reader.read_next_single_byte()?;
        let token_mint_y_program_flag = reader.read_next_single_byte()?;
        let mut _reserved = [0u8; 22];
        _reserved.copy_from_slice(reader.read_next_bytes(22)?);

        Ok(LbPair {
            parameters: *parameters,
            v_parameters: *v_parameters,
            bump_seed,
            bin_step_seed,
            pair_type,
            active_id,
            bin_step,
            status,
            require_base_factor_seed,
            base_factor_seed,
            activation_type,
            creator_pool_on_off_control,
            token_x_mint,
            token_y_mint,
            reserve_x,
            reserve_y,
            protocol_fee,
            _padding_1,
            reward_infos,
            oracle,
            bin_array_bitmap,
            last_updated_at,
            _padding_2,
            pre_activation_swap_address,
            base_key,
            activation_point,
            pre_activation_duration,
            _padding_3,
            _padding_4,
            creator,
            token_mint_x_program_flag,
            token_mint_y_program_flag,
            _reserved,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::str::FromStr;

    use super::MeteoraDLMM;
    use crate::amm::dlmm::meteora::METEORA_DLMM_PROGRAM_PK;
    use crate::Dooot::DlmmUser;
    use crate::TransactionParserTrait;
    use solana_sdk::pubkey::Pubkey;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    use step_ingestooor_sdk::utils::test_utils::init_test_extras_bare;

    #[test]
    fn check_math_overflows() {
        let bin_idx1: i64 = -5;
        let bin_idx2: i64 = 5;
        let bin_idx3: i64 = 1283109230119012;
        let bin_idx4: i64 = -1283109230119012;

        let (lower, ..) = MeteoraDLMM::get_bin_array_lower_upper_bin_id(bin_idx1);
        let _calc_offset = bin_idx1 - lower;
        let (lower, ..) = MeteoraDLMM::get_bin_array_lower_upper_bin_id(bin_idx2);
        let _calc_offset = bin_idx2 - lower;
        let (lower, ..) = MeteoraDLMM::get_bin_array_lower_upper_bin_id(bin_idx3);
        let _calc_offset = bin_idx3 - lower;
        let (lower, ..) = MeteoraDLMM::get_bin_array_lower_upper_bin_id(bin_idx4);
        let _calc_offset = bin_idx4 - lower;
    }

    #[test]
    fn deriving_bin_arrays() {
        let lb_pair = Pubkey::from_str("HTvjzsfX3yU6BUodCjZ5vZkUrAxMDTrBs3CJaq43ashR").unwrap();
        let program_id = Pubkey::from_str(METEORA_DLMM_PROGRAM_PK).unwrap();
        let lower_id = -19505;
        let upper_id = -19437;

        let lower_idx = MeteoraDLMM::bin_id_to_bin_array_index(lower_id);
        let upper_idx = MeteoraDLMM::bin_id_to_bin_array_index(upper_id);

        let lower_pubkey =
            MeteoraDLMM::derive_bin_array_addr(&lb_pair, lower_idx, &program_id).to_string();
        let upper_pubkey =
            MeteoraDLMM::derive_bin_array_addr(&lb_pair, upper_idx, &program_id).to_string();

        assert_eq!(lower_pubkey, "A7gbKm6RrhgYqjosSjk2xhL8pRyapSKbKYZc6887jVkj");
        assert_eq!(upper_pubkey, "CPsd3qWfeds4DZ4VUsxebqtgyVi7MPr2BPkPaHBucKGh");
    }

    #[tokio::test]
    async fn failing_dlq() {
        let file = fs::read_to_string("./tests/meteora_amm/meteora_failing.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let parser = MeteoraDLMM {};
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooots = vec![];

        parser.invoke(&txn, &mut dooots, &step_utils).await.unwrap();
    }

    #[tokio::test]
    async fn deposit() {
        let file = fs::read_to_string("./tests/meteora_dlmm/deposit.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let parser = MeteoraDLMM {};
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooots = vec![];

        parser.invoke(&txn, &mut dooots, &step_utils).await.unwrap();

        let DlmmUser(dlmm_user) = &dooots[2] else {
            panic!("Expected a clmm user dooot");
        };
        println!("{:#?}", dooots);

        assert_eq!(dooots.len(), 3);
        assert_eq!(
            dlmm_user.user_pubkey,
            "JAMESC37CTVoFEt7TAEcqBjdjAfAWZiPR1YdWotAFjeQ".to_string()
        );
    }

    #[tokio::test]
    async fn claim() {
        let file = fs::read_to_string("./tests/meteora_dlmm/claim.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let parser = MeteoraDLMM {};
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooots = vec![];

        parser.invoke(&txn, &mut dooots, &step_utils).await.unwrap();
        let DlmmUser(dlmm_user) = &dooots[0] else {
            panic!("Expected a clmm user dooot");
        };
        println!("{:#?}", dooots);

        assert_eq!(dooots.len(), 1);
        assert_eq!(
            dlmm_user.user_pubkey,
            "JAMESC37CTVoFEt7TAEcqBjdjAfAWZiPR1YdWotAFjeQ".to_string()
        );
    }

    #[tokio::test]
    async fn withdraw_close_all() {
        let file = fs::read_to_string("./tests/meteora_dlmm/withdraw_close_all.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let parser = MeteoraDLMM {};
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooots = vec![];
        parser.invoke(&txn, &mut dooots, &step_utils).await.unwrap();
        assert_eq!(dooots.len(), 3);
    }
}
