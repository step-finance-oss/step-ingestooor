use anyhow::{anyhow, Result};
use bytemuck::checked::try_from_bytes;
use fixed::types::U68F60;
use rust_decimal::Decimal;
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};

use std::{collections::HashMap, str::FromStr};
use step_ingestooor_idl_gens::kamino_lend_gen::kamino_lend::accounts::{Obligation, Reserve};

use crate::{state_source::StateSource, TransactionParserFilter, TransactionParserTrait};
use async_trait::async_trait;
use step_ingestooor_sdk::{
    dooot::{
        lending::{LendingGlobalDooot, LendingUserDooot},
        Dooot,
    },
    trait_defs::{
        discriminatable::Discriminatable,
        transaction::{AccountDatumIterItem, TransactionLike},
    },
    utils::{step_utils::StepUtils, util_fns::convert_blocktime_to_micros},
};

pub struct KaminoLend;

pub const KAMINO_LEND_PROGRAM: &str = "KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD";

#[async_trait]
impl TransactionParserTrait for KaminoLend {
    fn get_name(&self) -> &'static str {
        "KaminoLend"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            KAMINO_LEND_PROGRAM.into(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let accounts = get_accounts_data(txn)?;

        let mut reserve_accts: HashMap<Pubkey, &Reserve> = HashMap::new();
        let mut changed_reserves: Vec<Pubkey> = vec![];
        let mut changed_user_accts = vec![];

        let mut post_borrows = HashMap::new();
        let mut pre_borrows = HashMap::new();
        let mut all_borrows = HashMap::new();

        let mut post_deposits = HashMap::new();
        let mut pre_deposits = HashMap::new();
        let mut all_deposits = HashMap::new();

        let mut closed_obligations = vec![];

        // First iteration over accounts to collect reserves and changed user accounts
        for (acc, data_pre, data_post) in accounts.iter() {
            let key = acc.pubkey;

            let Some(disc) = get_discriminator_from_pre_or_post(data_pre, data_post)? else {
                continue;
            };

            match (data_pre, data_post) {
                // we have both pre and post
                (data_pre, Some(data_post)) => {
                    if disc == Reserve::discriminator().1 {
                        // Deser all Reserves since we may need them
                        let res: &Reserve = try_from_bytes(&data_post[8..])
                            .map_err(|e| anyhow!("Error deserializing KL Reserve account: {e}"))?;

                        let pk = Pubkey::from_str(key)?;
                        reserve_accts.insert(pk, res);

                        if acc.datum_changed {
                            changed_reserves.push(pk);
                        }
                    } else if disc == Obligation::discriminator().1 && acc.datum_changed {
                        // Only deser user accounts that changed.
                        let ob_post: &Obligation =
                            try_from_bytes(&data_post[8..]).map_err(|e| {
                                anyhow!("Error deserializing post-bytes KL Obligation account (still exists): {e}")
                            })?;

                        if let Some(data_pre) = data_pre {
                            let ob_pre: &Obligation = try_from_bytes(&data_pre[8..]).map_err(|e| {
                                anyhow!("Error deserializing pre-bytes KL Obligation account (still exists): {e}")
                            })?;

                            ob_pre.deposits.iter().for_each(|d| {
                                pre_deposits.insert(d.deposit_reserve, (d, ob_pre));
                            });

                            ob_pre.borrows.iter().for_each(|b| {
                                pre_borrows.insert(b.borrow_reserve, (b, ob_pre));
                            });
                        }

                        changed_user_accts.push(ob_post);

                        ob_post.deposits.iter().for_each(|d| {
                            post_deposits.insert(d.deposit_reserve, (d, ob_post));
                        });

                        ob_post.borrows.iter().for_each(|b| {
                            post_borrows.insert(b.borrow_reserve, (b, ob_post));
                        });
                    }
                }
                (Some(data_pre), None) => {
                    // account closed, meaning the entire position was withdrawn (last deposit in obligation)
                    let Ok(obligation) = try_from_bytes::<Obligation>(&data_pre[8..]) else {
                        // Probably not an account owned by this program, since we don't have pre owners as of writing.
                        continue;
                    };

                    closed_obligations.push(obligation);
                }
                _ => {
                    continue;
                }
            }
        }

        pre_borrows.iter().for_each(|(k, v)| {
            all_borrows.insert(*k, v);
        });

        post_borrows.iter().for_each(|(k, v)| {
            all_borrows.insert(*k, v);
        });

        pre_deposits.iter().for_each(|(k, v)| {
            all_deposits.insert(*k, v);
        });

        post_deposits.iter().for_each(|(k, v)| {
            all_deposits.insert(*k, v);
        });

        // Go through user accounts and create dooots
        for user_acc in changed_user_accts {
            let user_pubkey = user_acc.owner.to_string();
            let market = user_acc.lending_market.to_string();
            // User deposits
            for deposit in user_acc.deposits {
                let Some(reserve) = reserve_accts.get(&deposit.deposit_reserve) else {
                    // Not in `accounts` for this txn
                    continue;
                };
                let amt = U68F60::from(deposit.deposited_amount);
                let mint = reserve.liquidity.mint_pubkey.to_string();
                let token_amount = reserve.collateral_to_liquidity(amt);

                let deposit_dooot = Dooot::LendingUser(LendingUserDooot {
                    borrows: Decimal::ZERO,
                    deposits: token_amount,
                    mint,
                    platform_program_pubkey: KAMINO_LEND_PROGRAM.into(),
                    user_pubkey: user_pubkey.clone(),
                    time: txn.get_block_time(),
                    pool_pubkey: market.clone(),
                    borrow_shares: None,
                    deposit_shares: None,
                    lending_address_pubkey: None,
                });

                dooots.push(deposit_dooot);
            }

            // User borrows
            for borrow in user_acc.borrows {
                let Some(reserve) = reserve_accts.get(&borrow.borrow_reserve) else {
                    // Not in `accounts` for this txn
                    continue;
                };
                let amt = Self::sf_to_fixed(borrow.borrowed_amount_sf);
                let mint = reserve.liquidity.mint_pubkey.to_string();
                let token_amount = amt.to_num::<u64>().into();

                let borrow_dooot = Dooot::LendingUser(LendingUserDooot {
                    borrows: token_amount,
                    deposits: Decimal::ZERO,
                    mint,
                    platform_program_pubkey: KAMINO_LEND_PROGRAM.into(),
                    user_pubkey: user_pubkey.clone(),
                    time: txn.get_block_time(),
                    pool_pubkey: market.clone(),
                    borrow_shares: None,
                    deposit_shares: None,
                    lending_address_pubkey: None,
                });

                dooots.push(borrow_dooot);
            }
        }

        // loop through all borrows that don't exist in post borrows anymore (meaning they were closed by 100% withdraw of that token)
        for id in all_borrows.keys() {
            if let (Some(pre), None) = (pre_borrows.get(id), post_borrows.get(id)) {
                let borrow = pre.0;
                let owner = pre.1.owner.to_string();
                let Some(reserve) = reserve_accts.get(&borrow.borrow_reserve) else {
                    // Not in `accounts` for this txn
                    continue;
                };
                let mint = reserve.liquidity.mint_pubkey.to_string();

                let borrow_dooot = Dooot::LendingUser(LendingUserDooot {
                    borrows: Decimal::ZERO,
                    deposits: Decimal::ZERO,
                    mint,
                    platform_program_pubkey: KAMINO_LEND_PROGRAM.into(),
                    user_pubkey: owner,
                    time: txn.get_block_time(),
                    pool_pubkey: reserve.lending_market.to_string(),
                    borrow_shares: None,
                    deposit_shares: None,
                    lending_address_pubkey: None,
                });

                dooots.push(borrow_dooot);
            }
        }

        // loop through all deposits that doesn't exist in post deposits (meaning they were closed)
        for id in all_deposits.keys() {
            if let (Some(pre), None) = (pre_deposits.get(id), post_deposits.get(id)) {
                let deposit = pre.0;
                let owner = pre.1.owner.to_string();
                let Some(reserve) = reserve_accts.get(&deposit.deposit_reserve) else {
                    // Not in `accounts` for this txn
                    continue;
                };
                let mint = reserve.liquidity.mint_pubkey.to_string();

                let deposit_dooot = Dooot::LendingUser(LendingUserDooot {
                    borrows: Decimal::ZERO,
                    deposits: Decimal::ZERO,
                    mint,
                    platform_program_pubkey: KAMINO_LEND_PROGRAM.into(),
                    user_pubkey: owner,
                    time: txn.get_block_time(),
                    pool_pubkey: reserve.lending_market.to_string(),
                    borrow_shares: None,
                    deposit_shares: None,
                    lending_address_pubkey: None,
                });

                dooots.push(deposit_dooot);
            }
        }

        // go through closed_obligations and create zero dooots
        closed_obligations.iter().for_each(|obligation| {
            let Some(reserve) = reserve_accts.get(&obligation.deposits[0].deposit_reserve) else {
                return;
            };

            let mint = reserve.liquidity.mint_pubkey.to_string();

            let deposit_dooot = Dooot::LendingUser(LendingUserDooot {
                borrows: Decimal::ZERO,
                deposits: Decimal::ZERO,
                mint,
                platform_program_pubkey: KAMINO_LEND_PROGRAM.into(),
                user_pubkey: obligation.owner.to_string(),
                time: txn.get_block_time(),
                pool_pubkey: reserve.lending_market.to_string(),
                borrow_shares: None,
                deposit_shares: None,
                lending_address_pubkey: None,
            });

            dooots.push(deposit_dooot);
        });

        // Make global states from changed Reserves
        for res_key in changed_reserves {
            let reserve = reserve_accts
                .get(&res_key)
                .ok_or_else(|| anyhow!("Reserve not found in txn {}", txn.get_signature()))?;

            let mint = reserve.liquidity.mint_pubkey.to_string();
            let market = reserve.lending_market.to_string();

            let (supply_apr, borrow_apr) = reserve.get_rates();
            let total_deposits =
                Decimal::from(reserve.liquidity.total_supply().floor().to_num::<u64>());
            let total_borrows =
                Decimal::from(reserve.liquidity.total_borrows().floor().to_num::<u64>());

            let g_dooot = Dooot::LendingGlobal(LendingGlobalDooot {
                borrow_apr,
                supply_apr,
                mint,
                platform_program_pubkey: KAMINO_LEND_PROGRAM.into(),
                total_deposits,
                total_borrows,
                time: txn.get_block_time(),
                pool_pubkey: market,
                borrow_value_ratio: None,
                supply_value_ratio: None,
            });

            dooots.push(g_dooot);
        }
        Ok(())
    }
}

