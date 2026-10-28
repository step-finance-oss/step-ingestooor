use crate::lending::hubble::utils::{
    get_accounts_data, get_discriminator_from_pre_or_post, market_collateral_to_amount_map,
    user_collateral_to_amount_map,
};
use crate::state_source::StateSource;
use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::{Context, Result};
use async_trait::async_trait;
use borsh::BorshDeserialize;
use rust_decimal::Decimal;
use solana_sdk::account::ReadableAccount;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;
use step_ingestooor_idl_gens::hubble_lending_gen::hubble_lending::accounts::{
    BorrowingMarketState, BorrowingVaultsNew, UserMetadata,
};
use step_ingestooor_sdk::{
    dooot::{
        lending::{LendingGlobalDooot, LendingUserDooot},
        Dooot,
    },
    trait_defs::{discriminatable::Discriminatable, transaction::TransactionLike},
    utils::{step_utils::StepUtils, util_fns::convert_blocktime_to_micros},
};

pub struct HubbleLend;

pub const HUBBLE_LEND_PROGRAM: &str = "HubbLeXBb7qyLHt3x7gvYaRrxQmmgExb7fCJgDqFuB6T";

#[async_trait]
impl TransactionParserTrait for HubbleLend {
    fn get_name(&self) -> &'static str {
        "HubbleLend"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            HUBBLE_LEND_PROGRAM.into(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let accounts = get_accounts_data(txn)?;

        let mut user_metadata: HashMap<&str, UserMetadata> = HashMap::new();
        let mut deleted_user_metadata: HashMap<&str, UserMetadata> = HashMap::new();
        let mut borrowing_vaults: HashMap<&str, BorrowingVaultsNew> = HashMap::new();
        let mut borrowing_market: HashMap<&str, BorrowingMarketState> = HashMap::new();

        // Loop through accounts and save deserialized data
        for (acc, data_pre, data_post) in accounts.iter() {
            let key = acc.pubkey;
            let Some(disc) = get_discriminator_from_pre_or_post(data_pre, data_post)? else {
                continue;
            };

            let Some(data_post) = data_post else {
                // This happens when an account is deleted, meaning a user has withdrawn entire collateral
                let Some(data_pre) = data_pre else {
                    continue;
                };

                // save deleted user metadata, so we can emit zero balance dooot for collateral
                if disc == UserMetadata::discriminator().1 {
                    let data_copy = data_pre[8..].to_vec();
                    let user_meta = UserMetadata::deserialize(&mut &data_copy[..])
                        .context("Failed to deserialize market")?;

                    deleted_user_metadata.insert(key, user_meta);
                }

                continue;
            };

            if disc == BorrowingVaultsNew::discriminator().1 {
                let data_copy = data_post[8..].to_vec();
                let borrowing_vaults_data = BorrowingVaultsNew::deserialize(&mut &data_copy[..])
                    .context("Failed to deserialize borrowing vaults")?;

                borrowing_vaults.insert(key, borrowing_vaults_data);
            } else if disc == UserMetadata::discriminator().1 {
                let data_copy = data_post[8..].to_vec();
                let user_meta = UserMetadata::deserialize(&mut &data_copy[..])
                    .context("Failed to deserialize market")?;

                user_metadata.insert(key, user_meta);
            } else if disc == BorrowingMarketState::discriminator().1 {
                let data_copy = data_post[8..].to_vec();
                let market_data = BorrowingMarketState::deserialize(&mut &data_copy[..])
                    .context("Failed to deserialize market")?;

                borrowing_market.insert(key, market_data);
            }
        }

        // loop borrowing market state and create global dooots
        for (borrowing_market_key, borrowing_market_data) in borrowing_market.iter() {
            let borrow_value_ratio = Decimal::from(borrowing_market_data.stablecoin_borrowed)
                / Decimal::from(borrowing_market_data.total_stake);

            let borrowing_vault = match borrowing_vaults
                .get(borrowing_market_data.borrowing_vaults.to_string().as_str())
            {
                Some(borrowing_vaults) => borrowing_vaults,
                None => continue,
            };

            let amount_map = market_collateral_to_amount_map(
                borrowing_market_data.inactive_collateral,
                borrowing_market_data.deposited_collateral,
                borrowing_vault.collateral_mints,
            )?;

            let borrow_apr = borrowing_market_data.stability_fee_bps as f64 / 10_000.0;

            // global dooot for stablecoin borrows
            let global_lending_state = Dooot::LendingGlobal(LendingGlobalDooot {
                mint: borrowing_market_data.stablecoin_mint.to_string(),
                platform_program_pubkey: HUBBLE_LEND_PROGRAM.into(),
                time,
                total_borrows: Decimal::from(borrowing_market_data.stablecoin_borrowed),
                total_deposits: Decimal::ZERO,
                borrow_apr,
                supply_apr: 0.0,
                supply_value_ratio: None,
                borrow_value_ratio: Some(borrow_value_ratio),
                pool_pubkey: borrowing_market_key.to_string(),
            });
            dooots.push(global_lending_state);

            // global dooots for collateral deposits
            for (mint, amount) in amount_map.iter() {
                if amount > &0 {
                    let g_dooot = Dooot::LendingGlobal(LendingGlobalDooot {
                        time,
                        mint: mint.to_string(),
                        platform_program_pubkey: HUBBLE_LEND_PROGRAM.into(),
                        total_deposits: Decimal::from(*amount),
                        total_borrows: Decimal::ZERO,
                        supply_apr: 0.0,
                        borrow_apr: 0.0,
                        supply_value_ratio: None,
                        borrow_value_ratio: None,
                        pool_pubkey: borrowing_market_key.to_string(),
                    });

                    dooots.push(g_dooot);
                }
            }
        }

        for (_user_metadata_key, user_metadata_data) in user_metadata.iter() {
            let borrowing_market = match borrowing_market.get(
                user_metadata_data
                    .borrowing_market_state
                    .to_string()
                    .as_str(),
            ) {
                Some(borrowing_market) => borrowing_market,
                None => continue,
            };

            let borrowing_vault = match borrowing_vaults
                .get(borrowing_market.borrowing_vaults.to_string().as_str())
            {
                Some(borrowing_vault) => borrowing_vault,
                None => continue,
            };

            let user_share = Decimal::from(user_metadata_data.user_stake)
                / Decimal::from(borrowing_market.total_stake);

            let borrowed_stablecoin = user_metadata_data.borrowed_stablecoin as f64;

            // borrow dooot
            let user_lending_state = Dooot::LendingUser(LendingUserDooot {
                time,
                platform_program_pubkey: HUBBLE_LEND_PROGRAM.into(),
                mint: borrowing_market.stablecoin_mint.to_string(),
                user_pubkey: user_metadata_data.owner.to_string(),
                borrows: borrowed_stablecoin.try_into()?,
                deposits: Decimal::ZERO,
                borrow_shares: Some(user_share),
                deposit_shares: Some(Decimal::ZERO),
                pool_pubkey: user_metadata_data.borrowing_market_state.to_string(),
                lending_address_pubkey: Some(user_metadata_data.metadata_pk.to_string()),
            });

            dooots.push(user_lending_state);

            let amount_map = user_collateral_to_amount_map(
                user_metadata_data.inactive_collateral,
                user_metadata_data.deposited_collateral,
                borrowing_vault.collateral_mints,
            )?;

            for (mint, amount) in amount_map.iter() {
                let user_lending_state = Dooot::LendingUser(LendingUserDooot {
                    time,
                    platform_program_pubkey: HUBBLE_LEND_PROGRAM.into(),
                    mint: mint.to_string(),
                    user_pubkey: user_metadata_data.owner.to_string(),
                    borrows: Decimal::ZERO,
                    deposits: Decimal::from(*amount),
                    borrow_shares: None,
                    deposit_shares: None,
                    pool_pubkey: user_metadata_data.borrowing_market_state.to_string(),
                    lending_address_pubkey: Some(user_metadata_data.metadata_pk.to_string()),
                });

                dooots.push(user_lending_state);
            }
        }

        for (_user_metadata_key, user_metadata_data) in deleted_user_metadata.iter() {
            let borrowing_market = match borrowing_market.get(
                user_metadata_data
                    .borrowing_market_state
                    .to_string()
                    .as_str(),
            ) {
                Some(borrowing_market) => borrowing_market,
                None => continue,
            };

            let borrowing_vault = match borrowing_vaults
                .get(borrowing_market.borrowing_vaults.to_string().as_str())
            {
                Some(borrowing_vault) => borrowing_vault,
                None => continue,
            };

            let user_lending_state = Dooot::LendingUser(LendingUserDooot {
                time,
                platform_program_pubkey: HUBBLE_LEND_PROGRAM.into(),
                mint: borrowing_market.stablecoin_mint.to_string(),
                user_pubkey: user_metadata_data.owner.to_string(),
                borrows: Decimal::ZERO,
                deposits: Decimal::ZERO,
                borrow_shares: Some(Decimal::ZERO),
                deposit_shares: Some(Decimal::ZERO),
                pool_pubkey: user_metadata_data.borrowing_market_state.to_string(),
                lending_address_pubkey: Some(user_metadata_data.metadata_pk.to_string()),
            });

            dooots.push(user_lending_state);

            let amount_map = user_collateral_to_amount_map(
                user_metadata_data.inactive_collateral,
                user_metadata_data.deposited_collateral,
                borrowing_vault.collateral_mints,
            )?;

            for (mint, amount) in amount_map.iter() {
                if amount > &0 {
                    let user_lending_state = Dooot::LendingUser(LendingUserDooot {
                        time,
                        platform_program_pubkey: HUBBLE_LEND_PROGRAM.into(),
                        mint: mint.to_string(),
                        user_pubkey: user_metadata_data.owner.to_string(),
                        borrows: Decimal::ZERO,
                        deposits: Decimal::ZERO,
                        borrow_shares: None,
                        deposit_shares: None,
                        pool_pubkey: user_metadata_data.borrowing_market_state.to_string(),
                        lending_address_pubkey: Some(user_metadata_data.metadata_pk.to_string()),
                    });

                    dooots.push(user_lending_state);
                }
            }
        }

        Ok(())
    }
}

