use anyhow::Result;
use async_trait::async_trait;
use log::debug;
use rust_decimal::Decimal;
use step_ingestooor_sdk::{
    dooot::{Dooot, MintUnderlyingsGlobalDooot},
    trait_defs::transaction::TransactionLike,
    utils::{
        constants::{STEP_MINT, XSTEP_MINT, XSTEP_PROGRAM_ID, XSTEP_VAULT},
        step_utils::StepUtils,
    },
};

pub struct XStepStaking;

#[async_trait]
impl crate::TransactionParserTrait for XStepStaking {
    fn get_name(&self) -> &'static str {
        "xStep staking"
    }
    fn get_filters(&self) -> Vec<crate::TransactionParserFilter> {
        vec![
            crate::TransactionParserFilter::AccountId(XSTEP_MINT.to_string()),
            crate::TransactionParserFilter::AccountId(XSTEP_VAULT.to_string()),
        ]
    }
    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        // Check for transfers to/from ANYxx
        // Check for mint or burns of xSTEP
        // Use the cache to store the xSTEP supply and the vault balance
        // We only want to read the cache if one value **CHANGES** at some point during the txn,
        // Otherwise we'd get the same ratio values over and over again for unrelated txns
        let mut supply: Option<u64> = None;
        let mut supply_changed = false;
        let mut vault_balance: Option<u64> = None;
        let mut vault_balance_changed = false;

        for acct in decoded_transaction.iter_accounts() {
            if supply.is_some() && vault_balance.is_some() {
                // Done looking, we found both
                break;
            }

            match acct.pubkey {
                XSTEP_MINT => {
                    if supply.is_some() {
                        continue;
                    }

                    let Some(data) = acct.unpack_post_datum()? else {
                        return Ok(());
                    };
                    supply = Some(u64::from_le_bytes(data[36..44].try_into()?));
                    supply_changed = acct.datum_changed;
                }
                XSTEP_VAULT => {
                    if vault_balance.is_some() {
                        continue;
                    }

                    let Some(data) = acct.unpack_post_datum()? else {
                        return Ok(());
                    };
                    vault_balance = Some(u64::from_le_bytes(data[64..72].try_into()?));
                    vault_balance_changed = acct.datum_changed;
                }
                _ => continue,
            }
        }

        if !supply_changed && !vault_balance_changed {
            // No change, nothing to do
            return Ok(());
        }

        let (Some(supply), Some(vault_balance)) = (supply, vault_balance) else {
            return Ok(());
        };

        let amt_per_parent = (vault_balance as f64) / (supply as f64);
        debug!(
            "Got xSTEP supply: {}, vault balance: {} for sig: {}",
            supply,
            vault_balance,
            decoded_transaction.get_signature()
        );
        dooot.push(Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
            mint_pubkey: XSTEP_MINT.to_owned(),
            platform_program_pubkey: XSTEP_PROGRAM_ID.to_owned(),
            mints: vec![STEP_MINT.to_owned()],
            mints_qty_per_one_parent: vec![amt_per_parent],
            time: decoded_transaction.get_block_time(),
            total_underlying_amounts: vec![Decimal::from(vault_balance)],
            discriminant_id: XSTEP_PROGRAM_ID.to_owned(),
        }));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::TransactionParserTrait;
    use step_ingestooor_sdk::{
        decoded_transaction::DecodedTransaction, utils::util_fns::unpack_mint_any_size,
    };

    #[tokio::test]
    async fn check_ratio_values() {
        let decoded_txn =
            DecodedTransaction::load_rabbit_message_from_file("tests/xstep/full_stake_ix.json")
                .unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = XStepStaking;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooot.len(), 1);

        let Dooot::MintUnderlyingsGlobal(mint_underlying) = &dooot[0] else {
            panic!("Expected MintUnderlyings dooot, got something else");
        };

        let data_after = decoded_txn
            .get_account_data_after(XSTEP_MINT)
            .unwrap()
            .unwrap();
        let mint = unpack_mint_any_size(&data_after).unwrap();

        let expected_xstep_supply = 98009928346898808u64;
        let expected_vault_balance = Decimal::from(153559593001463826u64);

        assert_eq!(mint.supply, expected_xstep_supply);
        assert_eq!(
            mint_underlying.total_underlying_amounts[0],
            expected_vault_balance
        );
    }
}
