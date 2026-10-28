use anyhow::{anyhow, Result};
use async_trait::async_trait;
use borsh::BorshDeserialize;
use chrono::{DateTime, NaiveDateTime};
use solana_sdk::account::ReadableAccount;
use solana_sdk::pubkey;
use solana_sdk::pubkey::Pubkey;
use step_ingestooor_idl_gens::jup_dca_gen::jup_dca::{self, accounts::Dca};
use step_ingestooor_sdk::{
    dooot::{Dooot, RepeatingOrderUserDooot},
    trait_defs::{discriminatable::Discriminatable, transaction::TransactionLike},
    utils::step_utils::StepUtils,
};

use std::sync::mpsc::SyncSender;

use crate::state_source::{AccountProvider, StateSource};

pub struct JupiterDCA;

// Jupiter DCA program ID
const JUPITER_DCA_PROGRAM: &str = "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M";
const JUPITER_DCA_PROGRAM_PUBKEY: Pubkey = pubkey!("DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M");

#[async_trait]
impl crate::TransactionParserTrait for JupiterDCA {
    fn get_name(&self) -> &'static str {
        "JupiterDCA"
    }

    fn get_filters(&self) -> Vec<crate::TransactionParserFilter> {
        vec![crate::TransactionParserFilter::ProgramId(
            JUPITER_DCA_PROGRAM.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        // Find accounts created or modified by this transaction
        let dca_accounts = txn.get_accounts_typed_borsh::<Dca>()?;
        let events = txn.get_anchor_log_cpi_events_typed(JUPITER_DCA_PROGRAM)?;

        for account in dca_accounts.iter() {
            let dooot = match (account.pre_bytes_exist(), account.post_bytes_exist()) {
                (_, true) => {
                    //opened
                    let dca = account
                        .get_post()?
                        .ok_or(anyhow!("Failed to get post data"))?;
                    let dooot = create_repeating_order_dooot(
                        &dca,
                        account.pubkey.to_string(),
                        txn.get_block_time(),
                        false,
                        &events,
                    )?;
                    Some(Dooot::RepeatingOrderUser(dooot))
                }
                (true, false) => {
                    //closed
                    let dca = account
                        .get_pre()?
                        .ok_or(anyhow!("Failed to get pre data"))?;

                    let dooot = create_repeating_order_dooot(
                        &dca,
                        account.pubkey.to_string(),
                        txn.get_block_time(),
                        true,
                        &events,
                    )?;
                    Some(Dooot::RepeatingOrderUser(dooot))
                }
                (false, false) => {
                    //doesnt exist at all?!
                    //this cant happen
                    None
                }
            };
            if let Some(dooot) = dooot {
                dooots.push(dooot);
            }
        }

        Ok(())
    }
}

fn create_repeating_order_dooot(
    dca: &Dca,
    position_pubkey: String,
    block_time: NaiveDateTime,
    is_closed: bool,
    events: &[jup_dca::utils::Event],
) -> Result<RepeatingOrderUserDooot> {
    // Convert Unix timestamps to NaiveDateTime using the new method
    let created_at_dt = DateTime::from_timestamp(dca.created_at, 0)
        .ok_or_else(|| anyhow!("Invalid created_at timestamp"))?;
    let created_at = created_at_dt.naive_utc();

    let next_order_at = if is_closed {
        None
    } else {
        Some(
            DateTime::from_timestamp(dca.next_cycle_at, 0)
                .ok_or_else(|| anyhow!("Invalid next_cycle_at timestamp"))?
                .naive_utc(),
        )
    };

    let mut dooot = RepeatingOrderUserDooot {
        time: block_time,
        program_pubkey: JUPITER_DCA_PROGRAM.to_string(),
        strategy_type: "dca".to_string(),
        user_pubkey: dca.user.to_string(),
        position_pubkey: position_pubkey.clone(),
        input_mint_pubkey: dca.input_mint.to_string(),
        input_account_pubkey: dca.in_account.to_string(),
        output_mint_pubkey: dca.output_mint.to_string(),
        output_account_pubkey: dca.out_account.to_string(),
        sale_amount_mint_pubkey: dca.input_mint.to_string(), // For DCA, the sale amount mint is the input mint
        sale_amount_per_cycle: dca.in_amount_per_cycle,
        min_out_amount: if dca.min_out_amount == 0 {
            None
        } else {
            Some(dca.min_out_amount)
        },
        max_out_amount: if dca.max_out_amount == 0 {
            None
        } else {
            Some(dca.max_out_amount)
        },
        in_deposited: dca.in_deposited,
        in_withdrawn: dca.in_withdrawn,
        in_used: dca.in_used,
        out_received: dca.out_received,
        out_withdrawn: dca.out_withdrawn,
        created_at,
        next_order_at,
        order_interval: dca.cycle_frequency as u64, // Converting i64 to u64
        closed: is_closed,
    };

    if is_closed {
        //in the case of an early close, we can't capture the update to the withdrawn fields
        //so we have to find the event related to the close and use the values from there
        let Some(event) = events
            .iter()
            .filter_map(|event| match event {
                jup_dca::utils::Event::Closed(event) => Some(event),
                _ => None,
            })
            .find(|event| event.dca_key.to_string() == position_pubkey)
        else {
            return Err(anyhow!("No event found for position pubkey"));
        };
        dooot.in_withdrawn = event.total_in_withdrawn;
        dooot.in_deposited = event.in_deposited;
        dooot.in_used = dooot.in_deposited - dooot.in_withdrawn;

        dooot.out_withdrawn = event.total_out_withdrawn;
        dooot.out_received = dooot.out_withdrawn;
    }

    Ok(dooot)
}

impl StateSource for JupiterDCA {
    fn get_name(&self) -> &'static str {
        "JupiterDCA"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        // Get all Dcas using their discriminator
        let accts = acct_provider
            .program_accounts(JUPITER_DCA_PROGRAM_PUBKEY, Some(Dca::discriminator().1));

        for acct in accts {
            let data = &acct.account.data()[Dca::discriminator().1.len()..];

            // Skip if not enough data
            if data.len() < 200 {
                continue;
            }

            // Deserialize the DCA account
            match Dca::try_from_slice(data) {
                Ok(dca) => {
                    // Create a dooot for each DCA account
                    let dooot = create_repeating_order_dooot(
                        &dca,
                        acct.pubkey.to_string(),
                        DateTime::from_timestamp(acct_provider.get_block_time(), 0)
                            .unwrap_or_default()
                            .naive_utc(),
                        false, // Not closed during state init
                        &Vec::new(),
                    )?;

                    tx.send(Dooot::RepeatingOrderUser(dooot))?;
                }
                Err(err) => {
                    log::warn!("Error deserializing Dca: {:?}", err);
                    continue;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, rc::Rc};

    use crate::TransactionParserTrait;
    use serde_json::Value;
    use step_ingestooor_sdk::{decoded_transaction::DecodedTransaction, dooot::Dooot};

    use super::*;

    async fn run_test_case(file_name: &str) -> Vec<Dooot> {
        // Load test transaction json
        let message_json =
            fs::read_to_string(format!("./tests/jupiter/{}.json", file_name)).unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooots = vec![];
        let parser = JupiterDCA;
        parser
            .invoke(&decoded_txn, &mut dooots, &step_utils)
            .await
            .unwrap();

        dooots
    }

    #[tokio::test]
    async fn test_dca_create() {
        // Test parsing a transaction that creates a DCA position
        let dooots = run_test_case("dca_create").await;

        assert!(
            !dooots.is_empty(),
            "Expected at least one dooot for DCA creation"
        );

        if let Dooot::RepeatingOrderUser(ro) = &dooots[0] {
            assert_eq!(ro.time.to_string(), "2025-04-04 04:32:17.200031");
            assert_eq!(ro.program_pubkey, JUPITER_DCA_PROGRAM);
            assert_eq!(ro.strategy_type, "dca");
            // Verify some key fields
            assert_eq!(
                ro.user_pubkey,
                "DanayVvgHg3dFjhJgjNECxU589vGjzmwRBBZ7nCMVzUg",
            );
            assert_eq!(
                ro.position_pubkey,
                "J3indrzYy8T2BCfPRVKATDs2Nbm3SZxEA3GyCMdNDzzg",
            );
            assert_eq!(
                ro.input_mint_pubkey,
                "So11111111111111111111111111111111111111112",
            );
            assert_eq!(
                ro.input_account_pubkey,
                "J8KLn7y7QmN2kNSBZh1UPvBf8ciZLXPUiV1RqhLtVmkc",
            );
            assert_eq!(
                ro.output_mint_pubkey,
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            );
            assert_eq!(
                ro.output_account_pubkey,
                "9CtHFDSeZPVcL2LRQqKBbVuy6SMmBfUPvW7kzvjhg1Ud",
            );
            assert_eq!(
                ro.sale_amount_mint_pubkey,
                "So11111111111111111111111111111111111111112",
            );
            assert_eq!(ro.in_deposited, 900000000, "In deposited should match");
            assert_eq!(ro.in_withdrawn, 0, "In withdrawn should match");
            assert_eq!(ro.in_used, 0, "In used should match");
            assert_eq!(ro.out_received, 0, "Out received should match");
            assert_eq!(ro.out_withdrawn, 0, "Out withdrawn should match");
            assert_eq!(ro.created_at.to_string(), "2025-04-04 04:32:17",);
            assert_eq!(ro.next_order_at.unwrap().to_string(), "2025-04-04 04:32:17",);
            assert_eq!(ro.order_interval, 60);
            assert!(!ro.closed);
        } else {
            panic!("Expected a RepeatingOrderUser dooot");
        }
    }

    #[tokio::test]
    async fn test_dca_fill() {
        // Test parsing a transaction that fills a DCA order
        let dooots = run_test_case("dca_fill").await;

        // This might not produce a dooot if we're only tracking creations/closes
        // If it does produce a dooot, verify it's correct
        if !dooots.is_empty() {
            if let Dooot::RepeatingOrderUser(ro) = &dooots[0] {
                assert_eq!(ro.time.to_string(), "2025-04-04 04:36:32.001326");
                assert_eq!(ro.program_pubkey, JUPITER_DCA_PROGRAM);
                assert_eq!(ro.strategy_type, "dca");
                assert_eq!(
                    ro.user_pubkey,
                    "DanayVvgHg3dFjhJgjNECxU589vGjzmwRBBZ7nCMVzUg"
                );
                assert_eq!(
                    ro.position_pubkey,
                    "7YMkGudRHMDjFrmXMkXSffdwAbb7xtb7wD6cSTm1k4Ev"
                );
                assert_eq!(
                    ro.input_mint_pubkey,
                    "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
                );
                assert_eq!(
                    ro.input_account_pubkey,
                    "EPTb5mGqEY5Lv3vzRuFh2u2cr6jTn66ubzhEWb2CxcaE"
                );
                assert_eq!(
                    ro.output_mint_pubkey,
                    "So11111111111111111111111111111111111111112"
                );
                assert_eq!(
                    ro.output_account_pubkey,
                    "Ery3ZRGQsC9cduBAmVyC2ShyN7fJoGqaaHnTCbSdgnKe"
                );
                assert_eq!(
                    ro.sale_amount_mint_pubkey,
                    "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
                );
                assert_eq!(ro.sale_amount_per_cycle, 50000000);
                assert_eq!(ro.in_deposited, 100000000);
                assert_eq!(ro.in_withdrawn, 0);
                assert_eq!(ro.in_used, 100000000);
                assert_eq!(ro.out_received, 866779841);
                assert_eq!(ro.out_withdrawn, 866779841);
                assert_eq!(ro.created_at.to_string(), "2025-04-04 04:35:23");
                assert_eq!(ro.next_order_at.unwrap().to_string(), "2025-04-04 04:37:24");
                assert_eq!(ro.order_interval, 60);
                assert!(!ro.closed);
            } else {
                panic!("Expected a RepeatingOrderUser dooot");
            }
        }
    }

    #[tokio::test]
    async fn test_dca_close() {
        // Test parsing a transaction that closes a DCA position
        let dooots = run_test_case("dca_close").await;

        assert!(
            !dooots.is_empty(),
            "Expected at least one dooot for DCA close"
        );

        if let Dooot::RepeatingOrderUser(ro) = &dooots[0] {
            assert_eq!(ro.time.to_string(), "2025-04-04 04:36:50.101177");
            assert_eq!(ro.program_pubkey, JUPITER_DCA_PROGRAM);
            assert_eq!(ro.strategy_type, "dca");
            assert_eq!(
                ro.user_pubkey,
                "DanayVvgHg3dFjhJgjNECxU589vGjzmwRBBZ7nCMVzUg"
            );
            assert_eq!(
                ro.position_pubkey,
                "7YMkGudRHMDjFrmXMkXSffdwAbb7xtb7wD6cSTm1k4Ev"
            );
            assert_eq!(
                ro.input_mint_pubkey,
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
            );
            assert_eq!(
                ro.input_account_pubkey,
                "EPTb5mGqEY5Lv3vzRuFh2u2cr6jTn66ubzhEWb2CxcaE"
            );
            assert_eq!(
                ro.output_mint_pubkey,
                "So11111111111111111111111111111111111111112"
            );
            assert_eq!(
                ro.output_account_pubkey,
                "Ery3ZRGQsC9cduBAmVyC2ShyN7fJoGqaaHnTCbSdgnKe"
            );
            assert_eq!(
                ro.sale_amount_mint_pubkey,
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
            );
            assert_eq!(ro.sale_amount_per_cycle, 50000000);
            assert_eq!(ro.in_deposited, 100000000);
            assert_eq!(ro.in_withdrawn, 0);
            assert_eq!(ro.in_used, 100000000);
            assert_eq!(ro.out_received, 866779841);
            assert_eq!(ro.out_withdrawn, 866779841);
            assert_eq!(ro.created_at.to_string(), "2025-04-04 04:35:23");
            assert_eq!(ro.next_order_at, None);
            assert_eq!(ro.order_interval, 60);
            assert!(ro.closed);
        } else {
            panic!("Expected a RepeatingOrderUser dooot");
        }
    }

    #[tokio::test]
    async fn test_dca_early_close() {
        // Test parsing a transaction that early-closes a DCA position
        let dooots = run_test_case("dca_early_close").await;

        assert!(
            !dooots.is_empty(),
            "Expected at least one dooot for early DCA close"
        );

        if let Dooot::RepeatingOrderUser(ro) = &dooots[0] {
            assert_eq!(ro.time.to_string(), "2025-04-04 04:40:13.200050");
            assert_eq!(ro.program_pubkey, JUPITER_DCA_PROGRAM);
            assert_eq!(ro.strategy_type, "dca");
            assert_eq!(
                ro.user_pubkey,
                "DanayVvgHg3dFjhJgjNECxU589vGjzmwRBBZ7nCMVzUg"
            );
            assert_eq!(
                ro.position_pubkey,
                "DdmnSwq2AfGsKKNPRxL2UEZ1Lit18dSiUkySkwCHT19d"
            );
            assert_eq!(
                ro.input_mint_pubkey,
                "So11111111111111111111111111111111111111112"
            );
            assert_eq!(
                ro.input_account_pubkey,
                "F6X7Ai3JdVhYUnrLtwU812Fj4eFcPbfpKcFt9bSp2NWt"
            );
            assert_eq!(
                ro.output_mint_pubkey,
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
            );
            assert_eq!(
                ro.output_account_pubkey,
                "DnYaET6HswWKw6SCYWoKYyFuxXQ7ZmiFoxNqRSRVMcnq"
            );
            assert_eq!(
                ro.sale_amount_mint_pubkey,
                "So11111111111111111111111111111111111111112"
            );
            assert_eq!(ro.sale_amount_per_cycle, 450000000);
            assert_eq!(ro.in_deposited, 900000000);
            assert_eq!(ro.in_withdrawn, 450000000);
            assert_eq!(ro.in_used, 450000000);
            assert_eq!(ro.out_received, 51751468);
            assert_eq!(ro.out_withdrawn, 51751468);
            assert_eq!(ro.created_at.to_string(), "2025-04-04 04:39:42");
            assert_eq!(ro.next_order_at, None);
            assert_eq!(ro.order_interval, 60);
            assert!(ro.closed);
        } else {
            panic!("Expected a RepeatingOrderUser dooot");
        }
    }

    #[tokio::test]
    async fn test_dca_end_and_close() {
        // Test parsing a transaction that closes a DCA position
        let dooots = run_test_case("dca_end_and_close").await;

        assert!(
            !dooots.is_empty(),
            "Expected at least one dooot for DCA close"
        );

        if let Dooot::RepeatingOrderUser(ro) = &dooots[0] {
            assert_eq!(ro.time.to_string(), "2025-06-19 05:10:23.001280");
            assert_eq!(ro.program_pubkey, JUPITER_DCA_PROGRAM);
            assert_eq!(ro.strategy_type, "dca");
            assert_eq!(
                ro.user_pubkey,
                "3VpWnVhLYX7RkBhu2Af7oaa4FwuX1DbFKzJymLoXYuxi"
            );
            assert_eq!(
                ro.position_pubkey,
                "6H3UmRxVCWJwP3YDQK11FTLCYkJd7hHYgvYZ9GwWzSMn"
            );
            assert_eq!(
                ro.input_mint_pubkey,
                "4ELGifwr2jHtEaFZUXXvQLAYiE6W5bvpWYZbJTRSpump"
            );
            assert_eq!(
                ro.input_account_pubkey,
                "Gw8CYW9FMxtbiW3DEd4Xcz1s4nTngzTN5KMZwHdb1hzu"
            );
            assert_eq!(
                ro.output_mint_pubkey,
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
            );
            assert_eq!(
                ro.output_account_pubkey,
                "CcnpDU1sxh326qBWyta2g98aSLbai1UkpU7Dfv2S71ej"
            );
            assert_eq!(
                ro.sale_amount_mint_pubkey,
                "4ELGifwr2jHtEaFZUXXvQLAYiE6W5bvpWYZbJTRSpump"
            );
            assert_eq!(ro.sale_amount_per_cycle, 107266982549);
            assert_eq!(ro.in_deposited, 1287203790594);
            assert_eq!(ro.in_withdrawn, 0);
            assert_eq!(ro.in_used, 1287203790594);
            assert_eq!(ro.out_received, 808311905);
            assert_eq!(ro.out_withdrawn, 808311905);
            assert_eq!(ro.created_at.to_string(), "2025-06-18 18:10:04");
            assert_eq!(ro.next_order_at, None);
            assert_eq!(ro.order_interval, 3600);
            assert!(ro.closed);
        } else {
            panic!("Expected a RepeatingOrderUser dooot");
        }
    }
}
