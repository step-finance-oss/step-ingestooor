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
#[dooot(name = "LiquidityChangeEvent")]
#[clickhouse(
    table = "incoming_liquidity_change_event_dooot",
    queue = "liquidity_change_event_dooot",
    flush_ms = 5000
)]
pub struct LiquidityChangeEventDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    pub owner: String,
    pub pool: String,
    pub mints: Vec<String>,
    /// i128s
    #[ts(type = "number[]")]
    pub amounts: Vec<Decimal>,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "LPInfo")]
#[clickhouse(
    table = "incoming_lookup_lp_info_dooot",
    queue = "lookup_lp_info_dooot",
    flush_ms = 5000
)]
pub struct LPInfoDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    pub pool: String,
    pub program: String,
    //can be None on pools that dont share a common mint (ex. clmm)
    #[ts(optional)]
    pub lp_mint: Option<String>,
    pub underlyings: Vec<LPInfoUnderlyingMintVault>,
    #[ts(type = "number")]
    pub curve_type: CurveType,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, TS)]
pub struct LPInfoUnderlyingMintVault {
    pub mint: String,
    pub vault: String,
}

impl From<LPInfoUnderlyingMintVault> for ValueWrapper {
    fn from(underlying: LPInfoUnderlyingMintVault) -> Self {
        format!("{}:{}", underlying.mint, underlying.vault).into()
    }
}

#[derive(
    serde_repr::Serialize_repr, serde_repr::Deserialize_repr, Debug, Clone, Default, TS, PartialEq,
)]
#[repr(u16)]
pub enum CurveType {
    ConstantProduct,
    ConstantPrice,
    Stable,
    ConstantProductWithOffset,
    ConcentratedLiquidity, //aka CLMM; injecting into the "4" spot for backward compat
    StableWithOffset,
    #[default]
    Unknown,
    DynamicLiquidity,
}

//for expressing curve type in an expression
impl From<CurveType> for ValueWrapper {
    fn from(curve_type: CurveType) -> Self {
        match curve_type {
            CurveType::ConstantProduct => "ConstantProduct".to_string().into(),
            CurveType::ConstantPrice => "ConstantPrice".to_string().into(),
            CurveType::Stable => "Stable".to_string().into(),
            CurveType::ConstantProductWithOffset => "ConstantProductWithOffset".to_string().into(),
            CurveType::ConcentratedLiquidity => "ConcentratedLiquidity".to_string().into(),
            CurveType::StableWithOffset => "StableWithOffset".to_string().into(),
            CurveType::Unknown => "Unknown".to_string().into(),
            CurveType::DynamicLiquidity => "DynamicLiquidity".to_string().into(),
        }
    }
}
