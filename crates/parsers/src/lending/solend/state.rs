use crate::lending::solend::solend_decimal::SolendDecimal;
use crate::lending::solend::RateLimiter;
use crate::lending::solend::RATE_LIMITER_LEN;
use crate::lending::solend::{unpack_bool, unpack_solend_decimal};

use anyhow::bail;
use anyhow::{anyhow, Result};
use arrayref::array_ref;
use arrayref::array_refs;
use borsh::{BorshDeserialize, BorshSerialize};
use num_traits::FromPrimitive;
use rust_decimal::Decimal;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::pubkey::PUBKEY_BYTES;
use std::cmp::{max, min};
use std::ops::Div;
use std::str::FromStr;

use super::TryAdd;
use super::TrySub;
use super::WAD;

pub const MAX_PROTOCOL_LIQUIDATION_FEE_DECA_BPS: u8 = 100;
pub const RESERVE_LEN: usize = 619;
pub const PROGRAM_VERSION: u8 = 1;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Reserve {
    pub version: u8,
    pub last_update: LastUpdate,
    pub lending_market: Pubkey,
    pub liquidity: ReserveLiquidity,
    pub collateral: ReserveCollateral,
    pub config: ReserveConfig,
    pub rate_limiter: RateLimiter,
    pub attributed_borrow_value: SolendDecimal,
}
#[derive(BorshDeserialize, BorshSerialize, Debug, Default, Clone, PartialEq)]
pub struct LastUpdate {
    pub slot: u64,
    pub stale: bool,
}
#[allow(dead_code)]
#[derive(BorshDeserialize, Debug, Default, Clone)]
pub struct ReserveLiquidity {
    pub mint_pubkey: Pubkey,
    pub mint_decimals: u8,
    pub supply_pubkey: Pubkey,
    pub pyth_oracle_pubkey: Pubkey,
    pub switchboard_oracle_pubkey: Pubkey,
    pub available_amount: u64,
    pub borrowed_amount_wads: SolendDecimal,
    pub cumulative_borrow_rate_wads: SolendDecimal,
    pub accumulated_protocol_fees_wads: SolendDecimal,
    pub market_price: SolendDecimal,
    pub smoothed_market_price: SolendDecimal,
    pub extra_market_price: Option<SolendDecimal>,
}

#[derive(BorshDeserialize, Debug, Default, Clone)]
pub struct ReserveCollateral {
    pub mint_pubkey: Pubkey,
    pub mint_total_supply: u64,
    pub supply_pubkey: Pubkey,
}
#[allow(dead_code)]
#[derive(BorshDeserialize, Debug, Default, Clone)]
pub struct ReserveConfig {
    pub optimal_utilization_rate: u8,
    pub max_utilization_rate: u8,
    pub loan_to_value_ratio: u8,
    pub liquidation_bonus: u8,
    pub max_liquidation_bonus: u8,
    pub liquidation_threshold: u8,
    pub max_liquidation_threshold: u8,
    pub min_borrow_rate: u8,
    pub optimal_borrow_rate: u8,
    pub max_borrow_rate: u8,
    pub super_max_borrow_rate: u64,
    pub fees: ReserveFees,
    pub deposit_limit: u64,
    pub borrow_limit: u64,
    pub fee_receiver: Pubkey,
    pub protocol_liquidation_fee: u8,
    pub protocol_take_rate: u8,
    pub added_borrow_weight_bps: u64,
    pub reserve_type: ReserveType,
    pub scaled_price_offset_bps: i64,
    pub extra_oracle_pubkey: Option<Pubkey>,
    pub attributed_borrow_limit_open: u64,
    pub attributed_borrow_limit_close: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReserveType {
    #[default]
    Regular = 0,
    Isolated = 1,
}
impl FromStr for ReserveType {
    type Err = anyhow::Error;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "Regular" => Ok(ReserveType::Regular),
            "Isolated" => Ok(ReserveType::Isolated),
            _ => Err(anyhow::anyhow!("InvalidConfig")),
        }
    }
}