// function that gets accounts
#[allow(clippy::type_complexity)]
fn get_accounts_data(
    txn: &impl TransactionLike,
) -> Result<Vec<(AccountDatumIterItem, Option<Vec<u8>>, Option<Vec<u8>>)>> {
    let accounts = txn
        .iter_accounts()
        .map(|a| {
            let unpacked_pre_data = a.unpack_pre_datum().ok().flatten();
            let unpacked_post_data = a.unpack_post_datum().ok().flatten();
            Ok((a, unpacked_pre_data, unpacked_post_data))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(accounts)
}

fn get_discriminator_from_pre_or_post<'a>(
    data_pre: &'a Option<Vec<u8>>,
    data_post: &'a Option<Vec<u8>>,
) -> Result<Option<&'a [u8]>> {
    let disc = data_pre
        .as_ref()
        .and_then(|d| d.get(0..8))
        .or(data_post.as_ref().and_then(|d| d.get(0..8)));

    Ok(disc)
}

impl KaminoLend {
    pub fn sf_to_fixed(num: u128) -> U68F60 {
        U68F60::from_bits(num)
    }
}

impl StateSource for KaminoLend {
    fn get_name(&self) -> &'static str {
        "KaminoLend"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let blocktime = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;

        // Hold on to datums for bytemuck refs
        let mut reserve_datums = HashMap::new();
        let mut obligation_datums = HashMap::new();

