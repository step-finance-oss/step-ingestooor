use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use step_common_rust::evalexpr::ValueWrapper;
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

/// This dooot represents a swap. Swaps can be nested, so the parent_amm_program_pubkey
/// is used to track the parent swap.
/// The topic prefix is `Swap`.
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "SwapEvent")]
#[clickhouse(
    table = "incoming_swap_event_dooot",
    queue = "swap_event_dooot",
    flush_ms = 5000
)]
pub struct SwapEventDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    /// This field is exposed as a topic
    #[topic]
    pub program_pubkey: String,
    /// This field is exposed as a topic
    #[topic]
    pub swap_auth_pubkey: String,
    /// This field is exposed as a topic
    #[topic]
    pub in_mint_pubkey: String,
    /// This field is exposed as a topic
    #[topic]
    pub out_mint_pubkey: String,
    /// u64
    #[ts(type = "number")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64"
    )]
    pub in_amount: Decimal,
    /// u64
    #[ts(type = "number")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64"
    )]
    pub out_amount: Decimal,
    #[ts(optional)]
    pub fee_mint_is_in_mint: Option<bool>,
    /// This field is exposed as a topic
    #[topic]
    #[ts(optional)]
    pub referring_platform_account_pubkey: Option<String>,
    /// u64
    #[ts(type = "number", optional)]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64_opt",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64_opt"
    )]
    pub platform_referral_fee: Option<Decimal>,
    #[ts(optional)]
    pub parent_amm_program_pubkey: Option<String>,
    /// This field is exposed as a topic
    #[topic]
    #[ts(optional)]
    pub market_pubkey: Option<String>,
    pub fees: Vec<SwapFees>,
    pub slippage_bps: Option<u16>,
    pub tx_sig: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, TS)]
pub struct SwapFees {
    pub mint: String,
    #[ts(type = "number")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64"
    )]
    pub amount: Decimal,
}

#[allow(clippy::unwrap_used)]
impl From<SwapFees> for ValueWrapper {
    fn from(fees: SwapFees) -> Self {
        let m = ValueWrapper::from(fees.mint);
        let d = ValueWrapper::from(fees.amount);
        ValueWrapper::from(vec![m, d])
    }
}
