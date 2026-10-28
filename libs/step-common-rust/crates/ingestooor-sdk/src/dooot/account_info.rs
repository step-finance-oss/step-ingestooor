use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "AccountInfo")]
#[clickhouse(
    table = "incoming_lookup_account_info_dooot",
    queue = "account_info_dooot",
    flush_ms = 5000
)]
pub struct AccountInfoDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    //key
    pub account_pubkey: String,

    //on a project account
    #[ts(optional)]
    pub tr_project_name: Option<String>,

    //on a referrer account
    #[ts(optional)]
    pub tr_project_key: Option<String>,
    #[ts(optional)]
    pub tr_referral_account_name: Option<String>,

    //on a referrer token account
    #[ts(optional)]
    pub tr_referral_account_key: Option<String>,
}
