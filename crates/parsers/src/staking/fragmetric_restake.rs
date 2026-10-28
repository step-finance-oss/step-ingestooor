use crate::state_source::{AccountProvider, StateSource};
use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::{Ok, Result};
use async_trait::async_trait;
use rust_decimal::Decimal;
use solana_sdk::account::ReadableAccount;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::mpsc::SyncSender;
use step_ingestooor_sdk::utils::step_account_reader::StepAccountReader;
use step_ingestooor_sdk::utils::util_fns::convert_blocktime_to_micros;
use step_ingestooor_sdk::{
    dooot::{Dooot, MintUnderlyingsGlobalDooot},
    trait_defs::transaction::TransactionLike,
    utils::step_utils::StepUtils,
};

pub struct FragmetricRestake;

const FRAGMETRIC_PROGRAM_ID: &str = "fragnAis7Bp6FTsMoa6YcH8UffhEw43Ph79qAiK3iF3";
const FUND_ACCOUNT_DISC: &[u8] = &[0x31, 0x68, 0xa8, 0xd6, 0x86, 0xb4, 0xad, 0x9a];
const WSOL: &str = "So11111111111111111111111111111111111111112";
const SOL: &str = "11111111111111111111111111111111";
const JTO: &str = "jtojtomepa8beP8AuQc6eXt5FriJwfFMwQx2v2f9mCL";
const Z_BTC: &str = "zBTCug3er3tLyffELcvDNrKkCymbPWysGcWihESYfLg";

const DISC_LEN: usize = 8;
const RECEIPT_TOKEN_VALUE_OFFSET: usize = 208;
const RECEIPT_TOKEN_MINT: usize = 112;
const ASSET_POD_SIZE: usize = 96;
const NUMERATOR_LEN: usize = 33;
const NUMERATOR_BLOCK_SIZE: usize = ASSET_POD_SIZE * NUMERATOR_LEN; // 3168
const NUM_NUMERATOR_OFFSET: usize = RECEIPT_TOKEN_VALUE_OFFSET + NUMERATOR_BLOCK_SIZE; // 3376
const DENOMINATOR_OFFSET: usize = 3384;
const SOL_AMOUNT_OFFSET: usize = 8;
const TOKEN_AMOUNT_OFFSET: usize = 16;
const TOKEN_MINT_OFFSET: usize = 24;
const WRAPPED_TOKEN_AMOUNT_OFFSET: usize = 151184 + 72;
const WRAPPED_TOKEN_MINT_OFFSET: usize = 151184;
const ONE_RECEIPT_TOKEN_AS_SOL_OFFSET: usize = 184 + 8;

