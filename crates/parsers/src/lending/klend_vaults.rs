use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use fixed::types::U60F68;
use num_traits::ToPrimitive;
use rust_decimal::Decimal;
use solana_sdk::pubkey::Pubkey;
use step_ingestooor_idl_gens::kamino_lend_gen::kamino_lend::accounts::Reserve;
use step_ingestooor_sdk::{
    dooot::{Dooot, MintUnderlyingsGlobalDooot},
    trait_defs::{pod_types::RefFromByteRef, transaction::TransactionLike},
    utils::{step_account_reader::StepAccountReader, step_utils::StepUtils},
};

use crate::{TransactionParserFilter, TransactionParserTrait};

pub const KLEND_VAULT_PROGRAM: &str = "KvauGMspG5k6rtzrqqn7WNn3oZdyKqLKwK2XWQ8FLjd";
pub const KLEND_VAULT_STATE_DISC: &[u8] = &[0xe4, 0xc4, 0x52, 0xa5, 0x62, 0xd2, 0xeb, 0x98];

pub struct KLendVaults;

#[async_trait]
impl TransactionParserTrait for KLendVaults {
    fn get_name(&self) -> &'static str {
        "KLendVaults"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            KLEND_VAULT_PROGRAM.into(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let acct_iter = txn
            .iter_accounts()
            .filter(|a| a.datum_changed && a.post_owner_program == Some(KLEND_VAULT_PROGRAM));

        for acct in acct_iter {
            let datum = acct.unpack_post_datum()?;

            let Some(datum) = datum else {
                continue;
            };

            let disc = datum.get(..8).context("no discriminator")?;

            if disc != KLEND_VAULT_STATE_DISC {
                continue;
            }

            let vault_info = match Self::extract_vault_info(txn, &datum) {
                Ok(vault_info) => vault_info,
                Err(ExtractVaultInfoError::Deserialization(e)) => {
                    bail!("Failed to deserialize vault info: {:?}", e);
                }
                Err(ExtractVaultInfoError::MissingReserveData) => {
                    // Txn is doing something else rather than a token exchange,
                    // No need to worry about this.
                    continue;
                }
            };

            let KVaultInfo {
                vault_mint,
                shares_mint,
                shares_supply,
                aum,
            } = vault_info;

            let amt_per_parent = aum
                .ceil()
                .checked_div(Decimal::from(shares_supply))
                .unwrap_or(Decimal::ONE)
                .to_f64()
                .context("failed to convert Decimal to f64")?;

            let mu_dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                time,
                mint_pubkey: shares_mint,
                platform_program_pubkey: KLEND_VAULT_PROGRAM.into(),
                mints: vec![vault_mint],
                mints_qty_per_one_parent: vec![amt_per_parent],
                total_underlying_amounts: vec![aum],
                discriminant_id: acct.pubkey.to_string(),
            });

            dooot.push(mu_dooot);
        }

        Ok(())
    }
}

impl KLendVaults {
    fn extract_vault_info(
        txn: &impl TransactionLike,
        datum: &[u8],
    ) -> Result<KVaultInfo, ExtractVaultInfoError> {
        let mut reader = StepAccountReader::new(datum);
        reader.advance_unclamped(8 + 32 + 32 + 8);
        let vault_mint = reader
            .read_next_pubkey()
            .map_err(ExtractVaultInfoError::Deserialization)?
            .to_string();
        reader.advance_unclamped(8 + 32 + 32);
        let shares_mint = reader
            .read_next_pubkey()
            .map_err(ExtractVaultInfoError::Deserialization)?
            .to_string();
        reader.advance_unclamped(8);
        let vault_token_available = reader
            .read_next_le_u64()
            .map_err(ExtractVaultInfoError::Deserialization)?;
        let shares_supply = reader
            .read_next_le_u64()
            .map_err(ExtractVaultInfoError::Deserialization)?;
        reader.advance_unclamped(5 * 8 + 16);
        let pending_fees_sf = reader
            .read_next_le_u128()
            .map_err(ExtractVaultInfoError::Deserialization)?;
        let pending_fees = U60F68::from_bits(pending_fees_sf);
        let invested_total = Self::get_total_invested_liquidity(txn, &mut reader)?;
        let aum = Self::get_vault_aum(vault_token_available, invested_total, pending_fees);

        Ok(KVaultInfo {
            vault_mint,
            shares_mint,
            shares_supply,
            aum,
        })
    }

