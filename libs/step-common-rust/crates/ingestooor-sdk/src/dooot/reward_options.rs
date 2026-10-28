use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

/// RewardOptionUserDooot dooot for tracking reward option balances
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "RewardOptionUser")]
#[clickhouse(
    table = "incoming_reward_option_user_dooot",
    queue = "reward_option_user_dooot",
    flush_ms = 5000
)]
pub struct RewardOptionUserDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,

    /// The distributor
    #[topic]
    pub distributor: String,

    /// The index of the distributor
    #[ts(type = "number")]
    pub idx: u16,

    /// The writer
    #[topic]
    pub writer: String,

    /// The owner
    #[topic]
    pub owner: String,

    /// The user exercising the reward option
    #[topic]
    pub user: String,

    /// Strike price of the call option (in smallest units of the price mint)
    #[ts(type = "number")]
    pub strike_price: u64,

    /// Expiry datetime of the call option
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub expires_on: NaiveDateTime,

    /// The location of the data
    pub data_location: String,

    /// The reward mint pubkey
    #[topic]
    pub reward_mint: String,

    /// The price mint pubkey
    #[topic]
    pub price_mint: String,

    // The total amount written
    #[ts(type = "number")]
    pub written_amount: u64,

    // The total amount claimed
    #[ts(type = "number")]
    pub claimed_amount: u64,

    // The total amount paid
    #[ts(type = "number")]
    pub paid_amount: u64,
}
