use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "ClaimableBalanceUser")]
#[clickhouse(
    table = "incoming_claimable_balance_user_dooot",
    queue = "claimable_balance_user_dooot",
    flush_ms = 5000
)]
pub struct ClaimableBalanceUserDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub program: String,
    #[topic]
    pub owner: String,
    // relates_to means market, farm, etc
    #[topic]
    pub relates_to: String,
    #[topic]
    pub relates_to_type: String,
    #[topic]
    pub mint: String,
    /// u64
    #[ts(type = "number")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64"
    )]
    pub amount: Decimal,
}
