use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_common_rust::evalexpr::ValueWrapper;
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use strum_macros::Display;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[clickhouse(
    table = "incoming_ps_nx_lend_vsol_pool_dooot",
    queue = "ps_nx_lend_vsol_pool_dooot",
    flush_ms = 2000
)]
#[dooot(name = "NxLendVSolPool")]
pub struct PsNxLendVSolPoolDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub account_pubkey: String,
    #[topic]
    pub nx_market: String,
    pub vsol_leverage_tokens: u64,
    pub vsol_leverage_notes: u64,
    pub vsol_points: u64,
    /// rate = (vsol_points_new - vsol_points_old)/vsol_leverage_notes
    pub last_vsol_point_per_leverage_note: u64,
    pub vsol_points_fee_claimable: u64,
    pub last_vsol_point_update_time: i64,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[clickhouse(
    table = "incoming_ps_nx_lend_fragmetric_user_dooot",
    queue = "ps_nx_lend_fragmetric_user_dooot",
    flush_ms = 2000
)]
#[dooot(name = "NxLendFragmetricUser")]
pub struct PsNxLendFragmetricUserDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub account_pubkey: String,
    #[topic]
    pub nx_market: String,
    pub owner: String,
    pub receipt_token: String,
    pub amount: u64,
    pub nx_fragmetric_points: u64,
    pub last_update_time: i64,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[clickhouse(
    table = "incoming_ps_nx_lend_fragmetric_pool_dooot",
    queue = "ps_nx_lend_fragmetric_pool_dooot",
    flush_ms = 2000
)]
#[dooot(name = "NxLendFragmetricPool")]
pub struct PsNxLendFragmetricPoolDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub account_pubkey: String,
    #[topic]
    pub nx_market: String,
    pub receipt_token: String,
    pub amount: u64,
    pub total_nx_fragmetric_points: u64,
    pub last_update_time: i64,
    pub padding: [u64; 10],
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[clickhouse(
    table = "incoming_ps_nx_lend_lending_pool_dooot",
    queue = "ps_nx_lend_lending_pool_dooot",
    flush_ms = 2000
)]
#[dooot(name = "NxLendLendingPool")]
pub struct PsNxLendLendingPoolDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub account_pubkey: String,
    #[topic]
    pub nx_market: String,
    #[topic]
    pub token_mint: String,
    pub borrow_tokens: u64,
    pub borrow_notes: u64,
    pub deposit_tokens: u64,
    pub deposit_notes: u64,
    pub deposit_interest: u64,
    pub borrow_interest: u64,
    pub protocol_fee: u64,
    pub accrued_until: i64,
    pub utilization_flag: u16,
    pub interest_rate_configs: Vec<InterestRateData>,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[clickhouse(
    table = "incoming_ps_nx_lend_lending_account_dooot",
    queue = "ps_nx_lend_lending_account_dooot",
    flush_ms = 2000
)]
#[dooot(name = "NxLendLendingAccount")]
pub struct PsNxLendLendingAccountDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub account_pubkey: String,
    #[topic]
    pub nx_market: String,
    #[topic]
    pub owner: String,
    pub deposit_notes: u64,
    pub deposit_tokens: u64,
    pub last_note_rate: u64,
    pub total_reward: u64,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[clickhouse(
    table = "incoming_ps_nx_lend_solayer_pool_dooot",
    queue = "ps_nx_lend_solayer_pool_dooot",
    flush_ms = 2000
)]
#[dooot(name = "NxLendSolayerPool")]
pub struct PsNxLendSolayerPoolDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub account_pubkey: String,
    #[topic]
    pub nx_market: String,
    #[topic]
    pub lrt_mint: String,
    pub amount: u64,
    pub total_nx_solayer_points: u64,
    pub last_update_time: i64,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[clickhouse(
    table = "incoming_ps_nx_leverage_jupiter_account_dooot",
    queue = "ps_nx_leverage_jupiter_account_dooot",
    flush_ms = 2000
)]
#[dooot(name = "NxLeverageJupiterAccount")]
pub struct PsNxLeverageJupiterAccountDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub account_pubkey: String,
    #[topic]
    pub leveragefi: String,
    #[topic]
    pub admin: String,
    #[topic]
    pub jlp_mint: String,
    #[topic]
    pub jlp_account: String,
    pub jupiter_notes: u64,
    pub jlp_position: u64,
}

