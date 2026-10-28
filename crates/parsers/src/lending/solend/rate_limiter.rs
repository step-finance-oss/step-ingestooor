use anyhow::bail;
use solana_sdk::program_pack::IsInitialized;
use solana_sdk::slot_history::Slot;

use crate::{
    lending::solend::common::{TryAdd, TrySub},
    lending::solend::solend_decimal::SolendDecimal,
    lending::solend::{pack_solend_decimal, unpack_solend_decimal},
};
use arrayref::{array_mut_ref, array_ref, array_refs, mut_array_refs};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_sdk::program_pack::Sealed;

use super::{TryDiv, TryMul};
/// Sliding Window Rate limiter
/// guarantee: at any point, the outflow between [cur_slot - slot.window_duration, cur_slot]
/// is less than 2x max_outflow.

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimiter {
    /// configuration parameters
    pub config: RateLimiterConfig,
    /// prev qty is the sum of all outflows from [window_start - config.window_duration, window_start)
    prev_qty: SolendDecimal,
    /// window_start is the start of the current window
    window_start: Slot,
    /// cur qty is the sum of all outflows from [window_start, window_start + config.window_duration)
    cur_qty: SolendDecimal,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RateLimiterConfig {
    /// Rate limiter window size in slots
    pub window_duration: u64,
    /// Rate limiter param. Max outflow of tokens in a window
    pub max_outflow: u64,
}

impl RateLimiter {
    /// initialize rate limiter
    pub fn new(config: RateLimiterConfig, cur_slot: u64) -> Self {
        let slot_start = if config.window_duration != 0 {
            cur_slot / config.window_duration * config.window_duration
        } else {
            cur_slot
        };

        Self {
            config,
            prev_qty: SolendDecimal::zero(),
            window_start: slot_start,
            cur_qty: SolendDecimal::zero(),
        }
    }

    fn _update(&mut self, cur_slot: u64) -> anyhow::Result<Self> {
        if cur_slot < self.window_start {
            bail!("Current slot is less than window start, which is impossible");
        }

        // floor wrt window duration
        let cur_slot_start = cur_slot / self.config.window_duration * self.config.window_duration;

        // update prev window, current window
        match cur_slot_start.cmp(&(self.window_start + self.config.window_duration)) {
            // |<-prev window->|<-cur window (cur_slot is in here)->|
            std::cmp::Ordering::Less => (),

            // |<-prev window->|<-cur window->| (cur_slot is in here) |
            std::cmp::Ordering::Equal => {
                self.prev_qty = self.cur_qty;
                self.window_start = cur_slot_start;
                self.cur_qty = SolendDecimal::zero();
            }

            // |<-prev window->|<-cur window->|<-cur window + 1->| ... | (cur_slot is in here) |
            std::cmp::Ordering::Greater => {
                self.prev_qty = SolendDecimal::zero();
                self.window_start = cur_slot_start;
                self.cur_qty = SolendDecimal::zero();
            }
        }

        Ok(*self)
    }

    /// Calculate current outflow. Must only be called after ._update()!
    fn current_outflow(&self, cur_slot: u64) -> anyhow::Result<SolendDecimal> {
        if self.config.window_duration == 0 {
            bail!("Window duration cannot be 0");
        }

        // assume the prev_window's outflow is even distributed across the window
        // this isn't true, but it's a good enough approximation
        let prev_weight = SolendDecimal::from(self.config.window_duration)
            .try_sub(SolendDecimal::from(cur_slot - self.window_start + 1))?
            .try_div(self.config.window_duration)?;

        prev_weight.try_mul(self.prev_qty)?.try_add(self.cur_qty)
    }

    /// Calculate remaining outflow for the current window
    pub fn remaining_outflow(&mut self, cur_slot: u64) -> anyhow::Result<SolendDecimal> {
        // rate limiter is disabled if window duration == 0. this is here because we don't want to
        // brick borrows/withdraws in permissionless pools on program upgrade.
        if self.config.window_duration == 0 {
            return Ok(SolendDecimal::from(u64::MAX));
        }

        self._update(cur_slot)?;

        let cur_outflow = self.current_outflow(cur_slot)?;
        if cur_outflow > SolendDecimal::from(self.config.max_outflow) {
            return Ok(SolendDecimal::zero());
        }

        let diff = SolendDecimal::from(self.config.max_outflow).try_sub(cur_outflow)?;
        Ok(diff)
    }

    /// update rate limiter with new quantity. errors if rate limit has been reached
    pub fn update(&mut self, cur_slot: u64, qty: SolendDecimal) -> anyhow::Result<()> {
        // rate limiter is disabled if window duration == 0. this is here because we don't want to
        // brick borrows/withdraws in permissionless pools on program upgrade.
        if self.config.window_duration == 0 {
            return Ok(());
        }

        self._update(cur_slot)?;

        let cur_outflow = self.current_outflow(cur_slot)?;
        if cur_outflow.try_add(qty)? > SolendDecimal::from(self.config.max_outflow) {
            Err(anyhow::anyhow!("OutflowRateLimitExceeded"))
        } else {
            self.cur_qty = self.cur_qty.try_add(qty)?;
            Ok(())
        }
    }
}
impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(
            RateLimiterConfig {
                window_duration: 1,
                max_outflow: u64::MAX,
            },
            1,
        )
    }
}

