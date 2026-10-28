// code adapted from https://github.com/DeFi-Carrot/jup-carrot-integration/blob/main/amm/src/state.rs
use anyhow::{anyhow, Result};
use solana_sdk::pubkey::Pubkey;

use super::math::shares_earned;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Vault {
    pub authority: Pubkey,
    pub shares: Pubkey,
    pub fee: Fee,
    pub paused: bool,
    pub asset_index: u16,
    pub strategy_index: u16,
    pub assets: Vec<Asset>,
    pub strategies: Vec<StrategyRecord>,
}

impl Vault {
    pub fn load(account_data: &[u8]) -> Result<Self> {
        let mut offset = 8; // start at 8 to skip anchor account discriminator

        // Read fixed size fields
        let authority = Pubkey::new_from_array(account_data[offset..offset + 32].try_into()?);
        offset += 32;

        let shares = Pubkey::new_from_array(account_data[offset..offset + 32].try_into()?);
        offset += 32;

        // Assuming Fee::load exists and correctly handles deserialization
        let fee = Fee::load(&account_data[offset..offset + Fee::SPACE])?;
        offset += Fee::SPACE;

        let paused = account_data[offset] > 0;
        offset += 1;

        let asset_index = u16::from_le_bytes(account_data[offset..offset + 2].try_into()?);
        offset += 2;

        let strategy_index = u16::from_le_bytes(account_data[offset..offset + 2].try_into()?);
        offset += 2;

        // Dynamic Vec<Asset> deserialization
        let assets_len = u32::from_le_bytes(account_data[offset..offset + 4].try_into()?);
        offset += 4;
        let mut assets = Vec::with_capacity(assets_len as usize);
        for _ in 0..assets_len {
            let asset = Asset::load(&account_data[offset..offset + Asset::SPACE])?;
            assets.push(asset);
            offset += Asset::SPACE;
        }

        // Dynamic Vec<StrategyRecord> deserialization
        let strategies_len = u32::from_le_bytes(account_data[offset..offset + 4].try_into()?);
        offset += 4;
        let mut strategies = Vec::with_capacity(strategies_len as usize);
        for _ in 0..strategies_len {
            let strategy =
                StrategyRecord::load(&account_data[offset..offset + StrategyRecord::SPACE])?;
            strategies.push(strategy);
            offset += StrategyRecord::SPACE;
        }

        Ok(Vault {
            authority,
            shares,
            fee,
            paused,
            asset_index,
            strategy_index,
            assets,
            strategies,
        })
    }

    pub fn get_underlying_balances(&self, asset_state: &[AssetState]) -> Result<Vec<u64>> {
        // loop assets and sum their ata amounts with the strategy balances for their asset
        let mut balances = Vec::with_capacity(self.assets.len());

        for asset in &self.assets {
            // Get the ATA amount for this asset
            let state = get_asset_state_by_id(asset_state, asset.asset_id)?;
            let mut total_balance = state.ata_amount;

            // Add up all strategy balances for this asset
            for strategy in &self.strategies {
                if strategy.asset_id == asset.asset_id {
                    total_balance = total_balance
                        .checked_add(strategy.balance)
                        .ok_or(anyhow!("CARROT VAULT: Invalid token calculation"))?;
                }
            }

            balances.push(total_balance);
        }

        Ok(balances)
    }

    pub fn calculate_accumulated_performance_fee(
        &self,
        shares_supply: u64,
        shares_decimals: u8,
        vault_tvl: u128,
    ) -> Result<u64> {
        let mut performance_fee_accumulated: u64 = 0;
        for strategy in self.strategies.iter() {
            // calculate performance fee for each strategy
            let strategy_performance_fee = self.fee.calculate_performance_fee(
                strategy.net_earnings,
                shares_supply,
                shares_decimals,
                vault_tvl,
            )?;
            performance_fee_accumulated = performance_fee_accumulated
                .checked_add(strategy_performance_fee)
                .ok_or(anyhow!("CARROT VAULT: Invalid fee calculation"))?;
        }

        Ok(performance_fee_accumulated)
    }
}

// data

#[derive(Clone, Copy, Debug)]
pub struct Asset {
    pub asset_id: u16,
    pub mint: Pubkey,
    pub decimals: u8,
    pub ata: Pubkey,
}

impl Asset {
    // last 32 bytes are oracle data, which we dont use
    pub const SPACE: usize = 2 + 32 + 1 + 32 + 32;

