use anchor_lang::declare_program;
use anyhow::bail;

declare_program!(meteora_amm);

// making a custom here because meteora has properties on Stable
pub enum CurveType {
    ConstantProduct,
    Stable,
}

impl TryFrom<u8> for CurveType {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(CurveType::ConstantProduct),
            1 => Ok(CurveType::Stable),
            _ => bail!("Invalid curve type: {}", value),
        }
    }
}

impl From<CurveType> for step_ingestooor_sdk::dooot::CurveType {
    fn from(value: CurveType) -> Self {
        match value {
            CurveType::ConstantProduct => step_ingestooor_sdk::dooot::CurveType::ConstantProduct,
            CurveType::Stable => step_ingestooor_sdk::dooot::CurveType::Stable,
        }
    }
}
