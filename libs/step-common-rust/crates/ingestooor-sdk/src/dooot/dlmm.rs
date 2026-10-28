use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use step_common_rust::evalexpr::ValueWrapper;
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "DlmmGlobal")]
#[clickhouse(
    table = "incoming_dlmm_global_dooot",
    queue = "dlmm_global_dooot",
    flush_ms = 5000
)]
pub struct DlmmGlobalDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub pool_pubkey: String,
    pub parts_account_pubkey: String,
    pub part_index: i32,
    pub parts: Vec<DLMMPart>,
    pub closed: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, TS)]
pub struct DLMMPart {
    /// u128
    #[ts(type = "number")]
    pub shares: String,
    /// vec of u64
    #[ts(type = "number[]")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_vec_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_vec_u64"
    )]
    pub token_amounts: Vec<Decimal>,
    /// vec of u64
    #[ts(type = "number[]")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_vec_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_vec_u64"
    )]
    pub fee_amounts: Vec<Decimal>,
    /// u128
    #[ts(type = "number")]
    pub price: String,
}

#[allow(clippy::unwrap_used)]
impl From<DLMMPart> for ValueWrapper {
    fn from(value: DLMMPart) -> Self {
        // This is only called when ref'd in an expression in broadcaster
        let shares_parsed = value.shares.to_string().parse::<f64>().unwrap();
        let token_amounts_parsed = value
            .token_amounts
            .iter()
            .map(|x| evalexpr::Value::Float(x.to_string().parse::<f64>().unwrap()))
            .collect::<Vec<_>>();
        let fee_amounts_parsed = value
            .fee_amounts
            .iter()
            .map(|x| evalexpr::Value::Float(x.to_string().parse::<f64>().unwrap()))
            .collect::<Vec<_>>();
        let price_parsed = value.price.to_string().parse::<f64>().unwrap();

        Self::new(evalexpr::Value::Tuple(vec![
            evalexpr::Value::Float(shares_parsed),
            evalexpr::Value::Tuple(token_amounts_parsed),
            evalexpr::Value::Tuple(fee_amounts_parsed),
            evalexpr::Value::Float(price_parsed),
        ]))
    }
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "DlmmUser")]
#[clickhouse(
    table = "incoming_dlmm_user_dooot",
    queue = "dlmm_user_dooot",
    flush_ms = 2000
)]
pub struct DlmmUserDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub lower_part_account_pubkey: String,
    #[topic]
    pub upper_part_account_pubkey: String,
    #[topic]
    pub user_pubkey: String,
    /// u128s
    #[ts(type = "number[]")]
    pub shares: Vec<String>,
    #[ts(optional)]
    pub position_mint_pubkey: Option<String>,
    /// u64s
    #[ts(type = "number[]")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_vec_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_vec_u64"
    )]
    pub fees_claimed: Vec<Decimal>,
    pub lower_inner_id: i32,
    pub upper_inner_id: i32,
    pub closed: bool,
}
