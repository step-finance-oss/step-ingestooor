use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use step_common_rust::evalexpr::ValueWrapper;
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use strum_macros::Display;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "NativeStakeEvent")]
#[clickhouse(
    table = "incoming_native_stake_event_dooot",
    queue = "native_stake_event_dooot",
    flush_ms = 5000
)]
pub struct NativeStakeEventDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub tx_id: String,
    #[topic]
    pub event_type: NativeStakeEventType,
    /// This field is exposed as a topic
    #[topic]
    pub stake_account_pubkey: String,
    /// This field is exposed as a topic
    #[topic]
    #[ts(optional)]
    pub withdraw_auth_pubkey: Option<String>,
    /// This field is exposed as a topic
    #[topic]
    #[ts(optional)]
    pub stake_auth_pubkey: Option<String>,
    /// u64
    #[ts(type = "number")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_u64"
    )]
    pub amount: Decimal,
}

#[repr(u8)]
#[derive(
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
    Display,
    Debug,
    Clone,
    Default,
    TS,
    PartialEq,
    Eq,
)]
pub enum NativeStakeEventType {
    #[default]
    Delegate,
    Deactivate,
    Withdraw,
}

impl From<NativeStakeEventType> for ValueWrapper {
    fn from(item: NativeStakeEventType) -> Self {
        item.to_string().into()
    }
}