impl FragmetricRestake {
    fn get_restake_informations(account: &[u8]) -> Result<RestakingInfos> {
        let mut reader = StepAccountReader::new(account);

        reader.move_index_to(RECEIPT_TOKEN_MINT);
        let receipt_token_mint = reader.read_pubkey()?.to_string();

        reader.move_index_to(ONE_RECEIPT_TOKEN_AS_SOL_OFFSET);
        let one_receipt_token_as_sol = reader.read_le_u64()?;

        reader.move_index_to(WRAPPED_TOKEN_MINT_OFFSET);
        let wrapped_token_mint = reader.read_pubkey()?.to_string();

        reader.move_index_to(WRAPPED_TOKEN_AMOUNT_OFFSET);
        let wrapped_token_supply_amount = reader.read_le_u64()?;

        let mut token_to_sol_ratios = Vec::new();

        reader.move_index_to(NUM_NUMERATOR_OFFSET);
        let num_numerator = reader.read_le_u64()?;
        reader.move_index_to(DENOMINATOR_OFFSET);
        let denominator = reader.read_le_u64()?;
        for i in 0..std::cmp::min(num_numerator as usize, NUMERATOR_LEN) {
            let pod_offset = RECEIPT_TOKEN_VALUE_OFFSET + (i * ASSET_POD_SIZE);
            reader.move_index_to(pod_offset + SOL_AMOUNT_OFFSET);
            let sol_amount = reader.read_le_u64()?;
            reader.move_index_to(pod_offset + TOKEN_AMOUNT_OFFSET);
            let mut token_amount = reader.read_le_u64()?;

            if sol_amount == 0 && token_amount == 0 {
                continue;
            }

            reader.move_index_to(pod_offset + TOKEN_MINT_OFFSET);
            let mut token_mint = reader.read_pubkey()?.to_string();

            let tokens_per_sol: f64;
            // token-to-sol ratio based on amounts
            if token_amount == 0 && sol_amount > 0 && token_mint == SOL {
                // one_receipt_token_as_sol for price when only sol_amount has value
                tokens_per_sol = 1e9_f64 / (one_receipt_token_as_sol as f64);
                token_mint = WSOL.to_string();
                token_amount = sol_amount;
            } else if token_amount > 0 && (token_mint == JTO || token_mint == Z_BTC) {
                // ratio as numerator/denominator when token_amount is non-zero
                tokens_per_sol = (token_amount as f64) / (denominator as f64)
            } else {
                continue;
            };

            if tokens_per_sol.is_nan() || tokens_per_sol.is_infinite() {
                continue;
            }

            token_to_sol_ratios.push((token_mint, tokens_per_sol, Decimal::from(token_amount)));
        }

        Ok(RestakingInfos {
            receipt_token_mint,
            wrapped_token_mint,
            wrapped_token_supply_amount,
            token_to_sol_ratios,
        })
    }
}
struct RestakingInfos {
    receipt_token_mint: String,
    wrapped_token_mint: String,
    wrapped_token_supply_amount: u64,
    token_to_sol_ratios: Vec<(String, f64, Decimal)>,
}

#[async_trait]
impl TransactionParserTrait for FragmetricRestake {
    fn get_name(&self) -> &'static str {
        "Fragmetric Restaking"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::AccountId(
            FRAGMETRIC_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let accounts = txn
            .iter_accounts()
            .filter(|a| a.post_owner_program == Some(FRAGMETRIC_PROGRAM_ID) && a.datum_changed);

        for acct in accounts {
            let data = match acct.unpack_post_datum()? {
                Some(d) => d,
                None => {
                    log::warn!(
                        "Fragmetric: Missing post datum (could be closed) for account {}",
                        acct.pubkey
                    );
                    continue;
                }
            };
            let disc = &data[..DISC_LEN];
            let buf = &data[DISC_LEN..];
            if disc == FUND_ACCOUNT_DISC {
                let info = Self::get_restake_informations(buf)?;

                for (token_mint, tokens_per_sol, token_amount) in &info.token_to_sol_ratios {
                    let token_underlying_dooot =
                        Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                            time: txn.get_block_time(),
                            mint_pubkey: info.receipt_token_mint.to_string(),
                            platform_program_pubkey: FRAGMETRIC_PROGRAM_ID.to_string(),
                            mints: vec![token_mint.to_string()],
                            mints_qty_per_one_parent: vec![*tokens_per_sol],
                            total_underlying_amounts: vec![*token_amount],
                            discriminant_id: acct.pubkey.to_string(),
                        });
                    dooot.push(token_underlying_dooot);
                }

                let wrapped_underlying_dooot =
                    Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                        time: txn.get_block_time(),
                        mint_pubkey: info.wrapped_token_mint.to_string(),
                        platform_program_pubkey: FRAGMETRIC_PROGRAM_ID.to_string(),
                        mints: vec![info.receipt_token_mint.to_string()],
                        mints_qty_per_one_parent: vec![1.0],
                        total_underlying_amounts: vec![info.wrapped_token_supply_amount.into()],
                        discriminant_id: acct.pubkey.to_string(),
                    });
                dooot.push(wrapped_underlying_dooot);
            }
        }
        Ok(())
    }
}

