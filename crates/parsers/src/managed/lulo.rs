use anyhow::Result;
use async_trait::async_trait;
use borsh::BorshDeserialize;
use solana_sdk::account::ReadableAccount;
use solana_sdk::pubkey;
use solana_sdk::pubkey::Pubkey;
use step_ingestooor_sdk::dooot::{
    ClaimableBalanceUserDooot, Dooot, ManagedAccountUserDooot, MintUnderlyingsGlobalDooot,
};
use step_ingestooor_sdk::trait_defs::transaction::TransactionLike;
use step_ingestooor_sdk::utils::util_fns::convert_blocktime_to_micros;

use crate::state_source::StateSource;
use crate::{TransactionParserFilter, TransactionParserTrait};
use step_ingestooor_idl_gens::lulo_gen::lulo::accounts::{pendingWithdrawal, pool, userAccount};
use step_ingestooor_sdk::utils::step_utils::StepUtils;

pub const LULO_PROGRAM: &str = "FL3X2pRsQ9zHENpZSKDRREtccwJuei8yg9fwDu9UN69Q";
pub const LULO_PROGRAM_PUBKEY: Pubkey = pubkey!("FL3X2pRsQ9zHENpZSKDRREtccwJuei8yg9fwDu9UN69Q");

pub const USER_ACCOUNT_DISC: &[u8] = &[0xd3, 0x21, 0x88, 0x10, 0xba, 0x6e, 0xf2, 0x7f];
pub const POOL_DISC: &[u8] = &[0xf1, 0x9a, 0x6d, 0x04, 0x11, 0xb1, 0x6d, 0xbc];
pub const PENDING_WITHDRAWALS: &[u8] = &[0x3d, 0x67, 0xb3, 0xb1, 0x94, 0xc7, 0x3f, 0xab];
pub const BOOSTED_MINT: &str = "5vRM838fdN83YMK4Xau4pLyt9sC1cfvsQbHce8t35wj5";
pub const PROTECTED_MINT: &str = "DGuwEKWqapDjicWinSk8GaPx35pn5vvXAjXyd8rAwE4u";
pub const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

pub struct Lulo;

#[async_trait]
impl TransactionParserTrait for Lulo {
    fn get_name(&self) -> &'static str {
        "Lulo"
    }
    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(LULO_PROGRAM.to_string())]
    }
    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = decoded_transaction.get_block_time();
        let post_accts: Vec<_> = decoded_transaction
            .iter_accounts()
            .filter(|a| a.post_owner_program.is_some_and(|p| p == LULO_PROGRAM))
            .collect();

        for acct in decoded_transaction.iter_accounts() {
            if acct.pre_owner_program.is_none_or(|p| p != LULO_PROGRAM)
                && acct.post_owner_program.is_none_or(|p| p != LULO_PROGRAM)
            {
                continue;
            }
            if acct.was_created {
                let Some(post) = acct.unpack_post_datum()? else {
                    log::warn!("Failed to unpack post datum for Lulo.");
                    continue;
                };
                let disc = &post[..8];
                if disc == USER_ACCOUNT_DISC {
                    let user_acc = userAccount::deserialize(&mut &post[8..])?;
                    let managed_acct = Dooot::ManagedAccountUser(ManagedAccountUserDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        user_pubkey: user_acc.owner.to_string(),
                        program_id: LULO_PROGRAM.to_string(),
                        closed: false,
                    });
                    dooot.push(managed_acct);
                }
            } else if acct.was_closed {
                let Some(pre) = acct.unpack_pre_datum()? else {
                    log::warn!("Failed to unpack pre datum for Lulo.");
                    continue;
                };
                let disc = &pre[..8];
                if disc == USER_ACCOUNT_DISC {
                    let user_acc = userAccount::deserialize(&mut &pre[8..])?;
                    let managed_acct = Dooot::ManagedAccountUser(ManagedAccountUserDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        user_pubkey: user_acc.owner.to_string(),
                        program_id: LULO_PROGRAM.to_string(),
                        closed: true,
                    });
                    dooot.push(managed_acct);
                }
                if disc == PENDING_WITHDRAWALS {
                    let pending = pendingWithdrawal::deserialize(&mut &pre[8..])?;
                    let claimable = Dooot::ClaimableBalanceUser(ClaimableBalanceUserDooot {
                        time,
                        amount: 0.into(),
                        mint: USDC_MINT.to_string(),
                        owner: pending.owner.to_string(),
                        program: LULO_PROGRAM.to_string(),
                        relates_to: acct.pubkey.to_string(),
                        relates_to_type: "l".to_string(),
                    });
                    dooot.push(claimable)
                }
            }
        }

        for acct in post_accts {
            let Some(post) = acct.unpack_post_datum()? else {
                log::warn!("Failed to unpack post datum for Lulo pools.");
                continue;
            };
            let disc = &post[..8];
            if !acct.datum_changed {
                continue;
            }
            if disc == POOL_DISC {
                let pool = pool::deserialize(&mut &post[8..])?;
                // 0 being the first index in the allocations array on the pool
                // They only have one allocation atm so hardcoding it for now
                let discriminant_id = format!("{}-{}", acct.pubkey, 0);

                let amounts = self.get_amounts(pool);

                let protected = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                    time,
                    mint_pubkey: PROTECTED_MINT.to_string(),
                    platform_program_pubkey: LULO_PROGRAM.to_string(),
                    mints: vec![USDC_MINT.to_string()],
                    total_underlying_amounts: vec![amounts[0].0.into()],
                    mints_qty_per_one_parent: vec![amounts[0].1],
                    discriminant_id: discriminant_id.clone(),
                });

                let boosted = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                    time,
                    mint_pubkey: BOOSTED_MINT.to_string(),
                    platform_program_pubkey: LULO_PROGRAM.to_string(),
                    mints: vec![USDC_MINT.to_string()],
                    total_underlying_amounts: vec![amounts[1].0.into()],
                    mints_qty_per_one_parent: vec![amounts[1].1],
                    discriminant_id,
                });

                dooot.push(protected);
                dooot.push(boosted);
            }
            if disc == PENDING_WITHDRAWALS {
                let pending = pendingWithdrawal::deserialize(&mut &post[8..])?;
                let claimable = Dooot::ClaimableBalanceUser(ClaimableBalanceUserDooot {
                    time,
                    amount: pending.native_amount.into(),
                    mint: USDC_MINT.to_string(),
                    owner: pending.owner.to_string(),
                    program: LULO_PROGRAM.to_string(),
                    relates_to: acct.pubkey.to_string(),
                    relates_to_type: "l".to_string(),
                });
                dooot.push(claimable)
            }
        }

        Ok(())
    }
}

