use std::str::FromStr;

use crate::{state_source::StateSource, TransactionParserFilter, TransactionParserTrait};
use anyhow::Result;
use async_trait::async_trait;
use borsh::BorshDeserialize;
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
use step_ingestooor_idl_gens::defi_tuna_gen::defituna::accounts::{
    FusionPool, LendingPosition, Market, TunaPosition, Vault,
};
use step_ingestooor_sdk::{
    dooot::{
        Dooot, PsDefiTunaFusionPoolDooot, PsDefiTunaLendingPositionDooot, PsDefiTunaMarketDooot,
        PsDefiTunaTunaPositionDooot, PsDefiTunaVaultDooot,
    },
    trait_defs::transaction::TransactionLike,
    utils::{step_utils::StepUtils, util_fns::convert_blocktime_to_micros},
};

pub const DEFITUNA_PROGRAM_ID: &str = "tuna4uSQZncNeeiAMKbstuxA9CUkHH6HmC64wgmnogD";

const FUSION_POOL_DISCRIMINATOR: &[u8] = &[254, 204, 207, 98, 25, 181, 29, 67];
const LENDING_POSITION_DISCRIMINATOR: &[u8] = &[47, 255, 252, 35, 20, 245, 157, 243];
const MARKET_DISCRIMINATOR: &[u8] = &[219, 190, 213, 55, 0, 227, 198, 154];
const TUNA_POSITION_DISCRIMINATOR: &[u8] = &[0x4c, 0xc5, 0xa1, 0x33, 0xe8, 0x0f, 0x89, 0xdc];
const VAULT_DISCRIMINATOR: &[u8] = &[0xd3, 0x08, 0xe8, 0x2b, 0x02, 0x98, 0x75, 0x77];

pub struct DefiTuna;

#[async_trait]
impl TransactionParserTrait for DefiTuna {
    fn get_name(&self) -> &'static str {
        "DefiTuna"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            DEFITUNA_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let acct_iter = txn.iter_accounts().filter(|a| {
            (a.pre_owner_program == Some(DEFITUNA_PROGRAM_ID)
                || a.post_owner_program == Some(DEFITUNA_PROGRAM_ID))
                && a.datum_changed
        });

        for acct in acct_iter {
            let pre_data = acct.unpack_pre_datum()?.map(|d| (d.is_empty(), d));
            let post_data = acct.unpack_post_datum()?.map(|d| (d.is_empty(), d));

            let (closed, data) = match (pre_data, post_data) {
                (_, Some((false, post))) => (false, post),
                (Some((false, pre)), None | Some((true, _))) => (true, pre),
                _ => continue,
            };

            let Some(disc) = data.get(..8) else {
                continue;
            };

            match disc {
                FUSION_POOL_DISCRIMINATOR => {
                    let pool = FusionPool::deserialize(&mut &data[8..])?;
                    let dooot = Dooot::PsDefiTunaFusionPool(PsDefiTunaFusionPoolDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        bump: pool.bump,
                        version: pool.version,
                        token_mint_a: pool.token_mint_a.to_string(),
                        token_mint_b: pool.token_mint_b.to_string(),
                        token_vault_a: pool.token_vault_a.to_string(),
                        token_vault_b: pool.token_vault_b.to_string(),
                        tick_spacing: pool.tick_spacing,
                        tick_spacing_seed: pool.tick_spacing_seed,
                        fee_rate: pool.fee_rate,
                        protocol_fee_rate: pool.protocol_fee_rate,
                        clp_to_olp_reward_ratio: pool.clp_to_olp_reward_ratio,
                        order_protocol_fee_rate: pool.order_protocol_fee_rate,
                        liquidity: pool.liquidity.to_string(),
                        sqrt_price: pool.sqrt_price.to_string(),
                        tick_current_index: pool.tick_current_index,
                        protocol_fee_owed_a: pool.protocol_fee_owed_a,
                        protocol_fee_owed_b: pool.protocol_fee_owed_b,
                        fee_growth_global_a: pool.fee_growth_global_a.to_string(),
                        fee_growth_global_b: pool.fee_growth_global_b.to_string(),
                        orders_total_amount_a: pool.orders_total_amount_a,
                        orders_total_amount_b: pool.orders_total_amount_b,
                        orders_filled_amount_a: pool.orders_filled_amount_a,
                        orders_filled_amount_b: pool.orders_filled_amount_b,
                        olp_fee_owed_a: pool.olp_fee_owed_a,
                        olp_fee_owed_b: pool.olp_fee_owed_b,
                        closed,
                    });

                    dooots.push(dooot);
                }
                LENDING_POSITION_DISCRIMINATOR => {
                    let pos = LendingPosition::deserialize(&mut &data[8..])?;
                    let dooot = Dooot::PsDefiTunaLendingPosition(PsDefiTunaLendingPositionDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        version: pos.version,
                        bump: pos.bump,
                        authority: pos.authority.to_string(),
                        pool_mint: pos.pool_mint.to_string(),
                        deposited_funds: pos.deposited_funds,
                        deposited_shares: pos.deposited_shares,
                        closed,
                    });

                    dooots.push(dooot);
                }
                MARKET_DISCRIMINATOR => {
                    let pos = Market::deserialize(&mut &data[8..])?;
                    let dooot = Dooot::PsDefiTunaMarket(PsDefiTunaMarketDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        version: pos.version,
                        bump: pos.bump,
                        market_maker: pos.market_maker.to_string(),
                        pool: pos.pool.to_string(),
                        address_lookup_table: pos.address_lookup_table.to_string(),
                        max_leverage: pos.max_leverage,
                        protocol_fee: pos.protocol_fee,
                        protocol_fee_on_collateral: pos.protocol_fee_on_collateral,
                        liquidation_fee: pos.liquidation_fee,
                        liquidation_threshold: pos.liquidation_threshold,
                        limit_order_execution_fee: pos.limit_order_execution_fee,
                        oracle_price_deviation_threshold: pos.oracle_price_deviation_threshold,
                        disabled: pos.disabled,
                        borrowed_shares_a: pos.borrowed_shares_a,
                        borrowed_shares_b: pos.borrowed_shares_b,
                        borrow_limit_a: pos.borrow_limit_a,
                        borrow_limit_b: pos.borrow_limit_b,
                        max_swap_slippage: pos.max_swap_slippage,
                        closed,
                    });

