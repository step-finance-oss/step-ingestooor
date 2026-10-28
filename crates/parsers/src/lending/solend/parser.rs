use crate::lending::solend::state::{ObligationCollateral, ObligationLiquidity, Reserve};
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::mpsc::SyncSender,
};

use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
use step_ingestooor_sdk::{
    dooot::{Dooot, LendingGlobalDooot, LendingUserDooot, MintUnderlyingsGlobalDooot},
    trait_defs::transaction::TransactionLike,
    utils::{
        constants::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID},
        step_utils::StepUtils,
        util_fns::{check_token_account_type, convert_blocktime_to_micros, TokenAccountType},
    },
};

use crate::{
    state_source::{AccountProvider, StateSource},
    TransactionParserFilter, TransactionParserTrait,
};

use super::state::Obligation;

/// structure, impl functions and the other files that exist in solend has taken from this repo -> https://github.com/solendprotocol/solana-program-library/tree/mainnet/token-lending/sdk/src
pub struct Solend;
pub const SOLEND_PROTOCOL_ID: &str = "So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo";
pub const RESERVE_LEN: usize = 619;
pub const OBLIGATION_LEN: usize = 1300;

#[async_trait]
impl TransactionParserTrait for Solend {
    fn get_name(&self) -> &'static str {
        "Solend"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            SOLEND_PROTOCOL_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = decoded_transaction.get_block_time();

        let mut reserves: HashMap<String, Reserve> = HashMap::new();

