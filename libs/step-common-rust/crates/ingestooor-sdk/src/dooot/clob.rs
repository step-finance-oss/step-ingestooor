use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

/// This dooot represents a central limit order book fill.
/// The topic prefix is `ClobFill`.
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "ClobFillEvent")]
#[clickhouse(
    table = "incoming_clob_fill_event_dooot",
    queue = "clob_fill_event_dooot",
    flush_ms = 5000
)]
pub struct ClobFillEventDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub program_id: String,
    #[topic]
    pub market_pubkey: String,
    #[topic]
    pub base_mint_pubkey: String,
    #[topic]
    pub quote_mint_pubkey: String,
    #[topic]
    pub taker_pubkey: String,
    #[topic]
    pub maker_pubkey: String,
    pub buy_side: bool,
    pub base_qty: u64,
    #[ts(type = "number")]
    pub price: Decimal,
    pub taker_fee: f64,
    pub maker_fee: f64,
    #[topic]
    #[ts(optional)]
    pub referrer_pubkey: Option<String>,
    pub referral_fee: f64,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "ClobUser")]
#[clickhouse(
    table = "incoming_clob_user_dooot",
    queue = "clob_user_dooot",
    flush_ms = 2000
)]
pub struct ClobUserDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub program: String,
    #[topic]
    pub market: String,
    // pubkey of the open order
    #[topic]
    pub account: String,
    //owner = user wallet
    #[topic]
    pub owner: String,
    // Actual String
    pub order_id: String,
    pub is_buy: bool,
    /// Decimal
    #[ts(type = "number")]
    pub price: Decimal,
    /// Unsigned 64-bit integer
    pub unfilled_qty: u64,
    #[topic]
    pub base_mint: String,
    #[topic]
    pub quote_mint: String,
    #[ts(optional)]
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time_opt",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time_opt"
    )]
    pub expiry_ts: Option<NaiveDateTime>,
    pub closed: bool,
}