    fn get_vault_aum(
        vault_token_available: u64,
        invested_liquidity: Decimal,
        pending_fees: U60F68,
    ) -> Decimal {
        let fees_dec = Decimal::from(pending_fees.floor().to_num::<u64>());
        let vault_avail_dec = Decimal::from(vault_token_available);
        invested_liquidity + vault_avail_dec - fees_dec
    }

    fn extract_next_vault_allocation<'a>(
        reader: &'a mut StepAccountReader,
    ) -> Result<VaultAllocationInfo<'a>> {
        let reserve = reader.read_next_pubkey()?;
        reader.advance_unclamped(32 + 8 + 8 + 8 + 127 * 8);
        let ctoken_allocation = reader.read_next_le_u64()?;
        reader.advance_unclamped(8 + 16 + 128 * 8);

        Ok(VaultAllocationInfo {
            reserve,
            ctoken_allocation,
        })
    }

    fn get_total_invested_liquidity(
        txn: &impl TransactionLike,
        reader: &mut StepAccountReader,
    ) -> Result<Decimal, ExtractVaultInfoError> {
        let mut total_invested = Decimal::from(0);

        for _ in 0..25 {
            let vault_allocation = Self::extract_next_vault_allocation(reader)
                .map_err(ExtractVaultInfoError::Deserialization)?;
            if *vault_allocation.reserve == Pubkey::default() {
                continue;
            }

            let reserve_data = txn
                .get_account_data_after(&vault_allocation.reserve.to_string())
                .ok()
                .flatten()
                .ok_or(ExtractVaultInfoError::MissingReserveData)?;

            let res_ref = Reserve::from_bytes(&reserve_data[8..])
                .context(format!(
                    "Could not deserialize reserve {}",
                    vault_allocation.reserve
                ))
                .map_err(ExtractVaultInfoError::Deserialization)?;

            let liq_amount =
                res_ref.collateral_to_liquidity(vault_allocation.ctoken_allocation.into());

            total_invested += liq_amount;
        }

        Ok(total_invested)
    }
}

enum ExtractVaultInfoError {
    Deserialization(anyhow::Error),
    MissingReserveData,
}

struct KVaultInfo {
    vault_mint: String,
    shares_mint: String,
    shares_supply: u64,
    aum: Decimal,
}

struct VaultAllocationInfo<'a> {
    reserve: &'a Pubkey,
    ctoken_allocation: u64,
}

#[cfg(test)]
mod tests {
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;

    use super::*;

    #[tokio::test]
    async fn deposit() {
        let txn =
            DecodedTransaction::load_rabbit_message_from_file("./tests/kamino/klend_vaults.json")
                .unwrap();

        let mut dooot = Vec::new();
        let step_utils = StepUtils::default();

        let parser = KLendVaults;
        parser.invoke(&txn, &mut dooot, &step_utils).await.unwrap();

        let Dooot::MintUnderlyingsGlobal(mu_dooot) = dooot.first().unwrap() else {
            panic!("expected MintUnderlyingsGlobal");
        };

        assert_eq!(
            mu_dooot.mints[0],
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
        );
        assert_eq!(
            mu_dooot.mint_pubkey,
            "7D8C5pDFxug58L9zkwK7bCiDg4kD4AygzbcZUmf5usHS"
        );
        assert_eq!(mu_dooot.mints_qty_per_one_parent[0], 1.024544479159694);
    }
}
