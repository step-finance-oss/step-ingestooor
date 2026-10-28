use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_common_rust::evalexpr::ValueWrapper;
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "DefiTunaFusionPool")]
#[clickhouse(
    table = "incoming_ps_defi_tuna_fusion_pool_dooot",
    queue = "ps_defi_tuna_fusion_pool_dooot",
    flush_ms = 5000
)]
pub struct PsDefiTunaFusionPoolDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    pub account_pubkey: String,
    pub bump: [u8; 1],
    pub version: u16,
    pub token_mint_a: String,
    pub token_mint_b: String,
    pub token_vault_a: String,
    pub token_vault_b: String,
    pub tick_spacing: u16,
    pub tick_spacing_seed: [u8; 2],
    pub fee_rate: u16,
    pub protocol_fee_rate: u16,
    pub clp_to_olp_reward_ratio: u16,
    pub order_protocol_fee_rate: u16,
    pub liquidity: String,
    pub sqrt_price: String,
    pub tick_current_index: i32,
    pub protocol_fee_owed_a: u64,
    pub protocol_fee_owed_b: u64,
    pub fee_growth_global_a: String,
    pub fee_growth_global_b: String,
    pub orders_total_amount_a: u64,
    pub orders_total_amount_b: u64,
    pub orders_filled_amount_a: u64,
    pub orders_filled_amount_b: u64,
    pub olp_fee_owed_a: u64,
    pub olp_fee_owed_b: u64,
    pub closed: bool,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "DefiTunaLendingPosition")]
#[clickhouse(
    table = "incoming_ps_defi_tuna_lending_position_dooot",
    queue = "ps_defi_tuna_lending_position_dooot",
    flush_ms = 5000
)]
pub struct PsDefiTunaLendingPositionDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    pub account_pubkey: String,
    pub version: u16,
    pub bump: [u8; 1],
    pub authority: String,
    pub pool_mint: String,
    pub deposited_funds: u64,
    pub deposited_shares: u64,
    pub closed: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub enum MarketMaker {
    #[default]
    Orca,
    Fusion,
}

impl From<MarketMaker> for ValueWrapper {
    fn from(value: MarketMaker) -> Self {
        match value {
            MarketMaker::Orca => ValueWrapper::new(evalexpr::Value::String("Orca".to_string())),
            MarketMaker::Fusion => ValueWrapper::new(evalexpr::Value::String("Fusion".to_string())),
        }
    }
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "DefiTunaMarket")]
#[clickhouse(
    table = "incoming_ps_defi_tuna_market_dooot",
    queue = "ps_defi_tuna_market_dooot",
    flush_ms = 5000
)]
pub struct PsDefiTunaMarketDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    pub account_pubkey: String,
    pub version: u16,
    pub bump: [u8; 1],
    #[ts(type = "string")]
    pub market_maker: String,
    pub pool: String,
    pub address_lookup_table: String,
    pub max_leverage: u32,
    pub protocol_fee: u16,
    pub protocol_fee_on_collateral: u16,
    pub liquidation_fee: u32,
    pub liquidation_threshold: u32,
    pub limit_order_execution_fee: u32,
    pub oracle_price_deviation_threshold: u32,
    pub disabled: bool,
    pub borrowed_shares_a: u64,
    pub borrowed_shares_b: u64,
    pub borrow_limit_a: u64,
    pub borrow_limit_b: u64,
    pub max_swap_slippage: u32,
    pub closed: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub enum TunaPositionState {
    #[default]
    Normal,
    Liquidated,
    ClosedByLimitOrder,
}

impl From<TunaPositionState> for ValueWrapper {
    fn from(value: TunaPositionState) -> Self {
        match value {
            TunaPositionState::Normal => {
                ValueWrapper::new(evalexpr::Value::String("Normal".to_string()))
            }
            TunaPositionState::Liquidated => {
                ValueWrapper::new(evalexpr::Value::String("Liquidated".to_string()))
            }
            TunaPositionState::ClosedByLimitOrder => {
                ValueWrapper::new(evalexpr::Value::String("ClosedByLimitOrder".to_string()))
            }
        }
    }
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "DefiTunaTunaPosition")]
#[clickhouse(
    table = "incoming_ps_defi_tuna_tuna_position_dooot",
    queue = "ps_defi_tuna_tuna_position_dooot",
    flush_ms = 5000
)]
pub struct PsDefiTunaTunaPositionDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    pub account_pubkey: String,
    pub version: u16,
    pub bump: [u8; 1],
    pub authority: String,
    pub pool: String,
    pub mint_a: String,
    pub mint_b: String,
    pub position_mint: String,
    pub liquidity: String,
    pub tick_lower_index: i32,
    pub tick_upper_index: i32,
    pub loan_shares_a: u64,
    pub loan_shares_b: u64,
    pub loan_funds_a: u64,
    pub loan_funds_b: u64,
    pub leftovers_a: u64,
    pub leftovers_b: u64,
    pub tick_entry_index: i32,
    pub tick_stop_loss_index: i32,
    pub tick_take_profit_index: i32,
    #[ts(type = "string")]
    pub state: String,
    pub swap_to_token_on_limit_order: u8,
    pub compounded_yield_a: u64,
    pub compounded_yield_b: u64,
    pub flags: u32,
    #[ts(type = "string")]
    pub market_maker: String,
    pub closed: bool,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "DefiTunaVault")]
#[clickhouse(
    table = "incoming_ps_defi_tuna_vault_dooot",
    queue = "ps_defi_tuna_vault_dooot",
    flush_ms = 5000
)]
pub struct PsDefiTunaVaultDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    pub account_pubkey: String,
    pub version: u16,
    pub bump: [u8; 1],
    pub mint: String,
    pub deposited_funds: u64,
    pub deposited_shares: u64,
    pub borrowed_funds: u64,
    pub borrowed_shares: u64,
    pub unpaid_debt_shares: u64,
    pub interest_rate: u64,
    pub last_update_timestamp: u64,
    pub supply_limit: u64,
    pub pyth_oracle_price_update: String,
    pub pyth_oracle_feed_id: String,
    pub closed: bool,
}
