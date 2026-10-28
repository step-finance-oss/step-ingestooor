use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

/// This dooot representschanges to a mints underlying assets.  This could
/// be a LP token, a yield bearing token, or other derivatives supported.
/// The topic prefix is `MintUnderlyings`.
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "MintUnderlyingsGlobal")]
#[clickhouse(
    table = "incoming_mint_underlyings_global_dooot",
    queue = "mint_underlyings_global_dooot",
    flush_ms = 5_000,
    //eh, this proc is touchy rn.  SF-8043 will resolve ideally
    //which is hopefully done before this comment is read.
    //but this number might need adjusted down to save RAM in CH
    inserter_max_rows = 5_000
)]
pub struct MintUnderlyingsGlobalDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    /// This field is exposed as a topic
    #[topic]
    pub mint_pubkey: String,
    /// This field is exposed as a topic
    #[topic]
    pub platform_program_pubkey: String,
    pub mints: Vec<String>,
    #[ts(type = "Array<number>")]
    pub mints_qty_per_one_parent: Vec<f64>,
    /// u64s
    #[ts(type = "Array<number>")]
    #[serde(
        serialize_with = "crate::serde::ch_decimal::serialize_ch_decimal_vec_u64",
        deserialize_with = "crate::serde::ch_decimal::deserialize_ch_decimal_vec_u64"
    )]
    pub total_underlying_amounts: Vec<Decimal>,
    /// What discriminates this datapoint from others,
    /// i.e., two SOL/USDC pools on the same program id, in this case use pool_id
    pub discriminant_id: String,
}