impl StateSource for HubbleLend {
    fn get_name(&self) -> &'static str {
        "HubbleLend"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let blocktime = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let hubble_pubkey = Pubkey::from_str(HUBBLE_LEND_PROGRAM)?;
        let acct_iter = acct_provider.program_accounts(hubble_pubkey, None);

        let mut borrowing_vaults_datums = HashMap::new();
        let mut user_metadata_datums = HashMap::new();
        let mut borrowing_market_datums = HashMap::new();
        let mut collateral_mints: [Pubkey; 500] = [Pubkey::default(); 500];

        for acct in acct_iter {
            print!(".");
            let data = acct.account.data();
            let Some(discrim) = data.get(..8) else {
                continue;
            };

            if discrim == UserMetadata::discriminator().1 {
                let data_copy = data[8..].to_vec();
                user_metadata_datums.insert(acct.pubkey, data_copy);
            } else if discrim == BorrowingVaultsNew::discriminator().1 {
                let data_copy = data[8..].to_vec();
                borrowing_vaults_datums.insert(acct.pubkey, data_copy);
            } else if discrim == BorrowingMarketState::discriminator().1 {
                let data_copy = data[8..].to_vec();
                borrowing_market_datums.insert(acct.pubkey, data_copy);
            }
        }

        for (borrowing_market_pubkey, datum) in borrowing_market_datums.iter() {
            let market_data = BorrowingMarketState::deserialize(&mut &datum[..])
                .context("Failed to deserialize market")?;

            let borrowing_vault = match borrowing_vaults_datums.get(&market_data.borrowing_vaults) {
                Some(vault) => vault,
                None => continue,
            };

            let borrowing_vault_data = BorrowingVaultsNew::deserialize(&mut &borrowing_vault[..])
                .context("Failed to deserialize vault")?;

            collateral_mints = borrowing_vault_data.collateral_mints;
            let mut amount_map = HashMap::new();

            // loop through deposit_amounts
            for (i, amount) in market_data.deposited_collateral.amounts.iter().enumerate() {
                let mint = borrowing_vault_data.collateral_mints[i];

                amount_map.insert(mint, *amount);
            }

            // loop through inactive_collateral and add to amount_map
            for (i, amount) in market_data.inactive_collateral.amounts.iter().enumerate() {
                let mint = borrowing_vault_data.collateral_mints[i];
                let old_amount = amount_map.get(&mint).unwrap_or(&0);
                amount_map.insert(mint, old_amount + amount);
            }

            let borrow_value_ratio = Decimal::from(market_data.stablecoin_borrowed)
                / Decimal::from(market_data.total_stake);

            let borrow_apr = market_data.stability_fee_bps as f64 / 10_000.0;

            // Global dooot for stablecoin borrows
            let borrow_dooot = Dooot::LendingGlobal(LendingGlobalDooot {
                time: blocktime,
                mint: market_data.stablecoin_mint.to_string(),
                platform_program_pubkey: HUBBLE_LEND_PROGRAM.into(),
                total_deposits: Decimal::ZERO,
                total_borrows: Decimal::from(market_data.stablecoin_borrowed),
                supply_apr: 0.0,
                borrow_apr,
                supply_value_ratio: None,
                borrow_value_ratio: Some(borrow_value_ratio),
                pool_pubkey: borrowing_market_pubkey.to_string(),
            });
            tx.send(borrow_dooot)?;

            // loop through amount_map and create global dooots for deposits
            for (mint, amount) in amount_map.iter() {
                if amount > &0 {
                    // println!("mint: {:#?}, amount: {:#?}", mint, amount);

                    let g_dooot = Dooot::LendingGlobal(LendingGlobalDooot {
                        time: blocktime,
                        mint: mint.to_string(),
                        platform_program_pubkey: HUBBLE_LEND_PROGRAM.into(),
                        total_deposits: Decimal::from(*amount),
                        total_borrows: Decimal::ZERO,
                        supply_apr: 0.0,
                        borrow_apr: 0.0,
                        supply_value_ratio: None,
                        borrow_value_ratio: None,
                        pool_pubkey: borrowing_market_pubkey.to_string(),
                    });
                    tx.send(g_dooot)?;
                }
            }
        }