        for acct in decoded_transaction.iter_accounts() {
            let pre = acct.unpack_pre_datum()?;
            let post = acct.unpack_post_datum()?;

            let account_length = post
                .as_ref()
                .map(|p| p.len())
                .unwrap_or(pre.as_ref().map(|p| p.len()).unwrap_or(0));

            match account_length {
                RESERVE_LEN => {
                    let Some(pre) = pre else {
                        continue;
                    };
                    let Some(post) = post else {
                        continue;
                    };
                    let reserve_pre = Reserve::unpack_from_slice(&pre)?;
                    let pre_supply = reserve_pre.liquidity.available_amount;
                    let reserve_post = Reserve::unpack_from_slice(&post)?;
                    let post_supply = reserve_post.liquidity.available_amount;
                    reserves.insert(acct.pubkey.to_string(), reserve_post.clone());

                    if pre_supply != post_supply {
                        let ctoken_mint = reserve_post.collateral.mint_pubkey;
                        let underlying_mint = reserve_post.liquidity.mint_pubkey;
                        let total_liq = reserve_post.total_liq()?;
                        let total_coll = reserve_post.collateral.mint_total_supply;
                        let amt_per_parent = if total_coll == 0 {
                            1.0
                        } else {
                            (total_liq as f64) / (total_coll as f64)
                        };

                        let mint_underlying_dooot =
                            Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                                time: decoded_transaction.get_block_time(),
                                mint_pubkey: ctoken_mint.to_string(),
                                platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                                mints: vec![underlying_mint.to_string()],
                                mints_qty_per_one_parent: vec![amt_per_parent],
                                total_underlying_amounts: vec![total_liq.into()],
                                discriminant_id: acct.pubkey.to_string(),
                            });
                        dooot.push(mint_underlying_dooot);

                        let global_doot = Dooot::LendingGlobal(LendingGlobalDooot {
                            time,
                            mint: underlying_mint.to_string(),
                            platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                            total_deposits: Decimal::from(
                                reserve_post.collateral.mint_total_supply,
                            ),
                            total_borrows: Decimal::from(
                                reserve_post
                                    .liquidity
                                    .borrowed_amount_wads
                                    .try_round_u64()?,
                            ),
                            supply_apr: reserve_post.calc_supply_apr()?.to_f64().unwrap_or(0.0),
                            borrow_apr: reserve_post.calc_borrow_apr()?.to_f64().unwrap_or(0.0),
                            pool_pubkey: acct.pubkey.to_string(),
                            supply_value_ratio: None,
                            borrow_value_ratio: None,
                        });

                        dooot.push(global_doot);
                    }
                }
                _ => continue,
            }
        }

        for acct in decoded_transaction.iter_accounts() {
            // skip reserves we already processed
            if reserves.contains_key(acct.pubkey) {
                continue;
            }
            let pre = acct.unpack_pre_datum()?;
            let post = acct.unpack_post_datum()?;

            let account_length = post
                .as_ref()
                .map(|p| p.len())
                .unwrap_or(pre.as_ref().map(|p| p.len()).unwrap_or(0));

            match account_length {
                OBLIGATION_LEN => {
                    let pre_acct_data = acct.unpack_pre_datum()?;
                    let post_acct_data = acct.unpack_post_datum()?;

                    match (pre_acct_data, post_acct_data) {
                        // both pre and post are Some so lets diff them to not resave the same unchanged deposits or borrows
                        (Some(pre_acct_data), Some(post_acct_data)) => {
                            let obligation_pre = Obligation::unpack_from_slice(&pre_acct_data)?;
                            let obligation_post = Obligation::unpack_from_slice(&post_acct_data)?;

                            let mut pre_deposits: HashMap<String, &ObligationCollateral> =
                                HashMap::new();
                            let mut post_deposits: HashMap<String, &ObligationCollateral> =
                                HashMap::new();
                            let mut all_deposits = HashSet::new();

                            let mut pre_borrows: HashMap<String, &ObligationLiquidity> =
                                HashMap::new();
                            let mut post_borrows: HashMap<String, &ObligationLiquidity> =
                                HashMap::new();
                            let mut all_borrows = HashSet::new();

                            for deposit in &obligation_pre.deposits {
                                // these checks are to skip adding reserves from positions that didnt change this txn
                                let Some(_) = reserves.get(&deposit.deposit_reserve.to_string())
                                else {
                                    continue;
                                };
                                pre_deposits.insert(deposit.deposit_reserve.to_string(), deposit);
                            }
                            for deposit in &obligation_post.deposits {
                                let Some(_) = reserves.get(&deposit.deposit_reserve.to_string())
                                else {
                                    continue;
                                };
                                post_deposits.insert(deposit.deposit_reserve.to_string(), deposit);
                            }
                            for borrow in &obligation_pre.borrows {
                                let Some(_) = reserves.get(&borrow.borrow_reserve.to_string())
                                else {
                                    continue;
                                };
                                pre_borrows.insert(borrow.borrow_reserve.to_string(), borrow);
                            }
                            for borrow in &obligation_post.borrows {
                                let Some(_) = reserves.get(&borrow.borrow_reserve.to_string())
                                else {
                                    continue;
                                };
                                post_borrows.insert(borrow.borrow_reserve.to_string(), borrow);
                            }

                            all_deposits.extend(pre_deposits.keys());
                            all_deposits.extend(post_deposits.keys());
                            all_borrows.extend(pre_borrows.keys());
                            all_borrows.extend(post_borrows.keys());

                            for reserve in all_deposits {
                                match (pre_deposits.get(reserve), post_deposits.get(reserve)) {
                                    // Same Position exists in both pre and post
                                    (Some(pre_deposit), Some(post_deposit)) => {
                                        let Some(reserve_data) = reserves.get(reserve) else {
                                            continue;
                                        };
                                        let pre_amount = pre_deposit.deposited_amount;
                                        let post_amount = post_deposit.deposited_amount;
                                        // check if amount changed
                                        if pre_amount != post_amount {
                                            let deposit_doot =
                                                Dooot::LendingUser(LendingUserDooot {
                                                    borrows: Decimal::ZERO,
                                                    deposits: Decimal::from(post_amount),
                                                    mint: reserve_data
                                                        .collateral
                                                        .mint_pubkey
                                                        .to_string(),
                                                    platform_program_pubkey: SOLEND_PROTOCOL_ID
                                                        .to_string(),
                                                    user_pubkey: obligation_post.owner.to_string(),
                                                    time,
                                                    pool_pubkey: reserve.to_string(),
                                                    borrow_shares: None,
                                                    deposit_shares: None,
                                                    lending_address_pubkey: None,
                                                });
                                            dooot.push(deposit_doot);
                                        }
                                    }
                                    // Position exists in pre but not in post
                                    (Some(_), None) => {
                                        let Some(reserve_data) = reserves.get(reserve) else {
                                            continue;
                                        };
                                        let deposit_doot = Dooot::LendingUser(LendingUserDooot {
                                            borrows: Decimal::ZERO,
                                            deposits: Decimal::ZERO,
                                            mint: reserve_data.collateral.mint_pubkey.to_string(),
                                            platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                                            user_pubkey: obligation_post.owner.to_string(),
                                            time,
                                            pool_pubkey: reserve.to_string(),
                                            borrow_shares: None,
                                            deposit_shares: None,
                                            lending_address_pubkey: None,
                                        });
                                        dooot.push(deposit_doot);
                                    }
                                    // Position exists in post but not in pre
                                    (None, Some(post_deposit)) => {
                                        let Some(reserve_data) = reserves.get(reserve) else {
                                            continue;
                                        };
                                        let deposit_doot = Dooot::LendingUser(LendingUserDooot {
                                            borrows: Decimal::ZERO,
                                            deposits: Decimal::from(post_deposit.deposited_amount),
                                            mint: reserve_data.collateral.mint_pubkey.to_string(),
                                            platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                                            user_pubkey: obligation_post.owner.to_string(),
                                            time,
                                            pool_pubkey: reserve.to_string(),
                                            borrow_shares: None,
                                            deposit_shares: None,
                                            lending_address_pubkey: None,
                                        });
                                        dooot.push(deposit_doot);
                                    }
                                    _ => continue,
                                }
                            }
                            for reserve in all_borrows {
                                match (pre_borrows.get(reserve), post_borrows.get(reserve)) {
                                    // Same Position exists in both pre and post
                                    (Some(pre_borrow), Some(post_borrow)) => {
                                        let Some(reserve_data) = reserves.get(reserve) else {
                                            continue;
                                        };
                                        let pre_amount = pre_borrow.borrowed_amount_wads;
                                        let post_amount = post_borrow.borrowed_amount_wads;
                                        // check if amount changed
                                        if pre_amount != post_amount {
                                            let borrow_doot =
                                                Dooot::LendingUser(LendingUserDooot {
                                                    borrows: Decimal::from(
                                                        post_amount.try_round_u64()?,
                                                    ),
                                                    deposits: Decimal::ZERO,
                                                    mint: reserve_data
                                                        .liquidity
                                                        .mint_pubkey
                                                        .to_string(),
                                                    platform_program_pubkey: SOLEND_PROTOCOL_ID
                                                        .to_string(),
                                                    user_pubkey: obligation_post.owner.to_string(),
                                                    time,
                                                    pool_pubkey: reserve.to_string(),
                                                    borrow_shares: None,
                                                    deposit_shares: None,
                                                    lending_address_pubkey: None,
                                                });
                                            dooot.push(borrow_doot);
                                        }
                                    }
                                    //Position exists in pre but not in post
                                    (Some(_), None) => {
                                        let Some(reserve_data) = reserves.get(reserve) else {
                                            continue;
                                        };
                                        let borrow_doot = Dooot::LendingUser(LendingUserDooot {
                                            borrows: Decimal::ZERO,
                                            deposits: Decimal::ZERO,
                                            mint: reserve_data.liquidity.mint_pubkey.to_string(),
                                            platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                                            user_pubkey: obligation_pre.owner.to_string(),
                                            time,
                                            pool_pubkey: reserve.to_string(),
                                            borrow_shares: None,
                                            deposit_shares: None,
                                            lending_address_pubkey: None,
                                        });
                                        dooot.push(borrow_doot);
                                    }
                                    //Position exists in post but not in pre
                                    (None, Some(post_borrow)) => {
                                        let Some(reserve_data) = reserves.get(reserve) else {
                                            continue;
                                        };
                                        let borrow_doot = Dooot::LendingUser(LendingUserDooot {
                                            borrows: Decimal::from(
                                                post_borrow.borrowed_amount_wads.try_round_u64()?,
                                            ),
                                            deposits: Decimal::ZERO,
                                            mint: reserve_data.liquidity.mint_pubkey.to_string(),
                                            platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                                            user_pubkey: obligation_post.owner.to_string(),
                                            time,
                                            pool_pubkey: reserve.to_string(),
                                            borrow_shares: None,
                                            deposit_shares: None,
                                            lending_address_pubkey: None,
                                        });
                                        dooot.push(borrow_doot);
                                    }

                                    _ => continue,
                                }
                            }
                        }
                        // No post data on the entire Obligation at all. Closed
                        (Some(pre_acct_data), None) => {
                            let obligation_pre = Obligation::unpack_from_slice(&pre_acct_data)?;
                            for deposit in &obligation_pre.deposits {
                                let Some(reserve_data) =
                                    reserves.get(&deposit.deposit_reserve.to_string())
                                else {
                                    continue;
                                };
                                let mint = reserve_data.collateral.mint_pubkey.to_string();

                                let deposit_doot = Dooot::LendingUser(LendingUserDooot {
                                    borrows: Decimal::ZERO,
                                    deposits: Decimal::ZERO,
                                    mint,
                                    platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                                    user_pubkey: obligation_pre.owner.to_string(),
                                    time,
                                    pool_pubkey: deposit.deposit_reserve.to_string(),
                                    borrow_shares: None,
                                    deposit_shares: None,
                                    lending_address_pubkey: None,
                                });

                                dooot.push(deposit_doot);
                            }
                            for borrow in &obligation_pre.borrows {
                                let Some(reserve_data) =
                                    reserves.get(&borrow.borrow_reserve.to_string())
                                else {
                                    continue;
                                };
                                let mint = reserve_data.liquidity.mint_pubkey.to_string();

                                let borrow_doot = Dooot::LendingUser(LendingUserDooot {
                                    borrows: Decimal::from(
                                        borrow.borrowed_amount_wads.try_round_u64()?,
                                    ),
                                    deposits: Decimal::ZERO,
                                    mint,
                                    platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                                    user_pubkey: obligation_pre.owner.to_string(),
                                    time,
                                    pool_pubkey: borrow.borrow_reserve.to_string(),
                                    borrow_shares: None,
                                    deposit_shares: None,
                                    lending_address_pubkey: None,
                                });

                                dooot.push(borrow_doot);
                            }
                        }
                        // No pre data on the entire Obligation at all. Created
                        (None, Some(post_acct_data)) => {
                            let obligation_post = Obligation::unpack_from_slice(&post_acct_data)?;
                            for deposit in &obligation_post.deposits {
                                let Some(reserve_data) =
                                    reserves.get(&deposit.deposit_reserve.to_string())
                                else {
                                    continue;
                                };
                                let mint = reserve_data.collateral.mint_pubkey.to_string();

                                let deposit_doot = Dooot::LendingUser(LendingUserDooot {
                                    borrows: Decimal::ZERO,
                                    deposits: Decimal::from(deposit.deposited_amount),
                                    mint,
                                    platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                                    user_pubkey: obligation_post.owner.to_string(),
                                    time,
                                    pool_pubkey: deposit.deposit_reserve.to_string(),
                                    borrow_shares: None,
                                    deposit_shares: None,
                                    lending_address_pubkey: None,
                                });

                                dooot.push(deposit_doot);
                            }
                            for borrow in &obligation_post.borrows {
                                let Some(reserve_data) =
                                    reserves.get(&borrow.borrow_reserve.to_string())
                                else {
                                    continue;
                                };
                                let mint = reserve_data.liquidity.mint_pubkey.to_string();

                                let borrow_doot = Dooot::LendingUser(LendingUserDooot {
                                    borrows: Decimal::from(
                                        borrow.borrowed_amount_wads.try_round_u64()?,
                                    ),
                                    deposits: Decimal::ZERO,
                                    mint,
                                    platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                                    user_pubkey: obligation_post.owner.to_string(),
                                    time,
                                    pool_pubkey: borrow.borrow_reserve.to_string(),
                                    borrow_shares: None,
                                    deposit_shares: None,
                                    lending_address_pubkey: None,
                                });

                                dooot.push(borrow_doot);
                            }
                        }
                        _ => {
                            continue;
                        }
                    }
                }
                _ => continue,
            }
        }
        Ok(())
    }
}

