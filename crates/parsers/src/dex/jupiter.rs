use crate::state_source::{AccountProvider, StateSource};
use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime};
use num_traits::Zero;
use rust_decimal::Decimal;
use solana_sdk::account::ReadableAccount;
use solana_sdk::pubkey::Pubkey;
use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::result::Result::Ok;
use std::str::FromStr;
use step_ingestooor_sdk::utils::constants::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID};
use step_ingestooor_sdk::utils::step_account_reader::StepAccountReader;
use step_ingestooor_sdk::utils::util_fns::convert_blocktime_to_micros;
use step_ingestooor_sdk::{
    dooot::{ClobUserDooot, Dooot},
    trait_defs::transaction::TransactionLike,
    utils::step_utils::StepUtils,
};

pub struct JupiterLimitOrderV2;

const JUPITER_LO: &str = "jupoNjAxXgZ4rjzxzPMP4oxduvQsQtZzyknqvzYNrNu";
const JUPITER_LO_V2: &str = "j1o2qRpjcyUwEvwtcfhEQefh773ZgjxcVRry7LDqg5X";
const SELLING_SIGNER: &str = "j1oAbxxiDUWvoHxEDhWE7THLjEkDQW2cSHYn2vttxTF";
const ORDER_ACCOUNT_DISC: &[u8] = &[0x86, 0xad, 0xdf, 0xb9, 0x4d, 0x56, 0x1c, 0x33];
const DISC_LEN: usize = 8;

const CLICKHOUSE_SCALE: u32 = 9;

