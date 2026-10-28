use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "SolBalanceUser")]
#[clickhouse(
    table = "incoming_sol_balance_user_dooot",
    queue = "sol_balance_user_dooot",
    flush_ms = 2000
)]
pub struct SolBalanceUserDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub account_pubkey: String,
    /// u64
    #[ts(type = "number")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64"
    )]
    pub balance: Decimal,
}