        // loop through user_metadata_datums and create dooots
        for (_user_metadata_pubkey, datum) in user_metadata_datums.iter() {
            let user_metadata = UserMetadata::deserialize(&mut &datum[..])
                .context("Failed to deserialize user_metadata")?;

            // Skip this iteration if the borrowing market state doesn't exist
            let borrowing_market =
                match borrowing_market_datums.get(&user_metadata.borrowing_market_state) {
                    Some(market) => market,
                    None => continue,
                };

            let borrowing_market_data =
                BorrowingMarketState::deserialize(&mut &borrowing_market[..])
                    .context("Failed to deserialize market")?;

            let inactive_collateral = user_metadata.inactive_collateral;
            let deposited_collateral = user_metadata.deposited_collateral;

            let amount_map = user_collateral_to_amount_map(
                inactive_collateral,
                deposited_collateral,
                collateral_mints,
            )?;

            if user_metadata.borrowed_stablecoin > 0 {
                let user_share = Decimal::from(user_metadata.borrowed_stablecoin)
                    / Decimal::from(borrowing_market_data.total_stake);

                let u_dooot = Dooot::LendingUser(LendingUserDooot {
                    time: blocktime,
                    mint: borrowing_market_data.stablecoin_mint.to_string(),
                    platform_program_pubkey: HUBBLE_LEND_PROGRAM.into(),
                    user_pubkey: user_metadata.owner.to_string(),
                    borrows: Decimal::from(user_metadata.borrowed_stablecoin),
                    deposits: Decimal::ZERO,
                    pool_pubkey: user_metadata.borrowing_market_state.to_string(),
                    borrow_shares: Some(user_share),
                    deposit_shares: None,
                    lending_address_pubkey: Some(user_metadata.metadata_pk.to_string()),
                });
                tx.send(u_dooot)?;
            }

            // loop through amount_map and create User dooots
            for (mint, amount) in amount_map.iter() {
                if amount > &0 {
                    let u_dooot = Dooot::LendingUser(LendingUserDooot {
                        time: blocktime,
                        mint: mint.to_string(),
                        platform_program_pubkey: HUBBLE_LEND_PROGRAM.into(),
                        user_pubkey: user_metadata.owner.to_string(),
                        borrows: Decimal::ZERO,
                        deposits: Decimal::from(*amount),
                        pool_pubkey: user_metadata.borrowing_market_state.to_string(),
                        borrow_shares: None,
                        deposit_shares: None,
                        lending_address_pubkey: Some(user_metadata.metadata_pk.to_string()),
                    });
                    tx.send(u_dooot)?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::lending::hubble::parser::HubbleLend;
    use crate::TransactionParserTrait;
    use rust_decimal::Decimal;
    use std::fs;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    use step_ingestooor_sdk::dooot::Dooot;
    use step_ingestooor_sdk::utils::test_utils::init_test_extras_bare;

    async fn setup_hubble_test(json_file_name: &str) -> Vec<Dooot> {
        let file_path = format!("./tests/hubble/{}", json_file_name);
        let file = fs::read_to_string(file_path).expect("Unable to read file");
        let txn: DecodedTransaction = serde_json::from_str(&file).expect("Unable to parse json");
        let hubble = HubbleLend;
        let mut dooots = vec![];
        let (step_utils, _) = init_test_extras_bare().await;

        hubble.invoke(&txn, &mut dooots, &step_utils).await.unwrap();

        dooots
    }

    // Initial deposit of 0.2 SOL (200,000,000 lamports)
    // should result in having LendingUserDooot with that mint and deposits of 200,000,000
    #[tokio::test]
    async fn test_hubble_deposit() {
        let dooots = setup_hubble_test("deposit.json").await;

        // find dooot with SOL mint
        let dooot = dooots
            .iter()
            .find(|d| match d {
                Dooot::LendingUser(du) => du.mint == "So11111111111111111111111111111111111111112",
                _ => false,
            })
            .expect("Unable to find dooot");

        // Check if the new balance is 200,000,000
        if let Dooot::LendingUser(du) = dooot {
            assert_eq!(du.deposits, Decimal::from(200000000));
        }
    }

    // Withdraw of 0.05 SOL against the existing 0.1 SOL position
    // should result in having LendingUserDooot with that mint and deposits of 50,000,000
    #[tokio::test]
    async fn test_hubble_deposit_withdraw_part() {
        let dooots = setup_hubble_test("deposit_withdraw_part.json").await;

        let dooot = dooots
            .iter()
            .find(|d| match d {
                Dooot::LendingUser(du) => du.mint == "So11111111111111111111111111111111111111112",
                _ => false,
            })
            .expect("Unable to find dooot");

        // Check if the new balance is 50,000,000
        if let Dooot::LendingUser(du) = dooot {
            assert_eq!(du.deposits, Decimal::from(50000000));
        }
    }

    // Withdraw last 0.05 SOL collateral, which closes an account and your position
    // should result in having LendingUserDooot with that mint and deposits of 0
    #[tokio::test]
    async fn test_hubble_deposit_withdraw_fully() {
        let dooots = setup_hubble_test("deposit_withdraw_fully.json").await;

        let dooot = dooots
            .iter()
            .find(|d| match d {
                Dooot::LendingUser(du) => du.mint == "So11111111111111111111111111111111111111112",
                _ => false,
            })
            .expect("Unable to find dooot");

        // Check if the new balance is 0
        if let Dooot::LendingUser(du) = dooot {
            assert_eq!(du.deposits, Decimal::ZERO);
        }
    }

    // Withdraw last 0.1 SOL collateral, while the position still has 0.01 daoSOL collateral remaining (GEJpt3Wjmr628FqXxTgxMce1pLntcPV4uFi8ksxMyPQh)
    // should result in having LendingUserDooot with SOL mint and deposits of 0
    #[tokio::test]
    async fn test_hubble_deposit_withdraw_single_fully() {
        let dooots = setup_hubble_test("deposit_withdraw_single_fully.json").await;

        let sol_dooot = dooots
            .iter()
            .find(|d| match d {
                Dooot::LendingUser(du) => du.mint == "So11111111111111111111111111111111111111112",
                _ => false,
            })
            .expect("Unable to find SOL dooot");

        let dao_dooot = dooots
            .iter()
            .find(|d| match d {
                Dooot::LendingUser(du) => du.mint == "GEJpt3Wjmr628FqXxTgxMce1pLntcPV4uFi8ksxMyPQh",
                _ => false,
            })
            .expect("Unable to find daoSOL dooot");

        // Check if the new SOL balance is 0
        if let Dooot::LendingUser(du) = sol_dooot {
            assert_eq!(du.deposits, Decimal::ZERO);
        }

        // Check if the new daoSOL balance is still 0.01 (10000000)
        if let Dooot::LendingUser(du) = dao_dooot {
            assert_eq!(du.deposits, Decimal::from(10000000));
        }
    }

    // Initial deposit of 0.1 SOL (100,000,000 lamports) and borrow of 0.1 USDH (USDH1SM1ojwWUga67PGrgFWUHibbjqMvuMaDkRJTgkX)
    // should reuslt in having two LendingUserDooots with SOL and USDH mints and the corresponding balances
    // usdh borrow should include 0.5% fee (amount = 1,005,000)
    #[tokio::test]
    async fn test_hubble_deposit_plus_borrow() {
        let dooots = setup_hubble_test("deposit_plus_borrow.json").await;

        // find dooot with SOL mint
        let sol_dooot = dooots
            .iter()
            .find(|d| match d {
                Dooot::LendingUser(du) => du.mint == "So11111111111111111111111111111111111111112",
                _ => false,
            })
            .expect("Unable to find SOL dooot");

        let usdh_dooot = dooots
            .iter()
            .find(|d| match d {
                Dooot::LendingUser(du) => du.mint == "USDH1SM1ojwWUga67PGrgFWUHibbjqMvuMaDkRJTgkX",
                _ => false,
            })
            .expect("Unable to find USDH dooot");

        if let Dooot::LendingUser(du) = sol_dooot {
            assert_eq!(du.deposits, Decimal::from(100000000));
        }

        if let Dooot::LendingUser(du) = usdh_dooot {
            assert_eq!(du.borrows, Decimal::from(1005000));
        }
    }

    // Borrow of 1 USDH (USDH1SM1ojwWUga67PGrgFWUHibbjqMvuMaDkRJTgkX) against existing position
    // should result in having LendingUserDooot with that mint and borrows of 2,010,005 (last borrow + new borrow + 0.5% fee))
    #[tokio::test]
    async fn test_hubble_borrow() {
        let dooots = setup_hubble_test("borrow.json").await;

        // find dooot with USDH mint
        let dooot = dooots
            .iter()
            .find(|d| match d {
                Dooot::LendingUser(du) => du.mint == "USDH1SM1ojwWUga67PGrgFWUHibbjqMvuMaDkRJTgkX",
                _ => false,
            })
            .expect("Unable to find dooot");

        // Check if the new balance is 2,010,005
        if let Dooot::LendingUser(du) = dooot {
            assert_eq!(du.borrows, Decimal::from(2010005));
        }
    }

    #[tokio::test]
    // Repay of 1 USDH (USDH1SM1ojwWUga67PGrgFWUHibbjqMvuMaDkRJTgkX)
    // should result in having LendingUserDooot with that mint and borrows of 1,010,007
    async fn test_hubble_borrow_repay_part() {
        let dooots = setup_hubble_test("borrow_repay_part.json").await;

        let dooot = dooots
            .iter()
            .find(|d| match d {
                Dooot::LendingUser(du) => du.mint == "USDH1SM1ojwWUga67PGrgFWUHibbjqMvuMaDkRJTgkX",
                _ => false,
            })
            .expect("Unable to find dooot");

        // Check if the new balance is 1,010,007
        if let Dooot::LendingUser(du) = dooot {
            assert_eq!(du.borrows, Decimal::from(1010007));
        }
    }

    // Repay USDH fully (USDH1SM1ojwWUga67PGrgFWUHibbjqMvuMaDkRJTgkX)
    // should result in having LendingUserDooot with that mint and borrows of 0 (borrow shares should be Some(0))
    #[tokio::test]
    async fn test_hubble_borrow_repay_fully_part() {
        let dooots = setup_hubble_test("borrow_repay_fully.json").await;

        let dooot = dooots
            .iter()
            .find(|d| match d {
                Dooot::LendingUser(du) => du.mint == "USDH1SM1ojwWUga67PGrgFWUHibbjqMvuMaDkRJTgkX",
                _ => false,
            })
            .expect("Unable to find dooot");

        // Check if the new balance is 0
        if let Dooot::LendingUser(du) = dooot {
            assert_eq!(du.borrows, Decimal::from(0));
            assert_eq!(du.borrow_shares, Some(Decimal::from(0)));
        }
    }
}