impl StateSource for Solend {
    fn get_name(&self) -> &'static str {
        "Solend"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        let solend_protocol_pubkey = Pubkey::from_str(SOLEND_PROTOCOL_ID)?;
        let solend_main_pool_vault_pk =
            Pubkey::from_str("DdZR6zRFiUt4S5mg7AV1uKB2z1f1WzcNYCaTEEWPAuby")?;
        let mut reserve_mint_map = HashMap::new();
        let mut reserve_map = HashMap::new();
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        for account in acct_provider.all_accounts() {
            if account.account.owner() != &solend_protocol_pubkey {
                continue;
            }

            let data: &[u8] = account.account.data();

            if data.len() != RESERVE_LEN {
                continue;
            }

            let reserve = match Reserve::unpack_from_slice(data) {
                Ok(reserve) => reserve,
                Err(e) => {
                    log::error!(
                        "Failed to deserialize Reserve for account {}: {}",
                        account.pubkey,
                        e
                    );
                    continue;
                }
            };

            let mint_pubkey = reserve.collateral.supply_pubkey;
            reserve_mint_map.insert(mint_pubkey, (account.pubkey, reserve.clone()));
            reserve_map.insert(account.pubkey, (account.pubkey, reserve));
        }

        let token_program_pubkey = match Pubkey::from_str(TOKEN_PROGRAM_ID) {
            Ok(pubkey) => pubkey,
            Err(_e) => {
                return Err(anyhow::anyhow!("Invalid token program pubkey"));
            }
        };

