use std::str::FromStr;

use anchor_lang::declare_program;
use fixed::types::I80F48;
use marginfi::{
    accounts::Bank,
    types::{InterestRateConfig, WrappedI80F48},
};
use rust_decimal::Decimal;

declare_program!(marginfi);

impl From<WrappedI80F48> for I80F48 {
    fn from(wrapped: WrappedI80F48) -> I80F48 {
        I80F48::from_bits(wrapped.value)
    }
}

impl TryFrom<WrappedI80F48> for Decimal {
    type Error = rust_decimal::Error;

    fn try_from(wrapped: WrappedI80F48) -> Result<Decimal, Self::Error> {
        Decimal::from_str(&I80F48::from_bits(wrapped.value).to_string())
    }
}

// Ripped logic from https://github.com/mrgnlabs/marginfi-v2/blob/main/observability/indexer/src/utils/metrics.rs
impl Bank {
    pub fn get_utilization_rate(&self) -> Option<I80F48> {
        let asset_val: I80F48 = self.asset_share_value.into();
        let liab_val: I80F48 = self.liability_share_value.into();

        let asset_shares: I80F48 = self.total_asset_shares.into();
        let liab_shares: I80F48 = self.total_liability_shares.into();

        let total_asset_tokens = asset_val.checked_mul(asset_shares)?;
        let total_liab_tokens = liab_val.checked_mul(liab_shares)?;

        if total_asset_tokens.is_positive() {
            total_liab_tokens.checked_div(total_asset_tokens)
        } else {
            Some(I80F48::ZERO)
        }
    }

    /// Return (`lending_rate`, `borrowing_rate`, `group_fees_apr`, `insurance_fees_apr`)
    pub fn get_rates(&self) -> Option<(I80F48, I80F48, I80F48, I80F48)> {
        self.config
            .interest_rate_config
            .calc_interest_rate(self.get_utilization_rate()?)
    }
}

impl InterestRateConfig {
    /// Return interest rate charged to borrowers and to depositors.
    /// Rate is denominated in APR (0-).
    ///
    /// Return (`lending_rate`, `borrowing_rate`, `group_fees_apr`, `insurance_fees_apr`)
    fn calc_interest_rate(
        &self,
        utilization_ratio: I80F48,
    ) -> Option<(I80F48, I80F48, I80F48, I80F48)> {
        let protocol_ir_fee = I80F48::from(self.protocol_ir_fee);
        let insurance_ir_fee = I80F48::from(self.insurance_ir_fee);

        let protocol_fixed_fee_apr = I80F48::from(self.protocol_fixed_fee_apr);
        let insurance_fee_fixed_apr = I80F48::from(self.insurance_fee_fixed_apr);

        let rate_fee = protocol_ir_fee + insurance_ir_fee;
        let total_fixed_fee_apr = protocol_fixed_fee_apr + insurance_fee_fixed_apr;

        let base_rate = self.interest_rate_curve(utilization_ratio)?;

        // Lending rate is adjusted for utilization ratio to symmetrize payments between borrowers and depositors.
        let lending_rate = base_rate.checked_mul(utilization_ratio)?;

        // Borrowing rate is adjusted for fees.
        // borrowing_rate = base_rate + base_rate * rate_fee + total_fixed_fee_apr
        let borrowing_rate = base_rate
            .checked_mul(I80F48::ONE.checked_add(rate_fee)?)?
            .checked_add(total_fixed_fee_apr)?;

        let group_fees_apr = calc_fee_rate(
            base_rate,
            self.protocol_ir_fee.into(),
            self.protocol_fixed_fee_apr.into(),
        )?;

        let insurance_fees_apr = calc_fee_rate(
            base_rate,
            self.insurance_ir_fee.into(),
            self.insurance_fee_fixed_apr.into(),
        )?;

        // TODO: Add liquidation discount check

        Some((
            lending_rate,
            borrowing_rate,
            group_fees_apr,
            insurance_fees_apr,
        ))
    }

    /// Piecewise linear interest rate function.
    /// The curves approaches the `plateau_interest_rate` as the utilization ratio approaches the `optimal_utilization_rate`,
    /// once the utilization ratio exceeds the `optimal_utilization_rate`, the curve approaches the `max_interest_rate`.
    ///
    /// To be clear we don't particularly appreciate the piecewise linear nature of this "curve", but it is what it is.
    #[inline]
    fn interest_rate_curve(&self, ur: I80F48) -> Option<I80F48> {
        let optimal_ur = self.optimal_utilization_rate.into();
        let plateau_ir = self.plateau_interest_rate.into();
        let max_ir: I80F48 = self.max_interest_rate.into();

        if ur <= optimal_ur {
            ur.checked_div(optimal_ur)?.checked_mul(plateau_ir)
        } else {
            (ur - optimal_ur)
                .checked_div(I80F48::ONE - optimal_ur)?
                .checked_mul(max_ir - plateau_ir)?
                .checked_add(plateau_ir)
        }
    }
}

/// Calculates the fee rate for a given base rate and fees specified.
/// The returned rate is only the fee rate without the base rate.
///
/// Used for calculating the fees charged to the borrowers.
fn calc_fee_rate(base_rate: I80F48, rate_fees: I80F48, fixed_fees: I80F48) -> Option<I80F48> {
    base_rate.checked_mul(rate_fees)?.checked_add(fixed_fees)
}
