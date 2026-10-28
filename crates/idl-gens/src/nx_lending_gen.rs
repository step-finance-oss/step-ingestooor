use nx_lending::types::PositionType;
anchor_lang::declare_program!(nx_lending);

use std::fmt::Display;

impl Display for PositionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PositionType::Fragmetric => write!(f, "Fragmetric"),
            PositionType::Solayer => write!(f, "Solayer"),
            PositionType::Normal => write!(f, "Normal"),
        }
    }
}
impl From<PositionType> for step_ingestooor_sdk::dooot::nx::PositionType {
    fn from(value: PositionType) -> Self {
        match value {
            PositionType::Fragmetric => step_ingestooor_sdk::dooot::nx::PositionType::Fragmetric,
            PositionType::Solayer => step_ingestooor_sdk::dooot::nx::PositionType::Solayer,
            PositionType::Normal => step_ingestooor_sdk::dooot::nx::PositionType::Normal,
        }
    }
}
