use std::array::TryFromSliceError;

use crate::{vaults::carrot::state::AssetState, TransactionParserFilter, TransactionParserTrait};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rust_decimal::Decimal;
use step_ingestooor_sdk::{
    dooot::{Dooot, MintUnderlyingsGlobalDooot},
    trait_defs::transaction::TransactionLike,
    utils::step_utils::StepUtils,
};

use super::state::Vault;

pub const CARROT_VAULT_PROGRAM_ID: &str = "CarrotwivhMpDnm27EHmRLeQ683Z1PufuqEmBZvD282s";
pub const CARROT_VAULT: &str = "FfCRL34rkJiMiX5emNDrYp3MdWH2mES3FvDQyFppqgpJ";
pub const CRT_MINT: &str = "CRTx1JouZhzSU6XytsE42UQraoGqiHgxabocVfARTy2s";
pub const VAULT_ACCOUNT_DISCRIMINATOR: [u8; 8] = [0xd3, 0x08, 0xe8, 0x2b, 0x02, 0x98, 0x75, 0x77];
pub const CRT_DECIMALS: u8 = 9;

pub struct CarrotVault;

#[async_trait]
impl TransactionParserTrait for CarrotVault {
    fn get_name(&self) -> &'static str {
        "Carrot Vault"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            CARROT_VAULT_PROGRAM_ID.into(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let accts = txn
            .iter_accounts()
            .filter(|a| a.post_owner_program == Some(CARROT_VAULT_PROGRAM_ID));

        for acct in accts {
            let Some(post_data) = acct.unpack_post_datum()? else {
                log::warn!("CARROT VAULT: Missing pre datum (could be closed)");
                continue;
            };

            let Some(disc) = post_data.get(..8) else {
                log::warn!("CARROT VAULT: Missing discriminator");
                continue;
            };

            if disc != VAULT_ACCOUNT_DISCRIMINATOR {
                continue;
            }

            let Some(crt_token_supply) = txn
                .get_account_data_after(CRT_MINT)
                .ok()
                .flatten()
                .map(|b| {
                    let supply_bytes = b[36..44].try_into()?;
                    Ok(u64::from_le_bytes(supply_bytes))
                })
                .transpose()
                .map_err(|e: TryFromSliceError| anyhow!(e))?
            else {
                continue;
            };

            let vault = Vault::load(&post_data)?;

            let mut asset_state: Vec<AssetState> = Vec::with_capacity(vault.assets.len());
            for asset in vault.assets.iter() {
                let ata_amount = txn
                    .get_post_token_account_balance(&asset.ata.to_string())?
                    .ok_or_else(|| {
                        anyhow!(
                            "CARROT VAULT: ATA balance not found for {:?}",
                            asset.ata.to_string()
                        )
                    })?;

                asset_state.push(AssetState {
                    asset_id: asset.asset_id,
                    mint: asset.mint,
                    mint_decimals: asset.decimals,
                    ata_amount,
                });
            }

            let underlying_balances = vault.get_underlying_balances(&asset_state)?;
            let vault_tvl = underlying_balances.iter().sum::<u64>() as u128;

            // calculate unminted performance fees, used to adjust the shares supply
            let accumulated_performance_fee = vault.calculate_accumulated_performance_fee(
                crt_token_supply,
                CRT_DECIMALS,
                vault_tvl,
            )?;

            // adjust shares supply by unminted fees accrued
            // this is just used to have an accurate supply to calculate the management fee
            let adjusted_shares_supply_before_mgmt_fee = vault
                .fee
                .adjust_shares_by_fees(crt_token_supply, accumulated_performance_fee)?;

            // calculate management fee before deposit
            let fee_amount = vault.fee.calculate_management_fee(
                vault_tvl,
                adjusted_shares_supply_before_mgmt_fee,
                CRT_DECIMALS,
                time.and_utc().timestamp(),
            )?;

            // adjust shares supply by unminted fees accrued
            // this is now the true adjusted shares supply because it takes into account the latest fee data
            let adjusted_shares_supply = vault.fee.adjust_shares_by_fees(
                crt_token_supply
                    .checked_add(fee_amount)
                    .ok_or(anyhow!("CARROT VAULT: Invalid fee calculation"))?,
                accumulated_performance_fee,
            )?;

            let amts_per_parent = underlying_balances
                .iter()
                .map(|underlying_balance| {
                    let price_per_mint = if adjusted_shares_supply == 0 {
                        0.0
                    } else {
                        (*underlying_balance as f64) / (adjusted_shares_supply as f64)
                    };
                    Ok(price_per_mint)
                })
                .collect::<Result<Vec<_>>>()?;

            //println!("crt_token_supply: {}", crt_token_supply);
            //println!("underlying_balances: {:#?}", underlying_balances);
            //println!("adjusted_shares_supply: {}", adjusted_shares_supply);
            //println!("amts_per_parent: {:#?}", amts_per_parent);

            // NOTE: mints_qty_per_one_parent needs to be summed up to get true ratio when calculating user positions
            let mint_underlying_dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                time,
                mint_pubkey: vault.shares.to_string(),
                platform_program_pubkey: CARROT_VAULT_PROGRAM_ID.to_string(),
                mints: vault.assets.iter().map(|a| a.mint.to_string()).collect(),
                mints_qty_per_one_parent: amts_per_parent,
                total_underlying_amounts: underlying_balances
                    .iter()
                    .map(|&v| Decimal::from(v))
                    .collect(),
                discriminant_id: acct.pubkey.to_string(),
            });

            dooot.push(mint_underlying_dooot);
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

    use crate::vaults::carrot::parser::CarrotVault;

    use super::*;

    #[tokio::test]
    async fn try_deposit() {
        let txn_file = fs::read_to_string("tests/carrot_vault/deposit.json").unwrap();
        let json = serde_json::from_str(&txn_file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();

        let (step_utils, _) = init_test_extras_bare().await;

        let carrot_vault = CarrotVault;
        let mut dooot = vec![];
        carrot_vault
            .invoke(&txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooot.len(), 1);
    }

    #[tokio::test]
    async fn try_withdraw() {
        let txn_file = fs::read_to_string("tests/carrot_vault/withdraw.json").unwrap();
        let json = serde_json::from_str(&txn_file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();

        let (step_utils, _) = init_test_extras_bare().await;

        let carrot_vault = CarrotVault;
        let mut dooot = vec![];
        carrot_vault
            .invoke(&txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooot.len(), 1);
    }
}
