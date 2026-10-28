use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "OraclePriceEvent")]
#[clickhouse(
    table = "incoming_oracle_price_event_dooot",
    queue = "oracle_price_event_dooot",
    flush_ms = 2000
)]
pub struct OraclePriceEventDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    /// Decimal
    #[ts(type = "number")]
    pub price: Decimal,
    #[ts(optional)]
    pub confidence: Option<u64>,
    #[topic]
    pub feed_account_pubkey: String,
    #[topic]
    pub feed_id: String,
    #[ts(optional)]
    pub mint: Option<String>,
    #[ts(optional)]
    pub exponent: Option<i32>,
}
