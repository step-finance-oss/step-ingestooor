use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

/// This dooot represents mint metadata being created or updated.
/// The topic prefix is `MintMetadata`.
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "MintInfo")]
#[clickhouse(
    table = "incoming_mint_info_dooot",
    queue = "mint_info_dooot",
    flush_ms = 2000
)]
pub struct MintInfoDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    /// This field is exposed as a topic
    #[topic]
    pub mint: String,
    #[ts(optional)]
    pub symbol: Option<String>,
    #[ts(optional)]
    pub name: Option<String>,
    #[ts(optional)]
    pub description: Option<String>,
    #[ts(optional)]
    pub logouri: Option<String>,
    #[ts(optional)]
    pub token_standard: Option<i16>,
    #[ts(optional)]
    pub decimals: Option<i16>,
    #[ts(optional)]
    pub spl_version: Option<i16>,
    #[ts(optional)]
    pub mint_authority: Option<String>,
    #[ts(optional)]
    pub metadata_uri: Option<String>,
    #[ts(optional)]
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time_opt",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time_opt"
    )]
    pub metadata_fetch_success: Option<NaiveDateTime>,
    #[ts(optional)]
    pub metadata_fetch_tries: Option<u32>,
    #[ts(optional)]
    pub nft_collection_key: Option<String>,
}
