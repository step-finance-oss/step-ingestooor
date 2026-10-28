// code adapted from https://github.com/DeFi-Carrot/jup-carrot-integration/blob/main/amm/src/math.rs

// calculate the shares earned from depositing the usd
pub fn shares_earned(
    usd_value: u128,
    shares_supply: u64,
    shares_decimals: u8,
    vault_tvl: u128,
    round_up: bool,
) -> Option<u64> {
    if vault_tvl.le(&0) || shares_supply.le(&0) {
        // if vault_tvl or shares_supply is 0 or less, just mint 100 shares
        ui_to_amount(100.0, shares_decimals)
    } else {
        // Calculate shares
        let shares = if round_up {
            let prod = usd_value.checked_mul(shares_supply as u128)?;
            let sum = prod.checked_add(vault_tvl - 1)?;
            sum.checked_div(vault_tvl)?
        } else {
            let prod = usd_value.checked_mul(shares_supply as u128)?;
            prod.checked_div(vault_tvl)?
        };

        // Check if the result fits within u64
        if shares > (u64::MAX as u128) {
            Some(u64::MAX) // Handle overflow case, e.g., by returning the maximum u64 value
        } else {
            Some(shares as u64)
        }
    }
}

/// Converts a UI value (floating point) to an amount with the specified decimal places.
///
/// # Arguments
/// * `ui` - The UI value to convert (must be non-negative)
/// * `decimal` - The number of decimal places to use (must be <= 20)
///
/// # Returns
/// * `Option<u64>` - The converted amount, or None if:
///   - The input is negative
///   - The decimal places are invalid
///   - The conversion would overflow
///   - The result would be too large for u64
fn ui_to_amount(ui: f64, decimal: u8) -> Option<u64> {
    // Validate inputs
    if ui < 0.0 || decimal > 20 {
        return None;
    }

    // Convert the floating-point to an integer without losing precision
    let multiplier = (10u64).checked_pow(decimal as u32)?;

    // Check if the multiplication would overflow
    if ui > (u64::MAX as f64) / (multiplier as f64) {
        return None;
    }

    // Perform the conversion with overflow check
    let ui_int = (ui * (multiplier as f64)).round();
    if ui_int > (u64::MAX as f64) {
        return None;
    }

    Some(ui_int as u64)
}
