use anchor_lang::declare_program;
use orca_whirlies::types::Tick;
use step_ingestooor_sdk::dooot::ClmmTick;

declare_program!(orca_whirlies);

impl From<Tick> for ClmmTick {
    fn from(value: Tick) -> Self {
        Self {
            fee_growth_outside_a: value.fee_growth_outside_a.to_string(),
            fee_growth_outside_b: value.fee_growth_outside_b.to_string(),
            liquidity_gross: value.liquidity_gross.to_string(),
            liquidity_net: value.liquidity_net.to_string(),
        }
    }
}
