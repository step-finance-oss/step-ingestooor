use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "SubscriptionUser")]
#[clickhouse(
    table = "incoming_subscription_user_dooot",
    queue = "subscription_user_dooot",
    flush_ms = 2000
)]
pub struct SubscriptionUserDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub regarding_pubkey: String,
    #[topic]
    pub user_pubkey: String,
    pub payment_mint: String,
    pub total_paid: u64,
    pub name: String,
    #[topic]
    pub code: String,
    pub description: String,
    pub category: String,
    pub quantity_purchased: u64,
    pub expires_ts: i64,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "SubscriptionPurchaseEvent")]
#[clickhouse(
    table = "incoming_subscription_purchase_event_dooot",
    queue = "subscription_purchase_event_dooot",
    flush_ms = 2000
)]
pub struct SubscriptionPurchaseEventDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub tx_id: String,
    #[topic]
    pub regarding_pubkey: String,
    #[topic]
    pub user_pubkey: String,
    pub payment_mint: String,
    pub amount_paid: u64,
    pub name: String,
    #[topic]
    pub code: String,
    pub description: String,
    pub category: String,
    pub quantity: u64,
    pub expires_ts: i64,
}
