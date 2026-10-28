use defituna::types::{MarketMaker, TunaPositionState};
anchor_lang::declare_program!(defituna);

use std::fmt::Display;

impl Display for MarketMaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarketMaker::Orca => write!(f, "Orca"),
            MarketMaker::Fusion => write!(f, "Fusion"),
        }
    }
}

impl Display for TunaPositionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TunaPositionState::Normal => write!(f, "Normal"),
            TunaPositionState::Liquidated => write!(f, "Liquidated"),
            TunaPositionState::ClosedByLimitOrder => write!(f, "ClosedByLimitOrder"),
        }
    }
}

impl From<MarketMaker> for step_ingestooor_sdk::dooot::defi_tuna::MarketMaker {
    fn from(value: MarketMaker) -> Self {
        match value {
            MarketMaker::Orca => step_ingestooor_sdk::dooot::defi_tuna::MarketMaker::Orca,
            MarketMaker::Fusion => step_ingestooor_sdk::dooot::defi_tuna::MarketMaker::Fusion,
        }
    }
}

impl From<TunaPositionState> for step_ingestooor_sdk::dooot::defi_tuna::TunaPositionState {
    fn from(value: TunaPositionState) -> Self {
        match value {
            TunaPositionState::Normal => {
                step_ingestooor_sdk::dooot::defi_tuna::TunaPositionState::Normal
            }
            TunaPositionState::Liquidated => {
                step_ingestooor_sdk::dooot::defi_tuna::TunaPositionState::Liquidated
            }
            TunaPositionState::ClosedByLimitOrder => {
                step_ingestooor_sdk::dooot::defi_tuna::TunaPositionState::ClosedByLimitOrder
            }
        }
    }
}
