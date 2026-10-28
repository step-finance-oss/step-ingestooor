use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use crate::{state_source::StateSource, TransactionParserFilter, TransactionParserTrait};
use anyhow::Result;
use async_trait::async_trait;
use borsh::BorshDeserialize;
use fixed::types::I80F48;
use rust_decimal::Decimal;
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
use step_ingestooor_idl_gens::marginfi_gen::marginfi::{
    accounts::{Bank, MarginfiAccount},
    types::Balance,
};
use step_ingestooor_sdk::{
    dooot::{lending::LendingGlobalDooot, Dooot, LendingUserDooot},
    trait_defs::transaction::TransactionLike,
    utils::{step_utils::StepUtils, util_fns::convert_blocktime_to_micros},
};

pub const MARGINFI_PROGRAM: &str = "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA";
pub const MARGINFI_ACCOUNT_DISCRIMINATOR: &[u8] = &[0x43, 0xb2, 0x82, 0x6d, 0x7e, 0x72, 0x1c, 0x2a];
pub const MARGINFI_BANK_DISCRIMINATOR: &[u8] = &[0x8e, 0x31, 0xa6, 0xf2, 0x32, 0x42, 0x61, 0xbc];

pub struct Marginfi;

#[async_trait]
impl TransactionParserTrait for Marginfi {
    fn get_name(&self) -> &'static str {
        "Marginfi"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            MARGINFI_PROGRAM.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let accts_iter = txn
            .iter_accounts()
            .filter(|a| a.post_owner_program == Some(MARGINFI_PROGRAM));

        // K = pubkey, V = (BankAccount, changed?)
        let mut banks = HashMap::new();
        let mut user_accounts = HashMap::new();

        for acct in accts_iter {
            let Some(post) = acct.unpack_post_datum()? else {
                log::warn!(
                    "No post datum for MarginFi acct {}, may be closed",
                    acct.pubkey
                );
                continue;
            };

            let Some(disc) = post.get(..8) else {
                continue;
            };

            match disc {
                MARGINFI_BANK_DISCRIMINATOR => {
                    // Record all banks
                    let bank = Bank::deserialize(&mut &post[8..])?;
                    banks.insert(acct.pubkey, (bank, acct.datum_changed));
                }
                MARGINFI_ACCOUNT_DISCRIMINATOR => {
                    // Record only changed user accounts
                    if !acct.datum_changed {
                        continue;
                    }
                    let pre = acct.unpack_pre_datum()?;
                    let post_user = MarginfiAccount::deserialize(&mut &post[8..])?;
                    let pre_user = pre
                        .map(|d| MarginfiAccount::deserialize(&mut &d[8..]))
                        .transpose()?;
                    user_accounts.insert(acct.pubkey, (pre_user, post_user));
                }
                _ => {
                    //unknown, don't care
                    continue;
                }
            }
        }

        // Emit all global dooots
        for (bank, changed) in banks.values() {
            if !changed {
                continue;
            }

            let (total_deposits, total_borrows) = Self::get_bank_token_totals(bank);
            let mint = bank.mint.to_string();

            let (supply_apr, borrow_apr, ..) = bank.get_rates().unwrap_or_default();

            let supply_apr = supply_apr.to_num::<f64>();
            let borrow_apr = borrow_apr.to_num::<f64>();

            let market = bank.group.to_string();

            let global_lending_state = Dooot::LendingGlobal(LendingGlobalDooot {
                mint: mint.clone(),
                platform_program_pubkey: MARGINFI_PROGRAM.to_string(),
                time,
                total_borrows,
                total_deposits,
                borrow_apr,
                supply_apr,
                supply_value_ratio: Some(bank.asset_share_value.try_into()?),
                borrow_value_ratio: Some(bank.liability_share_value.try_into()?),
                pool_pubkey: market.clone(),
            });

            dooot.push(global_lending_state);
        }

