use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_common_rust::evalexpr::ValueWrapper;
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

/// Same as LP one, but may diverge in future
#[derive(Serialize, Deserialize, Debug, Clone, Default, TS)]
pub struct FarmInfoUnderlyingMintVault {
    pub mint: String,
    pub vault: String,
}

impl From<FarmInfoUnderlyingMintVault> for ValueWrapper {
    fn from(value: FarmInfoUnderlyingMintVault) -> Self {
        format!("{}:{}", value.mint, value.vault).into()
    }
}

/// lookup_farm_info
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "FarmInfo")]
#[clickhouse(
    table = "incoming_lookup_farm_info_dooot",
    queue = "lookup_farm_info_dooot",
    flush_ms = 5000
)]
pub struct FarmInfoDooot {
    pub farm_pubkey: String,
    pub program_pubkey: String,
    pub deposit_token_mint: String,
    pub deposit_token_vault: String,
    pub reward_token_vaults: Vec<FarmInfoUnderlyingMintVault>,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "FarmGlobal")]
#[clickhouse(
    table = "incoming_farm_global_dooot",
    queue = "farm_global_dooot",
    flush_ms = 5000
)]
pub struct FarmGlobalDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    // 0: per slot, 1: per second
    pub per_unit_type: FarmPerUnitType,
    // slot, time, etc. defined in per_unit_type
    #[ts(type = "number")]
    pub last_updated: u64,
    // slot, time, etc. defined in per_unit_type
    #[ts(type = "number[]")]
    pub reward_end: Vec<u64>,
    #[topic]
    pub farm_pubkey: String,
    /// u64s.
    #[ts(type = "number[]")]
    pub reward_rates: Vec<u64>,
    /// u128s
    #[ts(type = "number[]")]
    pub reward_atoms_per_atom_stored: Vec<String>,
    #[ts(type = "number")]
    pub total_qty_staked: u64,
    pub closed: bool,
}

#[repr(u8)]
#[derive(serde_repr::Serialize_repr, serde_repr::Deserialize_repr, Debug, Clone, Default, TS)]
pub enum FarmPerUnitType {
    #[default]
    PerSlot,
    PerSecond,
}

impl From<FarmPerUnitType> for ValueWrapper {
    fn from(farm_per_unit_type: FarmPerUnitType) -> Self {
        match farm_per_unit_type {
            FarmPerUnitType::PerSlot => "PerSlot".to_string().into(),
            FarmPerUnitType::PerSecond => "PerSecond".to_string().into(),
        }
    }
}

// total_reward_atoms_per_atom = reward_atoms_per_atom_stored + ((min(now, reward_end) - last_updated) * reward_atoms_per_atom_elapsed)

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "FarmUser")]
#[clickhouse(
    table = "incoming_farm_user_dooot",
    queue = "farm_user_dooot",
    flush_ms = 2000
)]
pub struct FarmUserDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub farm_pubkey: String,
    #[topic]
    pub user_pubkey: String,
    #[topic]
    pub position_pubkey: String,
    /// u64s
    #[ts(type = "number")]
    pub qty_staked: u64,
    /// u64s
    #[ts(type = "number[]")]
    pub reward_atoms_pending: Vec<u64>,
    /// u128s
    #[ts(type = "number[]")]
    pub reward_atoms_per_atom_complete: Vec<String>,
    pub closed: bool,
}

// total_user_rewards = ((total_reward_atoms_per_atom - reward_atoms_per_atom_complete) * token_deposits) + reward_atoms_pending
