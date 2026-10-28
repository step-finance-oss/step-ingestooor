use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "TokenPriceGlobal")]
#[clickhouse(
    table = "incoming_token_price_global_dooot",
    queue = "token_price_global_dooot",
    flush_ms = 2000
)]
pub struct TokenPriceGlobalDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    pub mint: String,
    #[ts(type = "number")]
    pub price_usd: Decimal,
    pub deleted: bool,
}