        let mut reserves_de = HashMap::new();
        // let mut obligations_de = HashMap::new();

        let kamino_pubkey = Pubkey::from_str(KAMINO_LEND_PROGRAM)?;
        let acct_iter = acct_provider.program_accounts(kamino_pubkey, None);

        for acct in acct_iter {
            let data = acct.account.data();
            let Some(discrim) = data.get(..8) else {
                continue;
            };

            if discrim == Reserve::discriminator().1 {
                let data_copy = data[8..].to_vec();
                reserve_datums.insert(acct.pubkey, data_copy);
                log::debug!("Found reserve acct");
            } else if discrim == Obligation::discriminator().1 {
                let data_copy = data[8..].to_vec();
                obligation_datums.insert(acct.pubkey, data_copy);
                log::debug!("Found obligation acct");
            }
        }

        // Deser all reserves, make global dooots in the meantime
        for (pubkey, datum) in reserve_datums.iter() {
            let Ok(reserve) = try_from_bytes::<Reserve>(datum) else {
                log::warn!("Failed to deserialize reserve: {pubkey}");
                continue;
            };
            reserves_de.insert(*pubkey, reserve);

            let mint = reserve.liquidity.mint_pubkey.to_string();
            let market = reserve.lending_market.to_string();
            let (supply_apr, borrow_apr) = reserve.get_rates();
            let total_deposits =
                Decimal::from(reserve.liquidity.total_supply().floor().to_num::<u64>());
            let total_borrows =
                Decimal::from(reserve.liquidity.total_borrows().floor().to_num::<u64>());

            let g_dooot = Dooot::LendingGlobal(LendingGlobalDooot {
                borrow_apr,
                supply_apr,
                mint,
                platform_program_pubkey: KAMINO_LEND_PROGRAM.into(),
                total_deposits,
                total_borrows,
                time: blocktime,
                pool_pubkey: market,
                borrow_value_ratio: None,
                supply_value_ratio: None,
            });

            tx.send(g_dooot)?;
        }

