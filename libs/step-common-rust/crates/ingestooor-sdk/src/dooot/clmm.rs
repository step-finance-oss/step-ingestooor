use chrono::NaiveDateTime;
use serde::Deserialize;
use serde::Serialize;
use step_common_rust::evalexpr::ValueWrapper;
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(Serialize, Deserialize, Debug, Clone, Default, TS, clickhouse::Row)]
pub struct ClmmTick {
    /// u128
    #[ts(type = "number")]
    pub liquidity_gross: String,
    /// i128
    #[ts(type = "number")]
    pub liquidity_net: String,
    /// u128
    #[ts(type = "number")]
    pub fee_growth_outside_a: String,
    /// u128
    #[ts(type = "number")]
    pub fee_growth_outside_b: String,
}

impl From<ClmmTick> for ValueWrapper {
    fn from(tick: ClmmTick) -> Self {
        ValueWrapper::new(evalexpr::Value::String(tick.liquidity_gross))
    }
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "ClmmTickGlobal")]
#[clickhouse(
    table = "incoming_clmm_tick_global_dooot",
    queue = "clmm_tick_global_dooot",
    flush_ms = 2000
)]
pub struct ClmmTickGlobalDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub pool_pubkey: String,
    #[topic]
    pub tick_account_pubkey: String,
    pub tick_index: i32,
    pub ticks: Vec<ClmmTick>,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "ClmmGlobal")]
#[clickhouse(
    table = "incoming_clmm_global_dooot",
    queue = "clmm_global_dooot",
    flush_ms = 5000
)]
pub struct ClmmGlobalDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub pool_pubkey: String,
    /// u128
    #[ts(type = "number", optional)]
    pub total_liquidity_shares: Option<String>,
    /// u128
    #[ts(type = "number")]
    pub current_price: String,
    pub current_tick_index: i32,
    pub tick_spacing: u32,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "ClmmUser")]
#[clickhouse(
    table = "incoming_clmm_user_dooot",
    queue = "clmm_user_dooot",
    flush_ms = 2000
)]
pub struct ClmmUserDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub pool_pubkey: String,
    #[topic]
    pub user_pubkey: String,
    #[ts(optional)]
    pub position_mint: Option<String>,
    /// u128
    #[ts(type = "number")]
    pub upper_price: String,
    /// u128
    #[ts(type = "number")]
    pub lower_price: String,
    /// u128
    #[ts(type = "number")]
    pub liquidity_shares: String,
    pub closed: bool,
}
