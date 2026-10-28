use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "LendingGlobal")]
#[clickhouse(
    table = "incoming_lending_global_dooot",
    queue = "lending_global_dooot",
    flush_ms = 5000
)]
pub struct LendingGlobalDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub mint: String,
    #[topic]
    pub platform_program_pubkey: String,
    /// u64
    #[ts(type = "number")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64"
    )]
    pub total_deposits: Decimal,
    /// u64
    #[ts(type = "number")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64"
    )]
    pub total_borrows: Decimal,
    pub supply_apr: f64,
    pub borrow_apr: f64,
    /// Decimal
    #[ts(type = "number", optional)]
    pub supply_value_ratio: Option<Decimal>,
    /// Decimal
    #[ts(type = "number", optional)]
    pub borrow_value_ratio: Option<Decimal>,
    #[topic]
    pub pool_pubkey: String,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "LendingUser")]
#[clickhouse(
    table = "incoming_lending_user_dooot",
    queue = "lending_user_dooot",
    flush_ms = 2000
)]
pub struct LendingUserDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub mint: String,
    #[topic]
    pub platform_program_pubkey: String,
    #[topic]
    pub user_pubkey: String,
    /// u64
    #[ts(type = "number")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64"
    )]
    pub deposits: Decimal,
    /// u64
    #[ts(type = "number")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64"
    )]
    pub borrows: Decimal,
    /// Decimal
    #[ts(type = "number", optional)]
    pub deposit_shares: Option<Decimal>,
    /// Decimal
    #[ts(type = "number", optional)]
    pub borrow_shares: Option<Decimal>,
    #[topic]
    pub pool_pubkey: String,
    #[ts(optional)]
    pub lending_address_pubkey: Option<String>,
}