        for (pubkey, datum) in obligation_datums.iter() {
            let Ok(obligation) = try_from_bytes::<Obligation>(datum) else {
                log::warn!("Failed to deserialize Obligation: {pubkey}");
                continue;
            };
            let user_pubkey = obligation.owner.to_string();
            let market = obligation.lending_market.to_string();
            // User deposits first
            for deposit in obligation.deposits.iter() {
                let amt = U68F60::from(deposit.deposited_amount);
                let Some(reserve) = reserves_de.get(&deposit.deposit_reserve) else {
                    log::warn!("Reserve not found for obligation {pubkey}");
                    continue;
                };

                let mint = reserve.liquidity.mint_pubkey.to_string();
                let token_amount = reserve.collateral_to_liquidity(amt);

                let deposit_dooot = Dooot::LendingUser(LendingUserDooot {
                    borrows: Decimal::ZERO,
                    deposits: token_amount,
                    mint,
                    platform_program_pubkey: KAMINO_LEND_PROGRAM.into(),
                    user_pubkey: user_pubkey.clone(),
                    time: blocktime,
                    pool_pubkey: market.clone(),
                    borrow_shares: None,
                    deposit_shares: None,
                    lending_address_pubkey: None,
                });

                tx.send(deposit_dooot)?;
            }

            // User borrows
            for borrow in obligation.borrows.iter() {
                let amt = Self::sf_to_fixed(borrow.borrowed_amount_sf);
                let Some(reserve) = reserves_de.get(&borrow.borrow_reserve) else {
                    log::warn!("Reserve not found for obligation {pubkey}");
                    continue;
                };

                let mint = reserve.liquidity.mint_pubkey.to_string();
                let token_amount = amt.to_num::<u64>().into();

                let borrow_dooot = Dooot::LendingUser(LendingUserDooot {
                    borrows: token_amount,
                    deposits: Decimal::ZERO,
                    mint,
                    platform_program_pubkey: KAMINO_LEND_PROGRAM.into(),
                    user_pubkey: user_pubkey.clone(),
                    time: blocktime,
                    pool_pubkey: market.clone(),
                    borrow_shares: None,
                    deposit_shares: None,
                    lending_address_pubkey: None,
                });

                tx.send(borrow_dooot)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{KaminoLend, TransactionParserTrait};
    use rust_decimal::Decimal;
    use serde_json::Value;
    use std::fs;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    use step_ingestooor_sdk::dooot::Dooot;
    use step_ingestooor_sdk::utils::test_utils::init_test_extras_bare;

    #[tokio::test]
    async fn test_kamino_v2_deser_err() {
        let file = fs::read_to_string("./tests/kamino/v2_deser_err.json").unwrap();
        let json: Value = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let kamino = KaminoLend;
        let mut dooots = vec![];
        let (step_utils, _) = init_test_extras_bare().await;

        kamino.invoke(&txn, &mut dooots, &step_utils).await.unwrap();
    }

    /// Helper function to set up and run a Kamino test with a given JSON file
    /// Returns the dooots generated by the test
    async fn setup_kamino_test(json_file_name: &str) -> Vec<Dooot> {
        let file_path = format!("./tests/kamino/{}", json_file_name);
        let file = fs::read_to_string(file_path).expect("Unable to read file");
        let txn: DecodedTransaction = serde_json::from_str(&file).expect("Unable to parse json");
        let kamino = KaminoLend;
        let mut dooots = vec![];
        let (step_utils, _) = init_test_extras_bare().await;

        kamino.invoke(&txn, &mut dooots, &step_utils).await.unwrap();

        dooots
    }

    /// Helper function to set up and run a Kamino test with a given JSON file
    /// Returns the dooots generated by the test
    async fn setup_kamino_test_rabbit_txn(json_file_name: &str) -> Vec<Dooot> {
        let file_path = format!("./tests/kamino/{}", json_file_name);
        let file = fs::read_to_string(file_path).expect("Unable to read file");
        let txn = serde_json::from_str(&file)
            .ok()
            .and_then(|txn| DecodedTransaction::from_rabbit_message_json(txn).ok())
            .expect("Unable to parse json");
        let kamino = KaminoLend;
        let mut dooots = vec![];
        let (step_utils, _) = init_test_extras_bare().await;

        kamino.invoke(&txn, &mut dooots, &step_utils).await.unwrap();

        dooots
    }

    // Repaying part of EURC (HzwqbKZw8HxMN6bF2yFZNrht3c2iXXzpKcFu7uBEDKtr) borrow position,
    // should result in having LendingUserDooot with that mint and borrows 500000
    #[tokio::test]
    async fn test_kamino_repay_part_of_borrow() {
        let dooots = setup_kamino_test("borrow_repay_part.json").await;

        // find dooot with EURC mint
        let dooot = dooots.iter().find(|d| match d {
            Dooot::LendingUser(du) => du.mint == "HzwqbKZw8HxMN6bF2yFZNrht3c2iXXzpKcFu7uBEDKtr",
            _ => false,
        });

        // Check if new balance is 500000
        if let Some(Dooot::LendingUser(du)) = dooot {
            assert_eq!(du.borrows, Decimal::from(500000));
        }
    }

    // Repaying entire EURC (HzwqbKZw8HxMN6bF2yFZNrht3c2iXXzpKcFu7uBEDKtr) borrow position,
    // should result in having LendingUserDooot with that mint and borrows and deposits of 0
    #[tokio::test]
    async fn test_kamino_repay_single_borrow_fully() {
        let dooots = setup_kamino_test("borrow_repay_single_fully.json").await;

        // find dooot with EURC mint
        let dooot = dooots.iter().find(|d| match d {
            Dooot::LendingUser(du) => du.mint == "HzwqbKZw8HxMN6bF2yFZNrht3c2iXXzpKcFu7uBEDKtr",
            _ => false,
        });

        // Check if new balance is 0
        if let Some(Dooot::LendingUser(du)) = dooot {
            assert_eq!(du.borrows, Decimal::ZERO);
            assert_eq!(du.deposits, Decimal::ZERO);
        }
    }

    // Repaying entire PYUSD (2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo) borrow position,
    // should result in having LendingUserDooot with that mint and borrows and deposits of 0
    // this is also the last borrow position after which the user is fully repaid and has no borrows
    #[tokio::test]
    async fn test_kamino_repay_last_fully() {
        let dooots = setup_kamino_test("borrow_repay_last_fully.json").await;

        // find dooot with EURC mint
        let dooot = dooots.iter().find(|d| match d {
            Dooot::LendingUser(du) => du.mint == "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo",
            _ => false,
        });

        // Check if new balance is 0
        if let Some(Dooot::LendingUser(du)) = dooot {
            assert_eq!(du.borrows, Decimal::ZERO);
            assert_eq!(du.deposits, Decimal::ZERO);
        }
    }

    // Withdraw part of USDC (EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v) deposit position,
    // should result in having LendingUserDooot with that mint and deposits 500850
    #[tokio::test]
    async fn test_kamino_withdraw_part_of_deposit() {
        let dooots = setup_kamino_test("deposit_withdraw_part.json").await;

        // find dooot with USDC mint
        let dooot = dooots.iter().find(|d| match d {
            Dooot::LendingUser(du) => du.mint == "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            _ => panic!("No LendingUserDooot with USDC mint"),
        });

        // Check if new balance is 500000
        if let Some(Dooot::LendingUser(du)) = dooot {
            assert_eq!(du.deposits, Decimal::from(500850));
        }
    }

    // Withdraw all USDC (EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v) deposit position,
    // should result in having LendingUserDooot with that mint and deposits of 0
    #[tokio::test]
    async fn test_kamino_withdraw_single_deposit_fully() {
        let dooots = setup_kamino_test("deposit_withdraw_single_fully.json").await;

        // find dooot with USDC mint or fail test
        let dooot = dooots.iter().find(|d| match d {
            Dooot::LendingUser(du) => du.mint == "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            _ => panic!("USDC lending user dooot not found"),
        });

        // Check if new balance is 0
        if let Some(Dooot::LendingUser(du)) = dooot {
            assert_eq!(du.deposits, Decimal::ZERO);
        }
    }

    // Withdraw entire SOL (So11111111111111111111111111111111111111112) deposit position,
    // should result in having LendingUserDooot with that mint and deposits of 0
    // this is also the last deposit position after which the user is fully repaid and has no deposits
    #[tokio::test]
    async fn test_kamino_withdraw_last_fully() {
        let dooots = setup_kamino_test("deposit_withdraw_last_fully.json").await;

        // find dooot with SOL mint or fail test
        let dooot = dooots.iter().find(|d| match d {
            Dooot::LendingUser(du) => du.mint == "So11111111111111111111111111111111111111112",
            _ => panic!("SOL lending user dooot not found"),
        });

        // Check if new balance is 0
        if let Some(Dooot::LendingUser(du)) = dooot {
            assert_eq!(du.deposits, Decimal::ZERO);
        }
    }

    #[tokio::test]
    async fn creation_missing() {
        let dooots = setup_kamino_test_rabbit_txn("account_creation.json").await;

        assert_eq!(dooots.len(), 2);
    }
}