impl StateSource for FragmetricRestake {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        let timestamp = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let accts = acct_provider.program_accounts(
            Pubkey::from_str(FRAGMETRIC_PROGRAM_ID).expect("failed to parse program_id to pubkey"),
            Some(FUND_ACCOUNT_DISC),
        );
        for acct in accts {
            let data = acct.account.data();
            if data.starts_with(FUND_ACCOUNT_DISC) {
                let buf = &data[DISC_LEN..];
                let info = Self::get_restake_informations(buf)?;

                for (token_mint, tokens_per_sol, token_amount) in &info.token_to_sol_ratios {
                    let token_underlying_dooot =
                        Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                            time: timestamp,
                            mint_pubkey: info.receipt_token_mint.to_string(),
                            platform_program_pubkey: FRAGMETRIC_PROGRAM_ID.to_string(),
                            mints: vec![token_mint.to_string()],
                            mints_qty_per_one_parent: vec![*tokens_per_sol],
                            total_underlying_amounts: vec![*token_amount],
                            discriminant_id: acct.pubkey.to_string(),
                        });
                    tx.send(token_underlying_dooot)?;
                }

                let wrapped_underlying_dooot =
                    Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                        time: timestamp,
                        mint_pubkey: info.wrapped_token_mint.to_string(),
                        platform_program_pubkey: FRAGMETRIC_PROGRAM_ID.to_string(),
                        mints: vec![info.receipt_token_mint.to_string()],
                        mints_qty_per_one_parent: vec![1.0],
                        total_underlying_amounts: vec![info.wrapped_token_supply_amount.into()],
                        discriminant_id: acct.pubkey.to_string(),
                    });
                tx.send(wrapped_underlying_dooot)?;
            }
        }
        Ok(())
    }
    fn get_name(&self) -> &'static str {
        "Fragmetric Restaking"
    }
}
#[cfg(test)]
mod tests {
    use step_ingestooor_sdk::utils::test_utils::init_test_extras_bare;

    use super::*;
    use rust_decimal::Decimal;
    use std::fs;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    use TransactionParserTrait;

    #[tokio::test]
    async fn invoke() {
        let f_str = fs::read_to_string("./tests/fragmetric/txn_frag_w5nFogr4stQuytxgdfGmQExsWEHN7FKb9y5pJ3bGTZgfjdWDDTdhq7dSY9y3TP9ykRiAT1119ua1V87G8nuGsPE.json").unwrap();
        let f_json: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&f_str).unwrap())
                .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: FragmetricRestake = FragmetricRestake;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        println!("{:#?}", dooot);
        assert_eq!(dooot.len(), 2);

        if let Dooot::MintUnderlyingsGlobal(underlying) = &dooot[0] {
            let expected_total_underlying_amount = vec![Decimal::from(110005168931u64)];
            let expected_mints_qty_per_one_parent = vec![0.9507881929057093];
            assert_eq!(
                underlying.mint_pubkey,
                "FRAGSEthVFL7fdqM8hxfxkfCZzUvmg21cqPJVvC1qdbo"
            );
            assert_eq!(
                underlying.total_underlying_amounts,
                expected_total_underlying_amount
            );
            assert_eq!(
                underlying.mints_qty_per_one_parent,
                expected_mints_qty_per_one_parent
            );
            assert_eq!(
                underlying.mints,
                vec!["So11111111111111111111111111111111111111112"]
            );
        }
        if let Dooot::MintUnderlyingsGlobal(underlying) = &dooot[1] {
            let expected_total_underlying_amount = vec![Decimal::from(740411898395385u64)];
            let expected_mints_qty_per_one_parent = vec![1.0];
            assert_eq!(
                underlying.mint_pubkey,
                "WFRGSWjaz8tbAxsJitmbfRuFV2mSNwy7BMWcCwaA28U"
            );
            assert_eq!(
                underlying.total_underlying_amounts,
                expected_total_underlying_amount
            );
            assert_eq!(
                underlying.mints_qty_per_one_parent,
                expected_mints_qty_per_one_parent
            );
            assert_eq!(
                underlying.mints,
                vec!["FRAGSEthVFL7fdqM8hxfxkfCZzUvmg21cqPJVvC1qdbo"]
            );
        }
    }
}