                    dooots.push(dooot);
                }
                TUNA_POSITION_DISCRIMINATOR => {
                    let pos = TunaPosition::deserialize(&mut &data[8..])?;
                    let dooot = Dooot::PsDefiTunaTunaPosition(PsDefiTunaTunaPositionDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        version: pos.version,
                        bump: pos.bump,
                        authority: pos.authority.to_string(),
                        pool: pos.pool.to_string(),
                        mint_a: pos.mint_a.to_string(),
                        mint_b: pos.mint_b.to_string(),
                        position_mint: pos.position_mint.to_string(),
                        liquidity: pos.liquidity.to_string(),
                        tick_lower_index: pos.tick_lower_index,
                        tick_upper_index: pos.tick_upper_index,
                        loan_shares_a: pos.loan_shares_a,
                        loan_shares_b: pos.loan_shares_b,
                        loan_funds_a: pos.loan_funds_a,
                        loan_funds_b: pos.loan_funds_b,
                        leftovers_a: pos.leftovers_a,
                        leftovers_b: pos.leftovers_b,
                        tick_entry_index: pos.tick_entry_index,
                        tick_stop_loss_index: pos.tick_stop_loss_index,
                        tick_take_profit_index: pos.tick_take_profit_index,
                        state: pos.state.to_string(),
                        swap_to_token_on_limit_order: pos.swap_to_token_on_limit_order,
                        compounded_yield_a: pos.compounded_yield_a,
                        compounded_yield_b: pos.compounded_yield_b,
                        flags: pos.flags,
                        market_maker: pos.market_maker.to_string(),
                        closed,
                    });

                    dooots.push(dooot);
                }
                VAULT_DISCRIMINATOR => {
                    let vault = Vault::deserialize(&mut &data[8..])?;
                    let dooot = Dooot::PsDefiTunaVault(PsDefiTunaVaultDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        version: vault.version,
                        bump: vault.bump,
                        mint: vault.mint.to_string(),
                        deposited_funds: vault.deposited_funds,
                        deposited_shares: vault.deposited_shares,
                        borrowed_funds: vault.borrowed_funds,
                        borrowed_shares: vault.borrowed_shares,
                        unpaid_debt_shares: vault.unpaid_debt_shares,
                        interest_rate: vault.interest_rate,
                        last_update_timestamp: vault.last_update_timestamp,
                        supply_limit: vault.supply_limit,
                        pyth_oracle_price_update: vault.pyth_oracle_price_update.to_string(),
                        pyth_oracle_feed_id: vault.pyth_oracle_feed_id.to_string(),
                        closed,
                    });

                    dooots.push(dooot);
                }
                _ => continue,
            }
        }

        Ok(())
    }
}