        // Emit all user doots
        for (pre_user, post_user) in user_accounts.values() {
            let mut checked_banks = HashSet::new();
            for balance in &post_user.lending_account.balances {
                if !balance.active {
                    continue;
                }

                let bank_pk = balance.bank_pk.to_string();
                let Some((bank, _)) = banks.get(&bank_pk.as_str()) else {
                    // Not involved in this txn
                    continue;
                };

                checked_banks.insert(bank_pk.clone());

                let (deposits, borrows) = Self::get_user_balance_token_amounts(balance, bank);
                let deposit_shares = Some(balance.asset_shares.try_into()?);
                let borrow_shares = Some(balance.liability_shares.try_into()?);

                let user_lending_state = Dooot::LendingUser(LendingUserDooot {
                    borrows,
                    deposits,
                    time,
                    platform_program_pubkey: MARGINFI_PROGRAM.to_string(),
                    mint: bank.mint.to_string(),
                    user_pubkey: post_user.authority.to_string(),
                    borrow_shares,
                    deposit_shares,
                    pool_pubkey: bank.group.to_string(),
                    lending_address_pubkey: None,
                });

                dooot.push(user_lending_state);
            }

            // Now go back to see which banks we missed in pre data, and emit 0's
            let Some(pre_user) = pre_user else {
                continue;
            };

            for balance in pre_user.lending_account.balances {
                if !balance.active {
                    continue;
                }

                if checked_banks.contains(&balance.bank_pk.to_string()) {
                    continue;
                }

                // This balance is not in post data, but is in pre data,
                // User withdrew all, or repaid all, etc.
                // Get bank info, and emit 0's
                let bank_pk = balance.bank_pk.to_string();
                let Some((bank, _)) = banks.get(&bank_pk.as_str()) else {
                    // Not involved in this txn
                    continue;
                };

                let (deposits, borrows) = (Decimal::ZERO, Decimal::ZERO);
                let deposit_shares = Some(Decimal::ZERO);
                let borrow_shares = Some(Decimal::ZERO);

                let user_lending_state = Dooot::LendingUser(LendingUserDooot {
                    borrows,
                    deposits,
                    time,
                    platform_program_pubkey: MARGINFI_PROGRAM.to_string(),
                    mint: bank.mint.to_string(),
                    user_pubkey: post_user.authority.to_string(),
                    borrow_shares,
                    deposit_shares,
                    pool_pubkey: bank.group.to_string(),
                    lending_address_pubkey: None,
                });

                dooot.push(user_lending_state);
            }
        }

        Ok(())
    }
}

impl Marginfi {
    #[inline]
    pub fn get_bank_token_totals(bank: &Bank) -> (Decimal, Decimal) {
        let asset_share_value: I80F48 = bank.asset_share_value.into();
        let liability_share_value: I80F48 = bank.liability_share_value.into();

        let total_asset_shares: I80F48 = bank.total_asset_shares.into();
        let total_liability_shares: I80F48 = bank.total_liability_shares.into();

        let total_asset_shares_val = asset_share_value * total_asset_shares;
        let total_liability_shares_val = liability_share_value * total_liability_shares;

        let total_asset_shares_val = Self::i80f48_to_decimal(total_asset_shares_val);
        let total_liability_shares_val = Self::i80f48_to_decimal(total_liability_shares_val);

        (total_asset_shares_val, total_liability_shares_val)
    }

    #[inline]
    pub fn get_user_balance_token_amounts(
        user_balance: &Balance,
        bank: &Bank,
    ) -> (Decimal, Decimal) {
        let asset_share_value: I80F48 = bank.asset_share_value.into();
        let liability_share_value: I80F48 = bank.liability_share_value.into();

        let user_asset_shares: I80F48 = user_balance.asset_shares.into();
        let user_liability_shares: I80F48 = user_balance.liability_shares.into();

        let user_asset_shares_val = asset_share_value * user_asset_shares;
        let user_liability_shares_val = liability_share_value * user_liability_shares;

        let user_asset_shares_val = Self::i80f48_to_decimal(user_asset_shares_val);
        let user_liability_shares_val = Self::i80f48_to_decimal(user_liability_shares_val);

        (user_asset_shares_val, user_liability_shares_val)
    }