impl StateSource for Lulo {
    fn get_name(&self) -> &'static str {
        "Lulo"
    }
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let accts = acct_provider.program_accounts(LULO_PROGRAM_PUBKEY, None);
        let block_time = acct_provider.get_block_time();
        let time = convert_blocktime_to_micros(block_time, Some(0), None)?;
        // grouping doots so when we write there is no timeouts
        let mut all_doots = vec![];

        for acct in accts {
            let data = acct.account.data();
            let disc = &data[..8];
            if disc == USER_ACCOUNT_DISC {
                let user_acc = userAccount::deserialize(&mut &data[8..])?;
                let managed_acct = Dooot::ManagedAccountUser(ManagedAccountUserDooot {
                    time,
                    account_pubkey: acct.pubkey.to_string(),
                    user_pubkey: user_acc.owner.to_string(),
                    program_id: LULO_PROGRAM.to_string(),
                    closed: false,
                });
                all_doots.push(managed_acct);
            }
            if disc == POOL_DISC {
                let pool = pool::deserialize(&mut &data[8..])?;

                let discriminant_id = format!("{}-{}", acct.pubkey, 0);

                let amounts = self.get_amounts(pool);

                let protected = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                    time,
                    mint_pubkey: PROTECTED_MINT.to_string(),
                    platform_program_pubkey: LULO_PROGRAM.to_string(),
                    mints: vec![USDC_MINT.to_string()],
                    total_underlying_amounts: vec![amounts[0].0.into()],
                    mints_qty_per_one_parent: vec![amounts[0].1],
                    discriminant_id: discriminant_id.clone(),
                });

                let boosted = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                    time,
                    mint_pubkey: BOOSTED_MINT.to_string(),
                    platform_program_pubkey: LULO_PROGRAM.to_string(),
                    mints: vec![USDC_MINT.to_string()],
                    total_underlying_amounts: vec![amounts[1].0.into()],
                    mints_qty_per_one_parent: vec![amounts[1].1],
                    discriminant_id,
                });

                all_doots.push(protected);
                all_doots.push(boosted);
            }
            if disc == PENDING_WITHDRAWALS {
                let pending = pendingWithdrawal::deserialize(&mut &data[8..])?;
                let claimable = Dooot::ClaimableBalanceUser(ClaimableBalanceUserDooot {
                    time,
                    amount: pending.native_amount.into(),
                    mint: USDC_MINT.to_string(),
                    owner: pending.owner.to_string(),
                    program: LULO_PROGRAM.to_string(),
                    relates_to: acct.pubkey.to_string(),
                    relates_to_type: "l".to_string(),
                });
                all_doots.push(claimable);
            }
        }

        for doot in all_doots {
            tx.send(doot)?;
        }

        Ok(())
    }
}

