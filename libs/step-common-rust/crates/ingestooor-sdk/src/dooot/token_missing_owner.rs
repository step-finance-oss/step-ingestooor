use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

//This dooot is used by tx hist to track incoming token xfers without the owner in the accounts.
//it's not published to topics
#[doc(hidden)]
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "TokenMissingOwner")]
#[clickhouse(
    table = "incoming_token_missing_owner_global_dooot",
    queue = "token_missing_owner_global_dooot",
    flush_ms = 15000
)]
pub struct TokenMissingOwnerGlobalDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    pub owner_pubkey: String,
    pub signature: String,
}