    #[inline]
    pub fn i80f48_to_decimal(value: I80F48) -> Decimal {
        // Shift out the decimal portion of the I80F*48*
        let val = value.to_bits() >> 48;
        Decimal::from(val)
    }

    #[inline]
    pub fn get_user_balance_for_bank<'a>(
        lending_acc: &'a MarginfiAccount,
        bank_pk: &str,
    ) -> Option<&'a Balance> {
        lending_acc
            .lending_account
            .balances
            .iter()
            .find(|bal| bal.bank_pk.to_string() == bank_pk)
    }
}

impl StateSource for Marginfi {
    fn get_name(&self) -> &'static str {
        "Marginfi"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let bt = acct_provider.get_block_time();
        let blocktime = convert_blocktime_to_micros(bt, Some(0), None)?;

        let bank_disc = [0x8e, 0x31, 0xa6, 0xf2, 0x32, 0x42, 0x61, 0xbc];
        let user_acct_disc = [0x43, 0xb2, 0x82, 0x6d, 0x7e, 0x72, 0x1c, 0x2a];

        let all_accts = acct_provider.program_accounts(Pubkey::from_str(MARGINFI_PROGRAM)?, None);
        let mut banks: HashMap<Pubkey, Bank> = HashMap::new();
        let mut user_accts_missing_banks: HashMap<Pubkey, (MarginfiAccount, Vec<Balance>)> =
            HashMap::new();

        for acct in all_accts {
            let data = acct.account.data();
            let key = acct.pubkey;
            let Some(disc_slice) = data.get(0..8) else {
                continue;
            };

            if disc_slice == bank_disc {
                // Bank acct.
                let Ok(bank) = Bank::deserialize(&mut &data[8..]) else {
                    log::warn!("Failed to deserialize Bank {key}");
                    continue;
                };

                banks.insert(key, bank);

                let (total_deposits, total_borrows) = Self::get_bank_token_totals(&bank);
                let (lending_apr, borrow_apr, ..) = bank.get_rates().unwrap_or_default();

                let supply_apr = lending_apr.to_num::<f64>();
                let borrow_apr = borrow_apr.to_num::<f64>();

                let mint = bank.mint.to_string();

                let market = bank.group.to_string();

                let global_lending_state = Dooot::LendingGlobal(LendingGlobalDooot {
                    mint: mint.clone(),
                    platform_program_pubkey: MARGINFI_PROGRAM.to_string(),
                    time: blocktime,
                    total_borrows,
                    total_deposits,
                    borrow_apr,
                    supply_apr,
                    supply_value_ratio: Some(bank.asset_share_value.try_into()?),
                    borrow_value_ratio: Some(bank.liability_share_value.try_into()?),
                    pool_pubkey: market,
                });

                tx.send(global_lending_state)?;
            } else if disc_slice == user_acct_disc {
                // MarginfiAccount acct.
                let Ok(user_acct) = MarginfiAccount::deserialize(&mut &data[8..]) else {
                    log::warn!("Failed to deserialize MarginfiAccount {key}");
                    continue;
                };

                for balance in user_acct.lending_account.balances.iter() {
                    let bank_pk = balance.bank_pk;

                    let Some(bank) = banks.get(&bank_pk) else {
                        // No bank found in our map for this acct,
                        // We'll come back to these later
                        let entry = user_accts_missing_banks
                            .entry(key)
                            .or_insert((user_acct, vec![]));
                        entry.1.push(*balance);
                        continue;
                    };

                    let market = bank.group.to_string();

                    let (user_deposits, user_borrows) =
                        Self::get_user_balance_token_amounts(balance, bank);

                    let deposit_shares = balance.asset_shares.try_into().ok();
                    let borrow_shares = balance.liability_shares.try_into().ok();

                    let user_lending_state = Dooot::LendingUser(LendingUserDooot {
                        borrows: user_borrows,
                        deposits: user_deposits,
                        time: blocktime,
                        platform_program_pubkey: MARGINFI_PROGRAM.to_string(),
                        mint: bank.mint.to_string(),
                        user_pubkey: user_acct.authority.to_string(),
                        pool_pubkey: market,
                        borrow_shares,
                        deposit_shares,
                        lending_address_pubkey: None,
                    });

                    tx.send(user_lending_state)?;
                }
            }
        }