        let token_2022_program_pubkey = match Pubkey::from_str(TOKEN_2022_PROGRAM_ID) {
            Ok(pubkey) => pubkey,
            Err(_e) => {
                return Err(anyhow::anyhow!("Invalid token 2022 program pubkey"));
            }
        };

        for acct in acct_provider.all_accounts() {
            if acct.account.owner() == &solend_protocol_pubkey {
                let data = acct.account.data();
                if data.len() == OBLIGATION_LEN {
                    let obligation_post =
                        Obligation::unpack_from_slice(data).expect("failed to unpack obligation");
                    for deposit in &obligation_post.deposits {
                        let Some(reserve) = reserve_map.get(&deposit.deposit_reserve) else {
                            continue;
                        };
                        let mint = reserve.1.collateral.mint_pubkey.to_string();

                        let deposit_dooot = Dooot::LendingUser(LendingUserDooot {
                            borrows: Decimal::ZERO,
                            deposits: Decimal::from(deposit.deposited_amount),
                            mint,
                            platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                            user_pubkey: obligation_post.owner.to_string(),
                            time,
                            pool_pubkey: reserve.0.to_string(),
                            borrow_shares: None,
                            deposit_shares: None,
                            lending_address_pubkey: None,
                        });
                        tx.send(deposit_dooot)?;
                    }

                    for borrow in &obligation_post.borrows {
                        let Some(reserve) = reserve_map.get(&borrow.borrow_reserve) else {
                            continue;
                        };
                        let mint = reserve.1.liquidity.mint_pubkey.to_string();

                        let borrow_dooot = Dooot::LendingUser(LendingUserDooot {
                            borrows: Decimal::from(borrow.borrowed_amount_wads.try_round_u64()?),
                            deposits: Decimal::ZERO,
                            mint,
                            platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                            user_pubkey: obligation_post.owner.to_string(),
                            time,
                            pool_pubkey: reserve.0.to_string(),
                            borrow_shares: None,
                            deposit_shares: None,
                            lending_address_pubkey: None,
                        });
                        tx.send(borrow_dooot)?;
                    }
                }
            }

            if (acct.account.owner() == &token_program_pubkey
                || acct.account.owner() == &token_2022_program_pubkey)
                && reserve_mint_map.contains_key(&acct.pubkey)
            {
                let data = acct.account.data();

                let token_program_id = if acct.account.owner() == &token_program_pubkey {
                    TOKEN_PROGRAM_ID
                } else {
                    TOKEN_2022_PROGRAM_ID
                };

                let token_account_type = match check_token_account_type(token_program_id, data) {
                    Some(t) => t,
                    None => continue,
                };

                match token_account_type {
                    TokenAccountType::TokenKegTokenAccount
                    | TokenAccountType::TokenT22TokenAccount
                    | TokenAccountType::TokenT22TokenAccountWithExtensions => {
                        let owner_pubkey = match Pubkey::try_from(&data[32..64]) {
                            Ok(pubkey) => pubkey,
                            Err(e) => {
                                log::error!("Failed to parse owner pubkey: {}", e);
                                continue;
                            }
                        };

                        if owner_pubkey != solend_main_pool_vault_pk {
                            continue;
                        }

                        let (reserve_pubkey, reserve) = match reserve_mint_map.get(&acct.pubkey) {
                            Some(reserve) => reserve,
                            None => {
                                log::error!("Reserve not found for account {}", acct.pubkey);
                                continue;
                            }
                        };

                        let supply_mint = reserve.liquidity.mint_pubkey;
                        let ctoken_mint = reserve.collateral.mint_pubkey;

                        let Ok(underlying_amt) = reserve.total_liq() else {
                            continue;
                        };

                        let total_c_token = reserve.collateral.mint_total_supply;

                        let amt_per_parent = if total_c_token == 0 {
                            1.0
                        } else {
                            (underlying_amt as f64) / (total_c_token as f64)
                        };

                        let time = match convert_blocktime_to_micros(
                            acct_provider.get_block_time(),
                            Some(0),
                            None,
                        ) {
                            Ok(time) => time,
                            Err(e) => {
                                log::error!("Failed to convert block time: {}", e);
                                continue;
                            }
                        };
                        let mint_underlying_dooot =
                            Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                                time,
                                mint_pubkey: ctoken_mint.to_string(),
                                platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                                mints: vec![supply_mint.to_string()],
                                mints_qty_per_one_parent: vec![amt_per_parent],
                                total_underlying_amounts: vec![underlying_amt.into()],
                                discriminant_id: reserve_pubkey.to_string(),
                            });
                        let _ = tx.send(mint_underlying_dooot);

                        let global_dooot = Dooot::LendingGlobal(LendingGlobalDooot {
                            time,
                            mint: supply_mint.to_string(),
                            platform_program_pubkey: SOLEND_PROTOCOL_ID.to_string(),
                            total_deposits: Decimal::from(reserve.collateral.mint_total_supply),
                            total_borrows: Decimal::from(
                                reserve.liquidity.borrowed_amount_wads.try_round_u64()?,
                            ),
                            supply_apr: reserve.calc_supply_apr()?.to_f64().unwrap_or(0.0),
                            borrow_apr: reserve.calc_borrow_apr()?.to_f64().unwrap_or(0.0),
                            pool_pubkey: acct.pubkey.to_string(),
                            supply_value_ratio: None,
                            borrow_value_ratio: None,
                        });
                        let _ = tx.send(global_dooot);
                    }
                    _ => continue,
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, rc::Rc};
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;

