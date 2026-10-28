use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

/// This dooot represents a Solana transfer.
/// The topic prefix is `SolTransfer`.
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "SolTransferEvent")]
#[clickhouse(
    table = "incoming_sol_transfer_event_dooot",
    queue = "sol_transfer_event_dooot",
    flush_ms = 5000
)]
pub struct SolTransferEventDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    /// This field is exposed as a topic
    #[topic]
    pub source_pubkey: String,
    /// This field is exposed as a topic
    #[topic]
    pub destination_pubkey: String,
    pub amount: i64,
    #[ts(optional)]
    pub parent_program: Option<String>,
}

/// This dooot represents a Solana token transfer.
/// The topic prefix is `SplTransfer`.
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "TokenTransferEvent")]
#[clickhouse(
    table = "incoming_token_transfer_event_dooot",
    queue = "token_transfer_event_dooot",
    flush_ms = 5000
)]
pub struct TokenTransferEventDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    /// This field is exposed as a topic
    #[topic]
    #[ts(optional)]
    pub source_pubkey: Option<String>,
    /// This field is exposed as a topic
    #[topic]
    #[ts(optional)]
    pub destination_pubkey: Option<String>,
    /// This field is exposed as a topic
    #[topic]
    pub mint_pubkey: String,
    /// u64
    #[ts(type = "number")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64"
    )]
    pub amount: Decimal,
    #[ts(optional)]
    pub parent_program: Option<String>,
    pub spl_version: i16,
    pub tx_id: String,
}

/// This dooot represents a Solana token account balance change.
/// The topic prefix is `TokenBalanceUser`.
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "TokenBalanceUser")]
#[clickhouse(
    table = "incoming_token_balance_user_dooot",
    queue = "token_balance_user_dooot",
    flush_ms = 2000
)]
pub struct TokenBalanceUserDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    /// This field is exposed as a topic
    #[topic]
    pub owner_pubkey: String,
    /// This field is exposed as a topic
    #[topic]
    pub token_account_pubkey: String,
    /// This field is exposed as a topic
    #[topic]
    pub mint_pubkey: String,
    // u64
    #[ts(type = "number")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64"
    )]
    pub balance: Decimal,
    pub closed: bool,
}

/// This dooot represents a Solana mint supply change.
/// The topic prefix is `SplTokenSupply`.
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "MintSupplyGlobal")]
#[clickhouse(
    table = "incoming_mint_supply_global_dooot",
    queue = "mint_supply_global_dooot",
    flush_ms = 1000
)]
pub struct MintSupplyGlobalDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub block_time: NaiveDateTime,
    /// This field is exposed as a topic
    #[topic]
    pub mint: String,
    /// u64
    #[ts(type = "number")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64"
    )]
    pub total_supply: Decimal,
    pub spl_version: i16,
}