#[derive(
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
    Debug,
    Clone,
    Default,
    TS,
    PartialEq,
    Display,
)]
#[repr(u16)]
pub enum PositionType {
    Fragmetric,
    Solayer,
    #[default]
    Normal,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[clickhouse(
    table = "incoming_ps_nx_lend_position_dooot",
    queue = "ps_nx_lend_position_dooot",
    flush_ms = 2000
)]
#[dooot(name = "NxLendPosition")]
pub struct PsNxLendPositionDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub account_pubkey: String,
    #[topic]
    pub collateral_mint: String,
    #[topic]
    pub borrow_mint: String,
    #[topic]
    pub leverage_mint: String,
    pub collateral_note: u64,
    pub collateral_tokens: u64,
    pub borrow_note: u64,
    pub borrow_tokens: u64,
    pub leverage_note: u64,
    pub leverage_tokens: u64,
    pub liquidation_flag: u64,
    pub leverage_multiples: u64,
    pub position_type: String,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[clickhouse(
    table = "incoming_ps_nx_leverage_margin_pool_dooot",
    queue = "ps_nx_leverage_margin_pool_dooot",
    flush_ms = 2000
)]
#[dooot(name = "NxLeverageMarginPool")]
pub struct PsNxLeverageMarginPoolDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub account_pubkey: String,
    #[topic]
    pub leveragefi: String,
    pub vault: String,
    pub fee_destination: String,
    pub pool_authority: String,
    #[topic]
    pub token_mint: String,
    pub token_price_oracle: String,
    pub borrowed_tokens: u64,
    pub deposit_tokens: u64,
    pub deposit_notes: u64,
    pub loan_notes: u64,
    pub deposit_interest: u64,
    pub loan_interest: u64,
    pub protocol_fee: u64,
    pub accrued_until: i64,
    pub utilization_flag: u16,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[clickhouse(
    table = "incoming_ps_nx_leverage_margin_account_dooot",
    queue = "ps_nx_leverage_margin_account_dooot",
    flush_ms = 2000
)]
#[dooot(name = "NxLeverageMarginAccount")]
pub struct PsNxLeverageMarginAccountDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub account_pubkey: String,
    pub leveragefi: String,
    pub owner: String,
    pub deposits: Vec<CollateralDetail>,
    pub loans: Vec<LoanDetail>,
    pub leverage: u64,
    pub jlp_notes: u64,
    pub active_loan: String,
    pub active_collateral: String,
    pub swap_temp: SwapTemp,
    pub account_type: u8,
    pub liquidation_flag: u8,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, TS, ClickhouseDooot, Dooot, clickhouse::Row,
)]
#[clickhouse(
    table = "incoming_ps_nx_lend_vsol_position_dooot",
    queue = "ps_nx_lend_vsol_position_dooot",
    flush_ms = 2000
)]
#[dooot(name = "NxLendVSolPosition")]
pub struct PsNxLendVSolPositionDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    #[topic]
    pub account_pubkey: String,
    pub nx_market: String,
    pub owner: String,
    pub positions: Vec<VSolPositionDetail>,
    pub swap_temp: SwapTemp,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, TS)]
pub struct CollateralDetail {
    pub token_mint: String,
    pub deposit_note: u64,
    pub deposit_token: u64,
    pub market_value: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, TS)]
pub struct LoanDetail {
    pub token_mint: String,
    pub loan_note: u64,
    pub loan_token: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, TS)]
pub struct SwapTemp {
    pub before_amount: u64,
    pub expected: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, TS)]
pub struct InterestRateData {
    pub utilization_rate: i32,
    pub k_value: i32,
    pub b_value: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, TS)]
pub struct VSolPositionDetail {
    pub collateral_mint: String,
    pub borrow_mint: String,
    pub leverage_mint: String,
    pub collateral_note: u64,
    pub collateral_tokens: u64,
    pub borrow_note: u64,
    pub borrow_tokens: u64,
    pub leverage_note: u64,
    pub leverage_tokens: u64,
    pub leverage_multiples: u64,
    pub last_points_and_leverage_notes_rate: u64,
    pub point_reward: u64,
}
impl From<CollateralDetail> for ValueWrapper {
    fn from(value: CollateralDetail) -> Self {
        format!(
            "{}:{}:{}:{}",
            value.token_mint, value.deposit_note, value.deposit_token, value.market_value
        )
        .into()
    }
}
impl From<LoanDetail> for ValueWrapper {
    fn from(value: LoanDetail) -> Self {
        format!(
            "{}:{}:{}",
            value.token_mint, value.loan_note, value.loan_token
        )
        .into()
    }
}

impl From<SwapTemp> for ValueWrapper {
    fn from(value: SwapTemp) -> Self {
        format!("{}:{}", value.before_amount, value.expected).into()
    }
}
impl From<VSolPositionDetail> for ValueWrapper {
    fn from(value: VSolPositionDetail) -> Self {
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}",
            value.collateral_mint,
            value.borrow_mint,
            value.leverage_mint,
            value.collateral_note,
            value.collateral_tokens,
            value.borrow_note,
            value.borrow_tokens,
            value.leverage_note,
            value.leverage_tokens,
            value.leverage_multiples,
            value.last_points_and_leverage_notes_rate,
            value.point_reward,
        )
        .into()
    }
}

impl From<InterestRateData> for ValueWrapper {
    fn from(value: InterestRateData) -> Self {
        format!(
            "{}:{}:{}",
            value.utilization_rate, value.k_value, value.b_value
        )
        .into()
    }
}

impl From<PositionType> for ValueWrapper {
    fn from(value: PositionType) -> Self {
        match value {
            PositionType::Fragmetric => {
                ValueWrapper::new(evalexpr::Value::String("Fragmetric".to_string()))
            }
            PositionType::Solayer => {
                ValueWrapper::new(evalexpr::Value::String("Solayer".to_string()))
            }
            PositionType::Normal => {
                ValueWrapper::new(evalexpr::Value::String("Normal".to_string()))
            }
        }
    }
}