    #[tokio::test]
    async fn invoke_test() {
        let path = "./tests/solend/solend.json";
        let message_json = fs::read_to_string(path).unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = Solend;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn invoke_deposit() {
        let path = "./tests/solend/deposit.json";
        let message_json = fs::read_to_string(path).unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = Solend;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 3);

        let Dooot::LendingUser(dooot_lending) = &dooot[2] else {
            panic!("Expected a lending dooot");
        };
        assert_eq!(dooot_lending.deposits, Decimal::from(12077503));
        assert_eq!(dooot_lending.borrows, Decimal::from(0));
    }

    #[tokio::test]
    async fn invoke_borrow() {
        let path = "./tests/solend/borrow.json";
        let message_json = fs::read_to_string(path).unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = Solend;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 3);
        let Dooot::LendingUser(dooot_lending) = &dooot[2] else {
            panic!("Expected a lending dooot");
        };
        assert_eq!(dooot_lending.deposits, Decimal::from(0));
        assert_eq!(dooot_lending.borrows, Decimal::from(10010000));
    }

    #[tokio::test]
    async fn invoke_repay() {
        let path = "./tests/solend/repay_all.json";
        let message_json = fs::read_to_string(path).unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = Solend;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooot.len(), 3);

        let Dooot::LendingUser(dooot_lending) = &dooot[2] else {
            panic!("Expected a lending dooot");
        };
        // expect 0s because we repaid all borrows and there are no deposits for this market
        assert_eq!(dooot_lending.deposits, Decimal::from(0));
        assert_eq!(dooot_lending.borrows, Decimal::from(0));
    }

    #[tokio::test]
    async fn invoke_withdraw() {
        let path = "./tests/solend/withdraw_all.json";
        let message_json = fs::read_to_string(path).unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = Solend;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooot.len(), 3);

        let Dooot::LendingUser(dooot_lending) = &dooot[2] else {
            panic!("Expected a lending dooot");
        };
        assert_eq!(dooot_lending.deposits, Decimal::from(0));
        assert_eq!(dooot_lending.borrows, Decimal::from(0));
    }

    #[tokio::test]
    async fn invoke_deposit_with_new_obligation_acc() {
        let path = "./tests/solend/deposit_with_new_obligation_acc.json";
        let message_json = fs::read_to_string(path).unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = Solend;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooot.len(), 3);

        let Dooot::LendingUser(dooot_lending) = &dooot[2] else {
            panic!("Expected a lending dooot");
        };
        assert_eq!(dooot_lending.deposits, Decimal::from(804214));
    }
}