impl Lulo {
    fn get_amounts(&self, pool: pool) -> Vec<(u128, f64)> {
        let scale = 10u128.pow(6); // 10^6 for 6 decimal places
        let protected_liquidity = pool.allocations[0].protected_amount as u128;
        let protected_supply = pool.protected_total_supply as u128;

        let protected_price = if protected_supply > 0 {
            (protected_liquidity * scale) / protected_supply
        } else {
            0
        };
        let protected_price = (protected_price as f64) / scale as f64;

        let boosted_liquidity = pool.allocations[0].regular_amount as u128
            - pool.allocations[0].pending_withdrawals as u128;
        let boosted_supply = pool.regular_total_supply as u128;

        let boosted_price = if boosted_supply > 0 {
            (boosted_liquidity * scale) / boosted_supply
        } else {
            0
        };
        let boosted_price = (boosted_price as f64) / scale as f64;

        vec![
            (protected_liquidity, protected_price),
            (boosted_liquidity, boosted_price),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::fs;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    use step_ingestooor_sdk::utils::test_utils::init_test_extras_bare;
    use TransactionParserTrait;

    #[tokio::test]
    async fn test_lulo_classic_deposit() {
        let f_str = fs::read_to_string("./tests/lulo/classic_deposit.json").unwrap();
        let f_json: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&f_str).unwrap())
                .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: Lulo = Lulo;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        println!("{:#?}", dooot);
        // nothing in this case since no accounts were created & classic deposits
        // go directly to 3rd party
        assert_eq!(dooot.len(), 0);
    }

    #[tokio::test]
    async fn test_lulo_boosted_deposit() {
        let f_str = fs::read_to_string("./tests/lulo/boosted_deposit.json").unwrap();
        let f_json: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&f_str).unwrap())
                .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: Lulo = Lulo;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 2);
        let Dooot::MintUnderlyingsGlobal(protected) = &dooot[0] else {
            panic!("Expected a mint underlyings global dooot");
        };
        let Dooot::MintUnderlyingsGlobal(boosted) = &dooot[1] else {
            panic!("Expected a mint underlyings global dooot");
        };

        assert_eq!(protected.mints_qty_per_one_parent, vec![1.024678]);
        assert_eq!(boosted.mints_qty_per_one_parent, vec![1.046086]);
    }

    #[tokio::test]
    async fn test_lulo_boosted_withdraw() {
        let f_str = fs::read_to_string("./tests/lulo/boosted_withdraw.json").unwrap();
        let f_json: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&f_str).unwrap())
                .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: Lulo = Lulo;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 3);
        let Dooot::ClaimableBalanceUser(withdraw) = &dooot[0] else {
            panic!("Expected a ClaimableBalanceUser dooot");
        };
        let Dooot::MintUnderlyingsGlobal(protected) = &dooot[1] else {
            panic!("Expected a mint underlyings global dooot");
        };
        let Dooot::MintUnderlyingsGlobal(boosted) = &dooot[2] else {
            panic!("Expected a mint underlyings global dooot");
        };
        assert_eq!(withdraw.amount, Decimal::from(23005195));
        assert_eq!(protected.mints_qty_per_one_parent, vec![1.024812]);
        assert_eq!(boosted.mints_qty_per_one_parent, vec![1.046306]);
    }
    #[tokio::test]
    async fn test_lulo_boosted_withdraw_2() {
        let f_str = fs::read_to_string("./tests/lulo/boosted_withdraw_2.json").unwrap();
        let f_json: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&f_str).unwrap())
                .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: Lulo = Lulo;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 3);
        let Dooot::ClaimableBalanceUser(withdraw) = &dooot[0] else {
            panic!("Expected a ClaimableBalanceUser dooot");
        };
        let Dooot::MintUnderlyingsGlobal(protected) = &dooot[1] else {
            panic!("Expected a mint underlyings global dooot");
        };

        let Dooot::MintUnderlyingsGlobal(boosted) = &dooot[2] else {
            panic!("Expected a mint underlyings global dooot");
        };

        assert_eq!(withdraw.amount, Decimal::from(2002562));
        assert_eq!(protected.mints_qty_per_one_parent, vec![1.025637]);
        assert_eq!(boosted.mints_qty_per_one_parent, vec![1.047605]);
    }

    #[tokio::test]
    async fn test_lulo_claim() {
        let f_str = fs::read_to_string("./tests/lulo/claim.json").unwrap();
        let f_json: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&f_str).unwrap())
                .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: Lulo = Lulo;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 3);
        let Dooot::ClaimableBalanceUser(withdraw) = &dooot[0] else {
            panic!("Expected a ClaimableBalanceUser dooot");
        };
        let Dooot::MintUnderlyingsGlobal(protected) = &dooot[1] else {
            panic!("Expected a mint underlyings global dooot");
        };

        let Dooot::MintUnderlyingsGlobal(boosted) = &dooot[2] else {
            panic!("Expected a mint underlyings global dooot");
        };

        assert_eq!(withdraw.amount, Decimal::from(0));
        assert_eq!(protected.mints_qty_per_one_parent, vec![1.025645]);
        assert_eq!(boosted.mints_qty_per_one_parent, vec![1.047618]);
    }
}