        // Circle back and try to pair every user balance with a Bank
        for (user_acct_pk, (acct, balances)) in user_accts_missing_banks {
            for balance in balances {
                let bank_pk = balance.bank_pk;

                let Some(bank) = banks.get(&bank_pk) else {
                    log::error!("Bank {bank_pk} still not found on second run of Marginfi accounts for user acct {user_acct_pk}! Snapshot may be incomplete!");
                    continue;
                };

                let market = bank.group.to_string();

                let (user_deposits, user_borrows) =
                    Self::get_user_balance_token_amounts(&balance, bank);

                let deposit_shares = balance.asset_shares.try_into().ok();
                let borrow_shares = balance.liability_shares.try_into().ok();

                let user_lending_state = Dooot::LendingUser(LendingUserDooot {
                    borrows: user_borrows,
                    deposits: user_deposits,
                    time: blocktime,
                    platform_program_pubkey: MARGINFI_PROGRAM.to_string(),
                    mint: bank.mint.to_string(),
                    user_pubkey: acct.authority.to_string(),
                    pool_pubkey: market,
                    borrow_shares,
                    deposit_shares,
                    lending_address_pubkey: None,
                });

                tx.send(user_lending_state)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use step_ingestooor_sdk::{
        decoded_transaction::DecodedTransaction, utils::test_utils::init_test_extras_bare,
    };

    #[tokio::test]
    async fn test_deserialization() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_f = fs::read_to_string("tests/marginfi/deser.json").unwrap();
        let decoded = serde_json::from_str(&txn_f).unwrap();
        let decoded = DecodedTransaction::from_rabbit_message_json(decoded).unwrap();

        let mut dooot = vec![];
        let parser = Marginfi;

        parser.invoke(&decoded, &mut dooot, &utils).await.unwrap();
    }

    #[tokio::test]
    async fn test_deserialization_2() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_f = fs::read_to_string("tests/marginfi/deser_2.json").unwrap();
        let decoded = serde_json::from_str(&txn_f).unwrap();
        let decoded = DecodedTransaction::from_rabbit_message_json(decoded).unwrap();

        let mut dooot = vec![];
        let parser = Marginfi;

        parser.invoke(&decoded, &mut dooot, &utils).await.unwrap();
    }

    #[tokio::test]
    async fn test_deserialization_3() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_f = fs::read_to_string("tests/marginfi/deser_3.json").unwrap();
        let decoded = serde_json::from_str(&txn_f).unwrap();
        let decoded = DecodedTransaction::from_rabbit_message_json(decoded).unwrap();

        let mut dooot = vec![];
        let parser = Marginfi;

        parser.invoke(&decoded, &mut dooot, &utils).await.unwrap();
    }

    #[tokio::test]
    async fn test_deserialization_4() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_f = fs::read_to_string("tests/marginfi/deser_4.json").unwrap();
        let decoded = serde_json::from_str(&txn_f).unwrap();
        let decoded = DecodedTransaction::from_rabbit_message_json(decoded).unwrap();

        let mut dooot = vec![];
        let parser = Marginfi;

        parser.invoke(&decoded, &mut dooot, &utils).await.unwrap();
    }

    #[tokio::test]
    async fn test_new_kamino_bank() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_f = fs::read_to_string("tests/marginfi/new_kamino_bank.json").unwrap();
        let decoded = serde_json::from_str(&txn_f).unwrap();
        let decoded = DecodedTransaction::from_rabbit_message_json(decoded).unwrap();

        let mut dooot = vec![];
        let parser = Marginfi;

        parser.invoke(&decoded, &mut dooot, &utils).await.unwrap();
        assert_eq!(dooot.len(), 1);
    }
}