impl StateSource for DefiTuna {
    fn get_name(&self) -> &'static str {
        "DefiTuna"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let block_time = acct_provider.get_block_time();
        let time = convert_blocktime_to_micros(block_time, Some(0), None)?;
        let p_acc_iter =
            acct_provider.program_accounts(Pubkey::from_str(DEFITUNA_PROGRAM_ID)?, None);

        for acc in p_acc_iter {
            let data = acc.account.data();
            let Some(disc) = data.get(..8) else {
                continue;
            };

            match disc {
                FUSION_POOL_DISCRIMINATOR => {
                    let Ok(pool) = FusionPool::deserialize(&mut &data[8..]) else {
                        continue;
                    };
                    let dooot = Dooot::PsDefiTunaFusionPool(PsDefiTunaFusionPoolDooot {
                        time,
                        account_pubkey: acc.pubkey.to_string(),
                        version: pool.version,
                        bump: pool.bump,
                        token_mint_a: pool.token_mint_a.to_string(),
                        token_mint_b: pool.token_mint_b.to_string(),
                        token_vault_a: pool.token_vault_a.to_string(),
                        token_vault_b: pool.token_vault_b.to_string(),
                        tick_spacing: pool.tick_spacing,
                        tick_spacing_seed: pool.tick_spacing_seed,
                        fee_rate: pool.fee_rate,
                        protocol_fee_rate: pool.protocol_fee_rate,
                        clp_to_olp_reward_ratio: pool.clp_to_olp_reward_ratio,
                        order_protocol_fee_rate: pool.order_protocol_fee_rate,
                        liquidity: pool.liquidity.to_string(),
                        sqrt_price: pool.sqrt_price.to_string(),
                        tick_current_index: pool.tick_current_index,
                        protocol_fee_owed_a: pool.protocol_fee_owed_a,
                        protocol_fee_owed_b: pool.protocol_fee_owed_b,
                        fee_growth_global_a: pool.fee_growth_global_a.to_string(),
                        fee_growth_global_b: pool.fee_growth_global_b.to_string(),
                        orders_total_amount_a: pool.orders_total_amount_a,
                        orders_total_amount_b: pool.orders_total_amount_b,
                        orders_filled_amount_a: pool.orders_filled_amount_a,
                        orders_filled_amount_b: pool.orders_filled_amount_b,
                        olp_fee_owed_a: pool.olp_fee_owed_a,
                        olp_fee_owed_b: pool.olp_fee_owed_b,
                        closed: false,
                    });

                    tx.send(dooot)?;
                }
                LENDING_POSITION_DISCRIMINATOR => {
                    let Ok(pos) = LendingPosition::deserialize(&mut &data[8..]) else {
                        continue;
                    };
                    let dooot = Dooot::PsDefiTunaLendingPosition(PsDefiTunaLendingPositionDooot {
                        time,
                        account_pubkey: acc.pubkey.to_string(),
                        version: pos.version,
                        bump: pos.bump,
                        authority: pos.authority.to_string(),
                        pool_mint: pos.pool_mint.to_string(),
                        deposited_funds: pos.deposited_funds,
                        deposited_shares: pos.deposited_shares,
                        closed: false,
                    });

                    tx.send(dooot)?;
                }
                MARKET_DISCRIMINATOR => {
                    let Ok(pos) = Market::deserialize(&mut &data[8..]) else {
                        continue;
                    };
                    let dooot = Dooot::PsDefiTunaMarket(PsDefiTunaMarketDooot {
                        time,
                        account_pubkey: acc.pubkey.to_string(),
                        version: pos.version,
                        bump: pos.bump,
                        market_maker: pos.market_maker.to_string(),
                        pool: pos.pool.to_string(),
                        address_lookup_table: pos.address_lookup_table.to_string(),
                        max_leverage: pos.max_leverage,
                        protocol_fee: pos.protocol_fee,
                        protocol_fee_on_collateral: pos.protocol_fee_on_collateral,
                        liquidation_fee: pos.liquidation_fee,
                        liquidation_threshold: pos.liquidation_threshold,
                        limit_order_execution_fee: pos.limit_order_execution_fee,
                        oracle_price_deviation_threshold: pos.oracle_price_deviation_threshold,
                        disabled: pos.disabled,
                        max_swap_slippage: pos.max_swap_slippage,
                        borrow_limit_a: pos.borrow_limit_a,
                        borrow_limit_b: pos.borrow_limit_b,
                        borrowed_shares_a: pos.borrowed_shares_a,
                        borrowed_shares_b: pos.borrowed_shares_b,
                        closed: false,
                    });

                    tx.send(dooot)?;
                }
                TUNA_POSITION_DISCRIMINATOR => {
                    let Ok(pos) = TunaPosition::deserialize(&mut &data[8..]) else {
                        continue;
                    };
                    let dooot = Dooot::PsDefiTunaTunaPosition(PsDefiTunaTunaPositionDooot {
                        time,
                        account_pubkey: acc.pubkey.to_string(),
                        version: pos.version,
                        bump: pos.bump,
                        authority: pos.authority.to_string(),
                        pool: pos.pool.to_string(),
                        mint_a: pos.mint_a.to_string(),
                        mint_b: pos.mint_b.to_string(),
                        position_mint: pos.position_mint.to_string(),
                        liquidity: pos.liquidity.to_string(),
                        tick_lower_index: pos.tick_lower_index,
                        tick_upper_index: pos.tick_upper_index,
                        loan_shares_a: pos.loan_shares_a,
                        loan_shares_b: pos.loan_shares_b,
                        loan_funds_a: pos.loan_funds_a,
                        loan_funds_b: pos.loan_funds_b,
                        leftovers_a: pos.leftovers_a,
                        leftovers_b: pos.leftovers_b,
                        tick_entry_index: pos.tick_entry_index,
                        tick_stop_loss_index: pos.tick_stop_loss_index,
                        tick_take_profit_index: pos.tick_take_profit_index,
                        state: pos.state.to_string(),
                        swap_to_token_on_limit_order: pos.swap_to_token_on_limit_order,
                        compounded_yield_a: pos.compounded_yield_a,
                        compounded_yield_b: pos.compounded_yield_b,
                        flags: pos.flags,
                        market_maker: pos.market_maker.to_string(),
                        closed: false,
                    });

                    tx.send(dooot)?;
                }
                VAULT_DISCRIMINATOR => {
                    let Ok(vault) = Vault::deserialize(&mut &data[8..]) else {
                        continue;
                    };
                    let dooot = Dooot::PsDefiTunaVault(PsDefiTunaVaultDooot {
                        time,
                        account_pubkey: acc.pubkey.to_string(),
                        version: vault.version,
                        bump: vault.bump,
                        mint: vault.mint.to_string(),
                        deposited_funds: vault.deposited_funds,
                        deposited_shares: vault.deposited_shares,
                        borrowed_funds: vault.borrowed_funds,
                        borrowed_shares: vault.borrowed_shares,
                        unpaid_debt_shares: vault.unpaid_debt_shares,
                        interest_rate: vault.interest_rate,
                        last_update_timestamp: vault.last_update_timestamp,
                        supply_limit: vault.supply_limit,
                        pyth_oracle_price_update: vault.pyth_oracle_price_update.to_string(),
                        pyth_oracle_feed_id: vault.pyth_oracle_feed_id.to_string(),
                        closed: false,
                    });

                    tx.send(dooot)?;
                }
                _ => continue,
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use step_ingestooor_sdk::{
        decoded_transaction::DecodedTransaction, utils::test_utils::init_test_extras_bare,
    };

    use crate::{margin::defi_tuna::DefiTuna, TransactionParserTrait};

    #[tokio::test]
    async fn lending_deposit() {
        let file = fs::read_to_string("./tests/defituna/deposit_lend.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let (step_utils, _) = init_test_extras_bare().await;

        let parser = DefiTuna {};
        parser.invoke(&txn, &mut vec![], &step_utils).await.unwrap();
    }

    #[tokio::test]
    async fn position_update() {
        let file = fs::read_to_string("./tests/defituna/claim_pos_rewards.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let (step_utils, _) = init_test_extras_bare().await;

        let parser = DefiTuna {};
        parser.invoke(&txn, &mut vec![], &step_utils).await.unwrap();
    }
}