impl Sealed for RateLimiter {}

impl IsInitialized for RateLimiter {
    fn is_initialized(&self) -> bool {
        true
    }
}

/// Size of RateLimiter when packed into account
pub const RATE_LIMITER_LEN: usize = 56;
impl RateLimiter {
    const _LEN: usize = RATE_LIMITER_LEN;

    pub fn pack_into_slice(&self, dst: &mut [u8]) {
        let dst = array_mut_ref![dst, 0, RATE_LIMITER_LEN];
        let (
            config_max_outflow_dst,
            config_window_duration_dst,
            prev_qty_dst,
            window_start_dst,
            cur_qty_dst,
        ) = mut_array_refs![dst, 8, 8, 16, 8, 16];
        *config_max_outflow_dst = self.config.max_outflow.to_le_bytes();
        *config_window_duration_dst = self.config.window_duration.to_le_bytes();
        pack_solend_decimal(self.prev_qty, prev_qty_dst);
        *window_start_dst = self.window_start.to_le_bytes();
        pack_solend_decimal(self.cur_qty, cur_qty_dst);
    }

    pub fn unpack_from_slice(src: &[u8]) -> anyhow::Result<Self> {
        let src = array_ref![src, 0, RATE_LIMITER_LEN];
        let (
            config_max_outflow_src,
            config_window_duration_src,
            prev_qty_src,
            window_start_src,
            cur_qty_src,
        ) = array_refs![src, 8, 8, 16, 8, 16];

        Ok(Self {
            config: RateLimiterConfig {
                max_outflow: u64::from_le_bytes(*config_max_outflow_src),
                window_duration: u64::from_le_bytes(*config_window_duration_src),
            },
            prev_qty: unpack_solend_decimal(prev_qty_src),
            window_start: u64::from_le_bytes(*window_start_src),
            cur_qty: unpack_solend_decimal(cur_qty_src),
        })
    }
}

#[cfg(test)]
pub fn rand_rate_limiter() -> RateLimiter {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    fn rand_solend_decimal() -> SolendDecimal {
        SolendDecimal::from_scaled_val(rand::thread_rng().gen())
    }

    RateLimiter {
        config: RateLimiterConfig {
            window_duration: rng.gen(),
            max_outflow: rng.gen(),
        },
        prev_qty: rand_solend_decimal(),
        window_start: rng.gen(),
        cur_qty: rand_solend_decimal(),
    }
}