impl ReserveType {
    pub fn from_u8(value: u8) -> anyhow::Result<Self> {
        match value {
            0 => Ok(ReserveType::Regular),
            1 => Ok(ReserveType::Isolated),
            _ => Err(anyhow::anyhow!("InvalidConfig")),
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize, Debug, Default, Clone)]
pub struct ReserveFees {
    pub borrow_fee_wad: u64,
    pub flash_loan_fee_wad: u64,
    pub host_fee_percentage: u8,
}

#[derive(BorshDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RateLimiterConfig {
    pub window_duration: u64,
    pub max_outflow: u64,
}

impl Reserve {
    /// See https://github.com/solendprotocol/solana-program-library/blob/mainnet/token-lending/sdk/src/state/reserve.rs#L689
    /// and https://github.com/solendprotocol/solana-program-library/blob/mainnet/token-lending/sdk/src/state/reserve.rs#L875
    pub fn total_liq(&self) -> Result<u64> {
        SolendDecimal::from(self.liquidity.available_amount)
            .try_add(self.liquidity.borrowed_amount_wads)?
            .try_sub(self.liquidity.accumulated_protocol_fees_wads)?
            .try_floor_u64()
    }

    #[allow(clippy::unwrap_used)]
    pub fn unpack_from_slice(input: &[u8]) -> anyhow::Result<Self> {
        let input = array_ref![input, 0, RESERVE_LEN];
        #[allow(clippy::ptr_offset_with_cast)]
        let (
            version,
            last_update_slot,
            last_update_stale,
            lending_market,
            liquidity_mint_pubkey,
            liquidity_mint_decimals,
            liquidity_supply_pubkey,
            liquidity_pyth_oracle_pubkey,
            liquidity_switchboard_oracle_pubkey,
            liquidity_available_amount,
            liquidity_borrowed_amount_wads,
            liquidity_cumulative_borrow_rate_wads,
            liquidity_market_price,
            collateral_mint_pubkey,
            collateral_mint_total_supply,
            collateral_supply_pubkey,
            config_optimal_utilization_rate,
            config_loan_to_value_ratio,
            config_liquidation_bonus,
            config_liquidation_threshold,
            config_min_borrow_rate,
            config_optimal_borrow_rate,
            config_max_borrow_rate,
            config_fees_borrow_fee_wad,
            config_fees_flash_loan_fee_wad,
            config_fees_host_fee_percentage,
            config_deposit_limit,
            config_borrow_limit,
            config_fee_receiver,
            config_protocol_liquidation_fee,
            config_protocol_take_rate,
            liquidity_accumulated_protocol_fees_wads,
            rate_limiter,
            config_added_borrow_weight_bps,
            liquidity_smoothed_market_price,
            config_asset_type,
            config_max_utilization_rate,
            config_super_max_borrow_rate,
            config_max_liquidation_bonus,
            config_max_liquidation_threshold,
            config_scaled_price_offset_bps,
            config_extra_oracle_pubkey,
            liquidity_extra_market_price_flag,
            liquidity_extra_market_price,
            attributed_borrow_value,
            config_attributed_borrow_limit_open,
            config_attributed_borrow_limit_close,
            _padding,
        ) = array_refs![
            input,
            1,
            8,
            1,
            PUBKEY_BYTES,
            PUBKEY_BYTES,
            1,
            PUBKEY_BYTES,
            PUBKEY_BYTES,
            PUBKEY_BYTES,
            8,
            16,
            16,
            16,
            PUBKEY_BYTES,
            8,
            PUBKEY_BYTES,
            1,
            1,
            1,
            1,
            1,
            1,
            1,
            8,
            8,
            1,
            8,
            8,
            PUBKEY_BYTES,
            1,
            1,
            16,
            RATE_LIMITER_LEN,
            8,
            16,
            1,
            1,
            8,
            1,
            1,
            8,
            32,
            1,
            16,
            16,
            8,
            8,
            49
        ];

        let version = u8::from_le_bytes(*version);
        if version > PROGRAM_VERSION {
            return Err(anyhow!(
                "Reserve version does not match lending program version"
            ));
        }

        let optimal_utilization_rate = u8::from_le_bytes(*config_optimal_utilization_rate);
        let max_borrow_rate = u8::from_le_bytes(*config_max_borrow_rate);

        // on program upgrade, the max_* values are zero, so we need to safely account for that.
        let liquidation_bonus = u8::from_le_bytes(*config_liquidation_bonus);
        let max_liquidation_bonus = max(
            liquidation_bonus,
            u8::from_le_bytes(*config_max_liquidation_bonus),
        );
        let liquidation_threshold = u8::from_le_bytes(*config_liquidation_threshold);
        let max_liquidation_threshold = max(
            liquidation_threshold,
            u8::from_le_bytes(*config_max_liquidation_threshold),
        );

        Ok(Self {
            version,
            last_update: LastUpdate {
                slot: u64::from_le_bytes(*last_update_slot),
                stale: unpack_bool(last_update_stale)?,
            },
            lending_market: Pubkey::new_from_array(*lending_market),
            liquidity: ReserveLiquidity {
                mint_pubkey: Pubkey::new_from_array(*liquidity_mint_pubkey),
                mint_decimals: u8::from_le_bytes(*liquidity_mint_decimals),
                supply_pubkey: Pubkey::new_from_array(*liquidity_supply_pubkey),
                pyth_oracle_pubkey: Pubkey::new_from_array(*liquidity_pyth_oracle_pubkey),
                switchboard_oracle_pubkey: Pubkey::new_from_array(
                    *liquidity_switchboard_oracle_pubkey,
                ),
                available_amount: u64::from_le_bytes(*liquidity_available_amount),
                borrowed_amount_wads: unpack_solend_decimal(liquidity_borrowed_amount_wads),
                cumulative_borrow_rate_wads: unpack_solend_decimal(
                    liquidity_cumulative_borrow_rate_wads,
                ),
                accumulated_protocol_fees_wads: unpack_solend_decimal(
                    liquidity_accumulated_protocol_fees_wads,
                ),
                market_price: unpack_solend_decimal(liquidity_market_price),
                smoothed_market_price: unpack_solend_decimal(liquidity_smoothed_market_price),
                extra_market_price: match liquidity_extra_market_price_flag[0] {
                    0 => None,
                    1 => Some(unpack_solend_decimal(liquidity_extra_market_price)),
                    _ => return Err(anyhow::anyhow!("Invalid extra market price flag")),
                },
            },
            collateral: ReserveCollateral {
                mint_pubkey: Pubkey::new_from_array(*collateral_mint_pubkey),
                mint_total_supply: u64::from_le_bytes(*collateral_mint_total_supply),
                supply_pubkey: Pubkey::new_from_array(*collateral_supply_pubkey),
            },
            config: ReserveConfig {
                optimal_utilization_rate,
                max_utilization_rate: max(
                    optimal_utilization_rate,
                    u8::from_le_bytes(*config_max_utilization_rate),
                ),
                loan_to_value_ratio: u8::from_le_bytes(*config_loan_to_value_ratio),
                liquidation_bonus,
                max_liquidation_bonus,
                liquidation_threshold,
                max_liquidation_threshold,
                min_borrow_rate: u8::from_le_bytes(*config_min_borrow_rate),
                optimal_borrow_rate: u8::from_le_bytes(*config_optimal_borrow_rate),
                max_borrow_rate,
                super_max_borrow_rate: max(
                    max_borrow_rate as u64,
                    u64::from_le_bytes(*config_super_max_borrow_rate),
                ),
                fees: ReserveFees {
                    borrow_fee_wad: u64::from_le_bytes(*config_fees_borrow_fee_wad),
                    flash_loan_fee_wad: u64::from_le_bytes(*config_fees_flash_loan_fee_wad),
                    host_fee_percentage: u8::from_le_bytes(*config_fees_host_fee_percentage),
                },
                deposit_limit: u64::from_le_bytes(*config_deposit_limit),
                borrow_limit: u64::from_le_bytes(*config_borrow_limit),
                fee_receiver: Pubkey::new_from_array(*config_fee_receiver),
                protocol_liquidation_fee: min(
                    u8::from_le_bytes(*config_protocol_liquidation_fee),
                    // the behaviour of this variable changed in v2.0.2 and now represents a
                    // fraction of the total liquidation value that the protocol receives as
                    // a bonus. Prior to v2.0.2, this variable used to represent a percentage of of
                    // the liquidator's bonus that would be sent to the protocol. For safety, we
                    // cap the value here to MAX_PROTOCOL_LIQUIDATION_FEE_DECA_BPS.
                    MAX_PROTOCOL_LIQUIDATION_FEE_DECA_BPS,
                ),
                protocol_take_rate: u8::from_le_bytes(*config_protocol_take_rate),
                added_borrow_weight_bps: u64::from_le_bytes(*config_added_borrow_weight_bps),
                reserve_type: ReserveType::from_u8(config_asset_type[0]).unwrap(),
                scaled_price_offset_bps: i64::from_le_bytes(*config_scaled_price_offset_bps),
                extra_oracle_pubkey: if config_extra_oracle_pubkey == &[0; 32] {
                    None
                } else {
                    Some(Pubkey::new_from_array(*config_extra_oracle_pubkey))
                },
                // this field is added in v2.0.3 and we will never set it to zero. only time it'll
                // the following two fields are added in v2.0.3 and we will never set it to zero. only time they will
                // be zero is when we upgrade from v2.0.2 to v2.0.3. in that case, the correct
                // thing to do is set the value to u64::MAX.
                attributed_borrow_limit_open: {
                    let value = u64::from_le_bytes(*config_attributed_borrow_limit_open);
                    if value == 0 {
                        u64::MAX
                    } else {
                        value
                    }
                },
                attributed_borrow_limit_close: {
                    let value = u64::from_le_bytes(*config_attributed_borrow_limit_close);
                    if value == 0 {
                        u64::MAX
                    } else {
                        value
                    }
                },
            },
            rate_limiter: RateLimiter::unpack_from_slice(rate_limiter)?,
            attributed_borrow_value: unpack_solend_decimal(attributed_borrow_value),
        })
    }

