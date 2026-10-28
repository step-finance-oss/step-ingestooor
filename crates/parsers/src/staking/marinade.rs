use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use borsh::BorshDeserialize;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use step_ingestooor_idl_gens::marinade_finance_gen::marinade_finance::accounts::State;
use step_ingestooor_sdk::dooot::{Dooot, MintUnderlyingsGlobalDooot};
use step_ingestooor_sdk::trait_defs::transaction::TransactionLike;
use step_ingestooor_sdk::utils::step_utils::StepUtils;
pub struct MarinadeFinance;

pub const MARINADE_MSOL_MINT: &str = "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So";
pub const MARINADE_STATE: &str = "8szGkuLTAux9XMgZ2vtY39jVSowEcpBfFfD8hXSEqdGC";
pub const MARINADE_PROGRAM_ID: &str = "MarBmsSgKXdrN1egZf5sqe1TMai9K1rChYNDJgjq7aD";
pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";

impl MarinadeFinance {
    pub fn get_total_sol(data: &[u8]) -> Result<Decimal> {
        let mut reader = &data[8..];
        let state = State::deserialize(&mut reader)
            .map_err(|e| anyhow!("Error deserializing state_after: {:?}", e))?;
        let total_active_balance = Decimal::from(state.validator_system.total_active_balance);
        let total_cooling_down = Decimal::from(state.stake_system.delayed_unstake_cooling_down)
            + Decimal::from(state.emergency_cooling_down);
        let reserve = Decimal::from(state.available_reserve_balance);
        let circulating_ticket_balance = Decimal::from(state.circulating_ticket_balance);

        Ok(total_active_balance + total_cooling_down + reserve - circulating_ticket_balance)
    }
    pub fn get_msol_supply(data: &[u8]) -> Result<Decimal> {
        let mut reader = &data[8..]; // Skip the discriminator
        let state = State::deserialize(&mut reader)
            .map_err(|e| anyhow!("Error deserializing state_after: {:?}", e))?;
        Ok(Decimal::from(state.msol_supply))
    }
    pub fn get_total_active_balance(data: &[u8]) -> Result<Decimal> {
        let mut reader = &data[8..]; // Skip the discriminator
        let state = State::deserialize(&mut reader).map_err(|e| {
            anyhow!(
                "Error deserializing state_after while getting total: {:?}",
                e
            )
        })?;
        Ok(Decimal::from(state.validator_system.total_active_balance))
    }
}

#[async_trait]
impl TransactionParserTrait for MarinadeFinance {
    fn get_name(&self) -> &'static str {
        "Marinade Finance MSOL Staking"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::AccountId(
            MARINADE_STATE.to_string(),
        )]
    }

    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> anyhow::Result<()> {
        let mut accounts = decoded_transaction
            .iter_accounts_filtered(|a| a.pubkey == MARINADE_STATE)
            .filter(|a| a.datum_changed);

        // Use the first (and only) account if it exists
        if let Some(account) = accounts.next() {
            let post_data_option = account.unpack_post_datum().context("expected str data")?;

            let post_data = match post_data_option {
                Some(data) => data,
                None => return Err(anyhow!("post_data is None")),
            };

            let total_sol_supply =
                Self::get_total_sol(&post_data).context("Error getting total sol supply")?;
            let msol_supply =
                Self::get_msol_supply(&post_data).context("Error getting msol supply")?;
            let total_active_balance = Self::get_total_active_balance(&post_data)
                .context("Error getting total active balance")?;
            let price_per_sol = if msol_supply.is_zero() {
                Decimal::zero()
            } else {
                total_sol_supply / msol_supply
            };
            let dooot_underlying = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                time: decoded_transaction.get_block_time(),
                mint_pubkey: MARINADE_MSOL_MINT.to_owned(),
                platform_program_pubkey: MARINADE_PROGRAM_ID.to_string(),
                mints: vec![SOL_MINT.to_string()],
                mints_qty_per_one_parent: vec![price_per_sol.to_f64().unwrap_or(0.0)],
                total_underlying_amounts: vec![total_active_balance],
                discriminant_id: MARINADE_STATE.to_string(),
            });
            dooot.push(dooot_underlying);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, rc::Rc};
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    use TransactionParserTrait;

    #[tokio::test]
    async fn invoke_test() {
        let path = "./tests/marinade/txn_marinade_2LUunymVvw83ZvPtFjxnoyECaR8SEj8M6zoRus2mYVDNvSBSSRMHpBybYU5xVSd5AkNrnPBBGq7PhehZSSPNqH7Y.json";
        let message_json = fs::read_to_string(path).unwrap();
        let decoded_txn: DecodedTransaction = serde_json::from_str(&message_json).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser: MarinadeFinance = MarinadeFinance;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 1);

        let dooot = dooot.first().unwrap();
        match dooot {
            Dooot::MintUnderlyingsGlobal(mint_underlying) => {
                assert_eq!(mint_underlying.mint_pubkey, MARINADE_MSOL_MINT);
                assert_eq!(mint_underlying.platform_program_pubkey, MARINADE_PROGRAM_ID);
                assert_eq!(mint_underlying.mints, vec![SOL_MINT.to_string()]);
                assert_eq!(
                    mint_underlying.mints_qty_per_one_parent,
                    vec![1.2226702269600085]
                );
                assert_eq!(
                    mint_underlying.total_underlying_amounts,
                    vec![Decimal::from(5178226377635173u64)]
                );
            }
            _ => panic!("Expected MintUnderlyings"),
        }
    }
}
