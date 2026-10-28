use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "YieldBearingGlobal")]
#[clickhouse(
    table = "incoming_yield_bearing_global_dooot",
    queue = "yield_bearing_global_dooot",
    flush_ms = 5000
)]
pub struct YieldBearingGlobalDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    pub yield_bearing_mint: String,
    pub program_pubkey: String,
    pub apr: f64,
}
