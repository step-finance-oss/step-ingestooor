use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

/// This dooot represents an NFT activity.
/// The topic prefix is `NftActivity`.
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "NftEvent", topic)]
pub struct NftEventDooot {
    pub signature: String,
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    /// This field is exposed as a topic
    #[topic]
    pub marketplace: String,
    /// This field is exposed as a topic
    #[topic]
    pub nft_pubkey: String,
    /// This field is exposed as a topic
    #[topic]
    #[ts(optional)]
    pub seller: Option<String>,
    /// This field is exposed as a topic
    #[topic]
    #[ts(optional)]
    pub buyer: Option<String>,
    pub token_name: String,
    #[ts(optional)]
    pub token_id: Option<i32>,
    pub token_image: String,
    /// This field is exposed as a topic
    #[topic]
    #[ts(optional)]
    pub collection_name: Option<String>,
    /// This field is exposed as a topic
    #[topic]
    #[ts(optional)]
    pub collection_code: Option<String>,
    #[ts(optional)]
    pub collection_image: Option<String>,
    #[ts(optional)]
    pub token_count: Option<u32>,
    /// This field is exposed as a topic
    #[topic]
    pub transaction_type: String,
    #[ts(optional)]
    pub amount: Option<f64>,
    #[ts(optional)]
    pub floor_price: Option<f64>,
    #[ts(optional)]
    pub owner_count: Option<f64>,
    #[ts(optional)]
    pub listed_count: Option<f64>,
    #[ts(optional)]
    pub slug: Option<String>,
    // this is a json stringified object of token attributes
    pub attributes: String,
}