    pub fn calc_utilization_ratio(&self) -> Result<Decimal> {
        // Use borrowed_amount_wads directly without dividing by WAD
        let borrowed_f64 = self.liquidity.borrowed_amount_wads.to_scaled_val()? as f64 / WAD as f64;
        let available_f64 = self.liquidity.available_amount as f64;

        // Calculate utilization ratio
        let total_f64 = borrowed_f64 + available_f64;
        if total_f64 == 0.0 {
            return Ok(Decimal::ZERO);
        }

        let ratio = borrowed_f64 / total_f64;
        Decimal::from_f64(ratio).ok_or(anyhow::anyhow!("failed to convert decimal f64"))
    }

    pub fn calc_supply_apr(&self) -> Result<Decimal> {
        let utilization = self.calc_utilization_ratio()?;
        let borrow = self.calc_borrow_apr()?;
        Ok(utilization * borrow)
    }

    pub fn calc_borrow_apr(&self) -> Result<Decimal> {
        let utilization = self.calc_utilization_ratio()?;

        let optimal = Decimal::from(self.config.optimal_utilization_rate).div(Decimal::from(100));

        let one = Decimal::ONE;

        let min_rate = Decimal::from(self.config.min_borrow_rate).div(Decimal::from(100));
        let opt_rate = Decimal::from(self.config.optimal_borrow_rate).div(Decimal::from(100));
        let max_rate = Decimal::from(self.config.max_borrow_rate).div(Decimal::from(100));

        if utilization.is_zero() {
            return Ok(min_rate);
        }

        let apr = if utilization <= optimal {
            let factor = utilization / optimal;
            factor * (opt_rate - min_rate) + min_rate
        } else {
            let factor = (utilization - optimal) / (one - optimal);
            factor * (max_rate - opt_rate) + opt_rate
        };

        Ok(apr)
    }
}

