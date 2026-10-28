use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

/// This dooot represents slot stats.
/// The topic prefix is `SlotStats`.
/// The dooot is exposed as a general topic `SlotStats` and has no other field level topics.
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "SlotStatsGlobal", topic)]
#[clickhouse(
    table = "incoming_slot_stats_global_dooot",
    queue = "slot_stats_global_dooot",
    flush_ms = 5000
)]
pub struct SlotStatsGlobalDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: chrono::NaiveDateTime,
    pub slot: i64,
    pub tx_success_count: i32,
    pub tx_success_fees: i64,
    pub tx_success_fees_priority: i64,
    pub tx_error_count: i32,
    pub tx_error_fees: i64,
    pub tx_error_fees_priority: i64,
    pub tx_vote_count: i32,
    pub tx_vote_fees: i64,
    pub tx_vote_error_count: i32,
    pub tx_vote_error_fees: i64,
    pub programs: Vec<String>,
    pub program_success_counts: Vec<i32>,
    pub program_error_counts: Vec<i32>,
    pub payers: Vec<String>,
    pub mints: Vec<String>,
    pub mint_new_account_counts: Vec<i32>,
    pub new_mints_fungible_count: i32,
    pub new_mints_nonfungible_count: i32,
    #[ts(optional)]
    pub info: Option<String>,
}