impl JupiterLimitOrderV2 {
    fn safe_decimal_from_u128(value: u128) -> Decimal {
        const MAX_SAFE_U128: u128 = 1_000_000_000_000_000;

        let safe_value = min(value, MAX_SAFE_U128);
        let safe_u64 = safe_value as u64;
        Decimal::from(safe_u64)
    }
    fn get_order_information_v2(account: &[u8]) -> Result<OrderInfo> {
        let mut reader = StepAccountReader::new(account);
        let maker = reader.read_next_pubkey()?.to_string();
        let input_mint = reader.read_next_pubkey()?.to_string();
        let output_mint = reader.read_next_pubkey()?.to_string();
        reader.move_index_to(192);
        let unique_id = reader.read_next_le_u64()?.to_string();
        let _ori_making_amount = reader.read_next_le_u64()?;
        let _ori_taking_amount = reader.read_next_le_u64()?;
        let making_amount = reader.read_next_le_u64()?;
        let taking_amount = reader.read_next_le_u64()?;

        reader.move_index_to(240);
        let expired_tag = reader.read_next_single_byte()?;
        let expiry_ts = if expired_tag == 0 {
            None
        } else {
            let raw = reader.read_next_le_i64()?;
            Some(
                DateTime::from_timestamp(raw, 0)
                    .ok_or(anyhow!(
                        "Failed to convert expiry to datetime in JupiterLimitOrderV2"
                    ))?
                    .naive_utc(),
            )
        };
        Ok(OrderInfo {
            maker,
            input_mint,
            output_mint,
            unique_id,
            making_amount,
            taking_amount,
            expiry_ts,
        })
    }
    fn get_order_information_v1(account: &[u8], order_account_pk: String) -> Result<OrderInfo> {
        let mut reader = StepAccountReader::new(account);
        let maker = reader.read_next_pubkey()?.to_string();
        let input_mint = reader.read_next_pubkey()?.to_string();
        let output_mint = reader.read_next_pubkey()?.to_string();
        reader.move_index_to(112);
        let making_amount = reader.read_next_le_u64()?;
        let taking_amount = reader.read_next_le_u64()?;

        reader.move_index_to(232);
        let expired_tag = reader.read_next_single_byte()?;
        let expiry_ts = if expired_tag == 0 {
            None
        } else {
            let raw = reader.read_next_le_i64()?;
            Some(
                DateTime::from_timestamp(raw, 0)
                    .ok_or(anyhow!(
                        "Failed to convert expiry to datetime in JupiterLimitOrderV1"
                    ))?
                    .naive_utc(),
            )
        };
        Ok(OrderInfo {
            maker,
            input_mint,
            output_mint,
            unique_id: order_account_pk,
            making_amount,
            taking_amount,
            expiry_ts,
        })
    }
    fn compute_price(info: &OrderInfo, is_buy: bool) -> Result<(Decimal, u64)> {
        let making = Self::safe_decimal_from_u128(info.making_amount as u128);
        let taking = Self::safe_decimal_from_u128(info.taking_amount as u128);

        if taking.is_zero() {
            return Ok((Decimal::zero(), 0_u64));
        }

        // atoms-per-atoms price, no decimal adjustment
        let mut price = making.checked_div(taking).unwrap_or(Decimal::zero());
        price = price.round_dp(CLICKHOUSE_SCALE);

        let unfilled_qty = if is_buy {
            info.making_amount
        } else {
            info.taking_amount
        };

        Ok((price, unfilled_qty))
    }
    fn collect_mints_and_orders<A: AccountProvider>(
        acct_provider: A,
    ) -> Result<HashMap<Pubkey, (Vec<u8>, Pubkey)>> {
        let order_disc = ORDER_ACCOUNT_DISC;
        let token_pid = Pubkey::from_str(TOKEN_PROGRAM_ID)?;
        let token22_pid = Pubkey::from_str(TOKEN_2022_PROGRAM_ID)?;

        let mut order_map = HashMap::new();
        let mut mints_set = HashSet::new();

        for prog in &[JUPITER_LO, JUPITER_LO_V2] {
            let pid = Pubkey::from_str(prog)?;
            for acct in acct_provider.program_accounts(pid, Some(order_disc)) {
                let data = acct.account.data();
                if data.len() != 372 {
                    continue;
                }

                order_map.insert(acct.pubkey, (data.to_vec(), *acct.account.owner()));

                if let Ok(info) = Self::get_order_information_v2(&data[DISC_LEN..]) {
                    if let Ok(pk) = Pubkey::from_str(&info.input_mint) {
                        mints_set.insert(pk);
                    }
                    if let Ok(pk) = Pubkey::from_str(&info.output_mint) {
                        mints_set.insert(pk);
                    }
                } else {
                    log::warn!("Skipping order with invalid data: {}", acct.pubkey);
                }
            }
        }

        let mut mint_map = HashMap::new();
        for pid in &[token_pid, token22_pid] {
            for acct in acct_provider.program_accounts(*pid, None) {
                if mints_set.contains(&acct.pubkey) {
                    let data = acct.account.data();
                    if data.len() >= 45 {
                        let decimals = data[44];
                        mint_map.insert(acct.pubkey, decimals);
                    }
                }
            }
        }
        Ok(order_map)
    }
}

#[derive(Debug)]
struct OrderInfo {
    maker: String,
    input_mint: String,
    output_mint: String,
    unique_id: String,
    making_amount: u64,
    taking_amount: u64,
    expiry_ts: Option<NaiveDateTime>,
}

#[async_trait]
impl TransactionParserTrait for JupiterLimitOrderV2 {
    fn get_name(&self) -> &'static str {
        "Jupiter Limit Order V2"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![
            TransactionParserFilter::ProgramId(JUPITER_LO.to_string()),
            TransactionParserFilter::ProgramId(JUPITER_LO_V2.to_string()),
        ]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let accounts = txn.iter_accounts().filter(|a| {
            (a.post_owner_program == Some(JUPITER_LO)
                || a.post_owner_program == Some(JUPITER_LO_V2)
                || a.post_owner_program.is_none())
                && a.datum_changed
        });

        let signer = txn.get_first_signer().expect("failed to get signer");
        let is_buy = signer != SELLING_SIGNER;