/// Lending market state
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LendingMarket {
    /// Version of lending market
    pub version: u8,
    /// Bump seed for derived authority address
    pub bump_seed: u8,
    /// Owner authority which can add new reserves
    pub owner: Pubkey,
    /// Currency market prices are quoted in
    /// e.g. "USD" null padded (`*b"USD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"`) or a SPL token mint pubkey
    pub quote_currency: [u8; 32],
    /// Token program id
    pub token_program_id: Pubkey,
    /// Oracle (Pyth) program id
    pub oracle_program_id: Pubkey,
    /// Oracle (Switchboard) program id
    pub switchboard_oracle_program_id: Pubkey,
    /// Outflow rate limiter denominated in dollars
    pub rate_limiter: RateLimiter,
    /// whitelisted liquidator
    pub whitelisted_liquidator: Option<Pubkey>,
    /// risk authority (additional pubkey used for setting params)
    pub risk_authority: Pubkey,
}

/// Max number of collateral and liquidity reserve accounts combined for an obligation
pub const MAX_OBLIGATION_RESERVES: usize = 10;

/// Lending market obligation state
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Obligation {
    /// Version of the struct
    pub version: u8,
    /// Last update to collateral, liquidity, or their market values
    pub last_update: LastUpdate,
    /// Lending market address
    pub lending_market: Pubkey,
    /// Owner authority which can borrow liquidity
    pub owner: Pubkey,
    /// Deposited collateral for the obligation, unique by deposit reserve address
    pub deposits: Vec<ObligationCollateral>,
    /// Borrowed liquidity for the obligation, unique by borrow reserve address
    pub borrows: Vec<ObligationLiquidity>,
    /// Market value of deposits
    pub deposited_value: SolendDecimal,
    /// Risk-adjusted market value of borrows.
    /// ie sum(b.borrowed_amount * b.current_spot_price * b.borrow_weight for b in borrows)
    pub borrowed_value: SolendDecimal,
    /// True borrow value. Ie, not risk adjusted like "borrowed_value"
    pub unweighted_borrowed_value: SolendDecimal,
    /// Risk-adjusted upper bound market value of borrows.
    /// ie sum(b.borrowed_amount * max(b.current_spot_price, b.smoothed_price) * b.borrow_weight for b in borrows)
    pub borrowed_value_upper_bound: SolendDecimal,
    /// The maximum open borrow value.
    /// ie sum(d.deposited_amount * d.ltv * min(d.current_spot_price, d.smoothed_price) for d in deposits)
    /// if borrowed_value_upper_bound >= allowed_borrow_value, then the obligation is unhealthy and
    /// borrows and withdraws are disabled.
    pub allowed_borrow_value: SolendDecimal,
    /// The dangerous borrow value at the weighted average liquidation threshold.
    /// ie sum(d.deposited_amount * d.liquidation_threshold * d.current_spot_price for d in deposits)
    /// if borrowed_value >= unhealthy_borrow_value, the obligation can be liquidated
    pub unhealthy_borrow_value: SolendDecimal,
    /// ie sum(d.deposited_amount * d.max_liquidation_threshold * d.current_spot_price for d in
    /// deposits). This field is used to calculate the liquidator bonus.
    /// An obligation with a borrowed value >= super_unhealthy_borrow_value is eligible for the max
    /// bonus
    pub super_unhealthy_borrow_value: SolendDecimal,
    /// True if the obligation is currently borrowing an isolated tier asset
    pub borrowing_isolated_asset: bool,
    /// Obligation can be marked as closeable
    pub closeable: bool,
}

