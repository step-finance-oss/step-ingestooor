use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "ManagedAccountUser")]
#[clickhouse(
    table = "incoming_managed_account_user_dooot",
    queue = "managed_account_user_dooot",
    flush_ms = 5000
)]
pub struct ManagedAccountUserDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    pub account_pubkey: String,
    pub user_pubkey: String,
    pub program_id: String,
    pub closed: bool,
}