        for acct in accounts {
            let buf = if acct.post_owner_program.is_some() {
                acct.unpack_post_datum()?
                    .ok_or_else(|| anyhow!("missing post datum {}", acct.pubkey))?
            } else {
                acct.unpack_pre_datum()?
                    .ok_or_else(|| anyhow!("missing pre datum {}", acct.pubkey))?
            };

            if &buf[..DISC_LEN] != ORDER_ACCOUNT_DISC {
                continue;
            }

            let info = if buf.len() == 372 {
                Self::get_order_information_v2(&buf[DISC_LEN..])?
            } else {
                Self::get_order_information_v1(&buf[DISC_LEN..], acct.pubkey.to_string())?
                //they don't have specific order id in V1 and the only txn they have cancelOrder, so that im passing acct pk as unique_id
            };
            let prog = acct.post_owner_program.unwrap_or(JUPITER_LO_V2).to_string();

            let (price, unfilled_qty, closed) = match acct.post_owner_program {
                None => (Decimal::zero(), 0_u64, true),
                Some(_) => {
                    let (price, unfilled_qty) = Self::compute_price(&info, is_buy)?;
                    if price.is_zero() {
                        continue; // means taking is zero
                    }
                    let closed = false;
                    (price, unfilled_qty, closed)
                }
            };

            dooot.push(Dooot::ClobUser(ClobUserDooot {
                time,
                program: prog.clone(),
                market: prog,
                account: acct.pubkey.to_string(),
                owner: info.maker,
                order_id: info.unique_id,
                is_buy,
                price,
                unfilled_qty,
                base_mint: info.input_mint,
                quote_mint: info.output_mint,
                expiry_ts: info.expiry_ts,
                closed,
            }));
        }
        Ok(())
    }
}

impl StateSource for JupiterLimitOrderV2 {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;

        let dos = match Self::collect_mints_and_orders(acct_provider) {
            Ok(d) => d,
            Err(e) => {
                log::error!("Failed to collect mints and orders: {:?}", e);
                return Ok(());
            }
        };

        for (order_pubkey, (data_vec, owner)) in dos {
            if &data_vec[..DISC_LEN] != ORDER_ACCOUNT_DISC {
                continue;
            }

            let str_prog = owner.to_string();

            let info = if data_vec.len() == 372 {
                Self::get_order_information_v2(&data_vec[DISC_LEN..]).ok()
            } else {
                Self::get_order_information_v1(&data_vec[DISC_LEN..], order_pubkey.to_string()).ok()
            };

            let info = match info {
                Some(info) => info,
                None => {
                    log::warn!("Failed to parse order info for {}", order_pubkey);
                    continue;
                }
            };

            if info.taking_amount == 0 {
                continue;
            }

            let making = Self::safe_decimal_from_u128(info.making_amount as u128);
            let taking = Self::safe_decimal_from_u128(info.taking_amount as u128);
            if taking.is_zero() {
                continue;
            }

            // atoms-per-atoms
            let price = making
                .checked_div(taking)
                .unwrap_or(Decimal::zero())
                .round_dp(CLICKHOUSE_SCALE);

            let unfilled_qty = info.making_amount as u64;

            let clob_user_dooot = Dooot::ClobUser(ClobUserDooot {
                time,
                program: str_prog.clone(),
                market: str_prog,
                account: order_pubkey.to_string(),
                owner: info.maker,
                order_id: info.unique_id,
                is_buy: true,
                price,
                unfilled_qty,
                base_mint: info.input_mint,
                quote_mint: info.output_mint,
                expiry_ts: info.expiry_ts,
                closed: false,
            });
            if let Err(e) = tx.send(clob_user_dooot) {
                log::warn!("Failed to send dooot for {}: {:?}", order_pubkey, e);
            }
        }
        Ok(())
    }

    fn get_name(&self) -> &'static str {
        "Jupiter Limit Order V2"
    }
}

#[cfg(test)]
mod tests {
    use step_ingestooor_sdk::utils::test_utils::init_test_extras_bare;