/// Obligation collateral state
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObligationCollateral {
    /// Reserve collateral is deposited to
    pub deposit_reserve: Pubkey,
    /// Amount of collateral deposited
    pub deposited_amount: u64,
    /// Collateral market value in quote currency
    pub market_value: SolendDecimal,
    /// How much borrow is attributed to this collateral (USD)
    pub attributed_borrow_value: SolendDecimal,
}

/// Obligation liquidity state
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObligationLiquidity {
    /// Reserve liquidity is borrowed from
    pub borrow_reserve: Pubkey,
    /// Borrow rate used for calculating interest
    pub cumulative_borrow_rate_wads: SolendDecimal,
    /// Amount of liquidity borrowed plus interest
    pub borrowed_amount_wads: SolendDecimal,
    /// Liquidity market value in quote currency
    pub market_value: SolendDecimal,
}

const OBLIGATION_COLLATERAL_LEN: usize = 88; // 32 + 8 + 16 + 32
const OBLIGATION_LIQUIDITY_LEN: usize = 112; // 32 + 16 + 16 + 16 + 32
const OBLIGATION_LEN: usize = 1300; // 1 + 8 + 1 + 32 + 32 + 16 + 16 + 16 + 16 + 64 + 1 + 1 + (88 * 1) + (112 * 9)
                                    // @TODO: break this up by obligation / collateral / liquidity https://git.io/JOCca
