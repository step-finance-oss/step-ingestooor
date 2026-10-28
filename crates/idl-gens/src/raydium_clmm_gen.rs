use anchor_lang::declare_program;
use raydium_clmm::types::TickState;
use step_ingestooor_sdk::dooot::ClmmTick;

declare_program!(raydium_clmm);

impl From<TickState> for ClmmTick {
    fn from(value: TickState) -> Self {
        Self {
            liquidity_gross: value.liquidity_gross.to_string(),
            liquidity_net: value.liquidity_net.to_string(),
            fee_growth_outside_a: value.fee_growth_outside0_x64.to_string(),
            fee_growth_outside_b: value.fee_growth_outside1_x64.to_string(),
        }
    }
}