    use super::*;
    use std::fs;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    use TransactionParserTrait;
    #[tokio::test]
    async fn failing() {
        let f_str = fs::read_to_string("./tests/jupiter/failing.json").unwrap();
        let f_json: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&f_str).unwrap())
                .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: JupiterLimitOrderV2 = JupiterLimitOrderV2;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 1);

        if let Dooot::ClobUser(clob) = &dooot[0] {
            assert!(clob.is_buy);
            assert_eq!(
                clob.order_id,
                "HtNEe9CzqmJqQPS1FksRkLuPiYzBUhRVfR66dznNeysN"
            );
            assert_eq!(clob.price, Decimal::from_str("0").unwrap());
            assert_eq!(clob.unfilled_qty, 0);
            assert_eq!(
                clob.base_mint,
                "DNvXa6zFwwuKaqrpxFxtLB5cXmWNT88bH752JdDtFBEV"
            );
            assert_eq!(
                clob.quote_mint,
                "So11111111111111111111111111111111111111112"
            );
        } else {
            panic!("Expected a ClobUser variant in initialize_order");
        }
    }
    #[tokio::test]
    async fn initialize_order() {
        let f_str = fs::read_to_string("./tests/jupiter/jup-lo-real.json").unwrap();
        let f_json: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&f_str).unwrap())
                .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: JupiterLimitOrderV2 = JupiterLimitOrderV2;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 1);
        if let Dooot::ClobUser(clob) = &dooot[0] {
            assert!(clob.is_buy);
            assert_eq!(clob.order_id, "9665897764384212524");
            assert_eq!(clob.price, Decimal::from_str("1.091703917").unwrap());
            assert_eq!(clob.unfilled_qty, 45883320);
            assert_eq!(
                clob.base_mint,
                "So11111111111111111111111111111111111111112"
            );
            assert_eq!(
                clob.quote_mint,
                "vSoLxydx6akxyMD9XEcPvGYNGq6Nn66oqVb3UkGkei7"
            );
        } else {
            panic!("Expected a ClobUser variant in initialize_order");
        }
    }
    #[tokio::test]
    async fn fill_order() {
        let f_str = fs::read_to_string("./tests/jupiter/jup-lo-fill-order.json").unwrap();
        let f_json: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&f_str).unwrap())
                .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: JupiterLimitOrderV2 = JupiterLimitOrderV2;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 1);
        if let Dooot::ClobUser(clob) = &dooot[0] {
            assert!(!clob.is_buy);
            assert_eq!(clob.order_id, "17145284959675887166");
            assert_eq!(clob.price, Decimal::from_str("0.146509818").unwrap());
            assert_eq!(clob.unfilled_qty, 107490169425_u64);
            assert_eq!(
                clob.base_mint,
                "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB"
            );
            assert_eq!(
                clob.quote_mint,
                "So11111111111111111111111111111111111111112"
            );
        } else {
            panic!("Expected a ClobUser variant in fill_order");
        }
    }
    #[tokio::test]
    async fn cancel_order() {
        let f_str = fs::read_to_string("./tests/jupiter/jup-lo-cancel-order.json").unwrap();
        let f_json: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&f_str).unwrap())
                .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: JupiterLimitOrderV2 = JupiterLimitOrderV2;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 1);
        if let Dooot::ClobUser(clob) = &dooot[0] {
            assert!(clob.is_buy);
            assert_eq!(clob.order_id, "16616626560520775950");
            assert_eq!(clob.price, Decimal::from(0));
            assert_eq!(clob.unfilled_qty, 0);
            assert_eq!(
                clob.base_mint,
                "71Jvq4Epe2FCJ7JFSF7jLXdNk1Wy4Bhqd9iL6bEFELvg"
            );
            assert_eq!(
                clob.quote_mint,
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
            );
        } else {
            panic!("Expected a ClobUser variant in cancel_order");
        }
    }
    #[tokio::test]
    async fn decimal_fix_test() {
        let f_str = fs::read_to_string("./tests/jupiter/jup_decimal_fix_dlq.json").unwrap();
        let f_json: DecodedTransaction =
            DecodedTransaction::from_rabbit_message_json(serde_json::from_str(&f_str).unwrap())
                .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: JupiterLimitOrderV2 = JupiterLimitOrderV2;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
    }
}
