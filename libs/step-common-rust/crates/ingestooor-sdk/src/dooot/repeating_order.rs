use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "RepeatingOrderUser")]
#[clickhouse(
    table = "incoming_repeating_order_user_dooot",
    queue = "repeating_order_user_dooot",
    flush_ms = 5000
)]
pub struct RepeatingOrderUserDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub program_pubkey: String,
    /// strategy type: 'dca' or 'va'
    #[topic]
    pub strategy_type: String,
    #[topic]
    pub user_pubkey: String,
    /// the unique acct id storing this position
    #[topic]
    pub position_pubkey: String,
    /// input token mint
    #[topic]
    pub input_mint_pubkey: String,
    /// input token storage
    pub input_account_pubkey: String,
    /// output token mint
    #[topic]
    pub output_mint_pubkey: String,
    /// output token storage
    pub output_account_pubkey: String,
    /// price token mint (for dca, this is in mint, for va, this is usdc)    
    pub sale_amount_mint_pubkey: String,
    /// sale amount per cycle
    #[ts(type = "number")]
    pub sale_amount_per_cycle: u64,
    /// minimum output amount per cycle
    #[ts(type = "number | null | undefined")]
    pub min_out_amount: Option<u64>,
    /// maximum output amount per cycle
    #[ts(type = "number | null | undefined")]
    pub max_out_amount: Option<u64>,
    /// Total amount of input tokens deposited
    #[ts(type = "number")]
    pub in_deposited: u64,
    /// Total amount of input tokens withdrawn
    #[ts(type = "number")]
    pub in_withdrawn: u64,
    /// Total amount of input tokens used for swaps
    #[ts(type = "number")]
    pub in_used: u64,
    /// Total output tokens received from swaps
    #[ts(type = "number")]
    pub out_received: u64,
    /// Total amount of output tokens withdrawn
    #[ts(type = "number")]
    pub out_withdrawn: u64,
    /// timestamp when position was created
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub created_at: NaiveDateTime,
    /// timestamp when next order will be executed
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time_opt",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time_opt"
    )]
    #[ts(optional)]
    pub next_order_at: Option<NaiveDateTime>,
    /// Time interval between orders
    #[ts(type = "number")]
    pub order_interval: u64,
    pub closed: bool,
}