impl Obligation {
    /// Unpacks a byte buffer into an [ObligationInfo](struct.ObligationInfo.html).
    pub fn unpack_from_slice(src: &[u8]) -> Result<Self> {
        let input = array_ref![src, 0, OBLIGATION_LEN];
        #[allow(clippy::ptr_offset_with_cast)]
        let (
            version,
            last_update_slot,
            last_update_stale,
            lending_market,
            owner,
            deposited_value,
            borrowed_value,
            allowed_borrow_value,
            unhealthy_borrow_value,
            borrowed_value_upper_bound,
            borrowing_isolated_asset,
            super_unhealthy_borrow_value,
            unweighted_borrowed_value,
            closeable,
            _padding,
            deposits_len,
            borrows_len,
            data_flat,
        ) = array_refs![
            input,
            1,
            8,
            1,
            PUBKEY_BYTES,
            PUBKEY_BYTES,
            16,
            16,
            16,
            16,
            16,
            1,
            16,
            16,
            1,
            14,
            1,
            1,
            OBLIGATION_COLLATERAL_LEN + (OBLIGATION_LIQUIDITY_LEN * (MAX_OBLIGATION_RESERVES - 1))
        ];

        let version = u8::from_le_bytes(*version);
        if version > PROGRAM_VERSION {
            bail!("Obligation version does not match lending program version");
        }

        let deposits_len = u8::from_le_bytes(*deposits_len);
        let borrows_len = u8::from_le_bytes(*borrows_len);
        let mut deposits = Vec::with_capacity(deposits_len as usize + 1);
        let mut borrows = Vec::with_capacity(borrows_len as usize + 1);

        let mut offset = 0;
        for _ in 0..deposits_len {
            let deposits_flat = array_ref![data_flat, offset, OBLIGATION_COLLATERAL_LEN];
            #[allow(clippy::ptr_offset_with_cast)]
            let (
                deposit_reserve,
                deposited_amount,
                market_value,
                attributed_borrow_value,
                _padding_deposit,
            ) = array_refs![deposits_flat, PUBKEY_BYTES, 8, 16, 16, 16];
            deposits.push(ObligationCollateral {
                deposit_reserve: Pubkey::from(*deposit_reserve),
                deposited_amount: u64::from_le_bytes(*deposited_amount),
                market_value: unpack_decimal(market_value),
                attributed_borrow_value: unpack_decimal(attributed_borrow_value),
            });
            offset += OBLIGATION_COLLATERAL_LEN;
        }
        for _ in 0..borrows_len {
            let borrows_flat = array_ref![data_flat, offset, OBLIGATION_LIQUIDITY_LEN];
            #[allow(clippy::ptr_offset_with_cast)]
            let (
                borrow_reserve,
                cumulative_borrow_rate_wads,
                borrowed_amount_wads,
                market_value,
                _padding_borrow,
            ) = array_refs![borrows_flat, PUBKEY_BYTES, 16, 16, 16, 32];
            borrows.push(ObligationLiquidity {
                borrow_reserve: Pubkey::from(*borrow_reserve),
                cumulative_borrow_rate_wads: unpack_decimal(cumulative_borrow_rate_wads),
                borrowed_amount_wads: unpack_decimal(borrowed_amount_wads),
                market_value: unpack_decimal(market_value),
            });
            offset += OBLIGATION_LIQUIDITY_LEN;
        }

        Ok(Self {
            version,
            last_update: LastUpdate {
                slot: u64::from_le_bytes(*last_update_slot),
                stale: unpack_bool(last_update_stale)?,
            },
            lending_market: Pubkey::new_from_array(*lending_market),
            owner: Pubkey::new_from_array(*owner),
            deposits,
            borrows,
            deposited_value: unpack_decimal(deposited_value),
            borrowed_value: unpack_decimal(borrowed_value),
            unweighted_borrowed_value: unpack_decimal(unweighted_borrowed_value),
            borrowed_value_upper_bound: unpack_decimal(borrowed_value_upper_bound),
            allowed_borrow_value: unpack_decimal(allowed_borrow_value),
            unhealthy_borrow_value: unpack_decimal(unhealthy_borrow_value),
            super_unhealthy_borrow_value: unpack_decimal(super_unhealthy_borrow_value),
            borrowing_isolated_asset: unpack_bool(borrowing_isolated_asset)?,
            closeable: unpack_bool(closeable)?,
        })
    }
}

fn unpack_decimal(src: &[u8; 16]) -> SolendDecimal {
    SolendDecimal::from_scaled_val(u128::from_le_bytes(*src))
}
