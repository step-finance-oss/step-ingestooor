use anchor_lang::declare_program;
use fixed::types::U68F60;
use kamino_lend::{
    accounts::Reserve,
    types::{CurvePoint, ReserveCollateral, ReserveLiquidity},
};
use rust_decimal::Decimal;

declare_program!(kamino_lend);

impl Reserve {
    pub fn collateral_exchange_rate(&self) -> U68F60 {
        let total_liquidity = self.liquidity.total_supply();
        self.collateral.exchange_rate(total_liquidity)
    }

    /// Token amt -> Acct state value
    pub fn liquidity_to_collateral(&self, val: u64) -> Decimal {
        let ex = self.collateral_exchange_rate();
        Decimal::from((ex * u128::from(val)).floor().to_num::<u64>())
    }

    /// Acct state value -> Token amt
    pub fn collateral_to_liquidity(&self, val: U68F60) -> Decimal {
        let ex = self.collateral_exchange_rate();
        Decimal::from((val / ex).floor().to_num::<u64>())
    }

    /// (supply_apr, borrow_apr)
    pub fn get_rates(&self) -> (f64, f64) {
        let current_utilization = self.utilization_ratio();
        let borrow_rate = self.get_borrow_rate(current_utilization);
        let protocol_take_rate_pct =
            U68F60::ONE - (U68F60::from(self.config.protocol_take_rate_pct) / 100);
        let supply_apr =
            (current_utilization * borrow_rate * protocol_take_rate_pct).to_num::<f64>();

        let host_interest_rate = U68F60::from(self.config.host_fixed_interest_rate_bps) / 10_000;
        let borrow_apr = (borrow_rate + host_interest_rate).to_num::<f64>();

        (supply_apr, borrow_apr)
    }

    // Guarnteed by program
    #[allow(clippy::unwrap_used)]
    pub fn get_borrow_rate(&self, utilization_rate: U68F60) -> U68F60 {
        let utilization_rate = utilization_rate.min(U68F60::ONE);

        let utilization_rate_bps = (utilization_rate * 10_000).to_num::<u32>();

        let (start_pt, end_pt) = self
            .config
            .borrow_rate_curve
            .points
            .windows(2)
            .map(|seg| {
                let [first, second]: &[CurvePoint; 2] = seg.try_into().unwrap();
                (first, second)
            })
            .find(|(first, second)| {
                utilization_rate_bps >= first.utilization_rate_bps
                    && utilization_rate_bps <= second.utilization_rate_bps
            })
            .unwrap();

        let offset = U68F60::from(start_pt.borrow_rate_bps) / 10_000;

        if utilization_rate_bps == start_pt.utilization_rate_bps {
            return offset;
        } else if utilization_rate_bps == end_pt.utilization_rate_bps {
            return U68F60::from(end_pt.borrow_rate_bps) / 10_000;
        }

        let start_ur = U68F60::from(start_pt.utilization_rate_bps) / 10_000;

        let coef = utilization_rate - start_ur;
        let slope_nom = end_pt.borrow_rate_bps - start_pt.borrow_rate_bps;
        let nom = coef * u128::from(slope_nom);
        let slope_denom = end_pt.utilization_rate_bps - start_pt.utilization_rate_bps;
        let base_rate = nom / u128::from(slope_denom);

        base_rate + offset
    }

    fn utilization_ratio(&self) -> U68F60 {
        let total_deposits = self.liquidity.total_supply();
        if total_deposits == U68F60::ZERO {
            return U68F60::ZERO;
        }

        let total_borrows = self.liquidity.total_borrows();

        total_borrows / total_deposits
    }
}

impl ReserveLiquidity {
    pub fn total_supply(&self) -> U68F60 {
        U68F60::from(self.available_amount) + U68F60::from_bits(self.borrowed_amount_sf)
            - U68F60::from_bits(self.accumulated_protocol_fees_sf)
            - U68F60::from_bits(self.accumulated_referrer_fees_sf)
            - U68F60::from_bits(self.pending_referrer_fees_sf)
    }

    pub fn total_borrows(&self) -> U68F60 {
        U68F60::from_bits(self.borrowed_amount_sf)
    }
}

impl ReserveCollateral {
    pub fn exchange_rate(&self, total_liquidity: U68F60) -> U68F60 {
        if self.mint_total_supply == 0 || total_liquidity == U68F60::ZERO {
            U68F60::ONE
        } else {
            U68F60::from(self.mint_total_supply) / total_liquidity
        }
    }
}