    pub fn load(account_data: &[u8]) -> Result<Self> {
        let asset_id = u16::from_le_bytes(account_data[0..2].try_into()?);
        let mint = Pubkey::new_from_array(account_data[2..34].try_into()?);
        let decimals = account_data[34];
        let ata = Pubkey::new_from_array(account_data[35..67].try_into()?);

        Ok(Asset {
            asset_id,
            mint,
            decimals,
            ata,
        })
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct StrategyRecord {
    pub strategy_id: u16,
    pub asset_id: u16,
    pub balance: u64,
    pub net_earnings: i64,
}

impl StrategyRecord {
    pub const SPACE: usize = 2 + 2 + 8 + 8;

    pub fn load(account_data: &[u8]) -> Result<Self> {
        let strategy_id = u16::from_le_bytes(account_data[0..2].try_into()?);
        let asset_id = u16::from_le_bytes(account_data[2..4].try_into()?);
        let balance = u64::from_le_bytes(account_data[4..12].try_into()?);
        let net_earnings = i64::from_le_bytes(account_data[12..20].try_into()?);

        Ok(StrategyRecord {
            strategy_id,
            asset_id,
            balance,
            net_earnings,
        })
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct Fee {
    pub redemption_fee_bps: u16,
    pub redemption_fee_accumulated: u64,
    pub management_fee_bps: u16,
    pub management_fee_last_update: i64,
    pub management_fee_accumulated: u64,
    pub performance_fee_bps: u16,
}

impl Fee {
    // Assuming the SPACE constant for Fee is defined as the sum of its fields' sizes
    pub const SPACE: usize = 2 + 8 + 2 + 8 + 8 + 2; // Example, adjust based on actual sizes

    const SECONDS_IN_YEAR: f64 = 31557600.0;

    pub fn load(account_data: &[u8]) -> Result<Self> {
        let mut offset = 0;

        // Deserialize each field from the byte slice
        let redemption_fee_bps = u16::from_le_bytes(account_data[offset..offset + 2].try_into()?);
        offset += 2;

        let redemption_fee_accumulated =
            u64::from_le_bytes(account_data[offset..offset + 8].try_into()?);
        offset += 8;

        let management_fee_bps = u16::from_le_bytes(account_data[offset..offset + 2].try_into()?);
        offset += 2;

        let management_fee_last_update =
            i64::from_le_bytes(account_data[offset..offset + 8].try_into()?);
        offset += 8;

        let management_fee_accumulated =
            u64::from_le_bytes(account_data[offset..offset + 8].try_into()?);
        offset += 8;

        let performance_fee_bps = u16::from_le_bytes(account_data[offset..offset + 2].try_into()?);
        // No need to adjust offset here if it's the last field

        Ok(Fee {
            redemption_fee_bps,
            redemption_fee_accumulated,
            management_fee_bps,
            management_fee_last_update,
            management_fee_accumulated,
            performance_fee_bps,
        })
    }

    // returns the number of shares that should be minted to the fee account
    // increments accumulated store
    pub fn calculate_management_fee(
        &self,
        tvl: u128,
        shares_supply: u64,
        shares_decimals: u8,
        block_time: i64,
    ) -> Result<u64> {
        // require a delta of over 60 seconds
        let time_delta = block_time - self.management_fee_last_update;
        if time_delta <= 60 {
            return Ok(0);
        }

        // Calculate elapsed time in seconds
        let elapsed_seconds = time_delta as u128;

        // Calculate fee in USD cents using integer arithmetic
        let fee_usd_cents = self
            .calc_management_fee(tvl)?
            .checked_mul(elapsed_seconds)
            .and_then(|prod| prod.checked_div(Fee::SECONDS_IN_YEAR as u128))
            .ok_or(anyhow!("CARROT VAULT: Invalid fee calculation"))?;

        if fee_usd_cents == 0 {
            return Ok(0);
        }

        // convert usd cents to shares ui based on NAV
        let shares_amount = shares_earned(fee_usd_cents, shares_supply, shares_decimals, tvl, true)
            .ok_or(anyhow!("CARROT VAULT: Invalid token calculation"))?;

        Ok(shares_amount)
    }

    // returns the shares amount of the performance fee that should be minted to the fee account
    pub fn calculate_performance_fee(
        &self,
        net_earnings: i64,
        shares_supply: u64,
        shares_decimals: u8,
        vault_tvl: u128,
    ) -> Result<u64> {
        // if we lost/didnt make any money dont charge a fee
        if net_earnings.le(&0) {
            return Ok(0);
        }

        // calculate performance fee
        let fee_amount = self.calc_performance_fee(net_earnings as u128)?;

        shares_earned(fee_amount, shares_supply, shares_decimals, vault_tvl, true)
            .ok_or(anyhow!("CARROT VAULT: Invalid token calculation"))
    }

    // inflates the shares_supply by the amount of unrealized fees accrued by the protocol
    // performance fees is computed inside the ix, which is why we pass it in
    pub fn adjust_shares_by_fees(
        &self,
        shares_supply: u64,
        total_performance_fees_accumulated: u64,
    ) -> Result<u64> {
        shares_supply
            .checked_add(total_performance_fees_accumulated)
            .and_then(|sum| sum.checked_add(self.management_fee_accumulated))
            .and_then(|sum| sum.checked_add(self.redemption_fee_accumulated))
            .ok_or(anyhow!("CARROT VAULT: Invalid token calculation"))
    }

    fn calc_management_fee(&self, tvl: u128) -> Result<u128> {
        tvl.checked_mul(self.management_fee_bps as u128)
            .and_then(|prod| prod.checked_add(9_999))
            .and_then(|sum| sum.checked_div(10_000))
            .ok_or(anyhow!("CARROT VAULT: Invalid fee calculation"))
    }

    fn calc_performance_fee(&self, net_earnings_usd: u128) -> Result<u128> {
        net_earnings_usd
            .checked_mul(self.performance_fee_bps as u128)
            .and_then(|prod| prod.checked_add(9_999))
            .and_then(|sum| sum.checked_div(10_000))
            .ok_or(anyhow!("CARROT VAULT: Invalid fee calculation"))
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct AssetState {
    pub asset_id: u16,
    pub mint: Pubkey,
    pub mint_decimals: u8,
    pub ata_amount: u64,
}

pub fn get_asset_state_by_id(asset_state: &[AssetState], asset_id: u16) -> Result<&AssetState> {
    let asset = asset_state
        .iter()
        .find(|a| a.asset_id.eq(&asset_id))
        .ok_or(anyhow!("CARROT VAULT: asset state by ID not found"))?;
    Ok(asset)
}
