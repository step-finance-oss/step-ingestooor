use crate::{TransactionParserFilter, TransactionParserTrait};
use async_trait::async_trait;
use bytemuck::{Pod, Zeroable};
use itertools::Itertools;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use rust_decimal::Decimal;
use solana_sdk::pubkey::Pubkey;
use std::collections::{HashMap, HashSet};
use step_ingestooor_sdk::dooot::ClobUserDooot;
use step_ingestooor_sdk::{
    dooot::{ClobFillEventDooot, Dooot},
    trait_defs::transaction::TransactionLike,
    utils::step_utils::StepUtils,
};
//https://github.com/CKS-Systems/manifest/blob/918d0aa3da6df95f300be987986b3f4542138417/programs/manifest/src/logs.rs#L174
const BASE_OFFSET: usize = 16;
const QUOTE_OFFSET: usize = 48;
const PK_END: usize = 80;
const FILL_LOG_DISCRIMINANT: &[u8] = &[0x3A, 0xE6, 0xF2, 0x03, 0x4B, 0x71, 0x04, 0xA9];
const PLACE_ORDER_LOG_DISCRIMINANT: &[u8] = &[0x9D, 0x76, 0xF7, 0xD5, 0x2F, 0x13, 0xA4, 0x78];
const PLACE_ORDER_LOG_V2_DISCRIMINANT: &[u8] = &[0xBD, 0x61, 0x9F, 0xEB, 0x88, 0x05, 0x01, 0x8D];
const CANCEL_ORDER_LOG_DISCRIMINANT: &[u8] = &[0x16, 0x41, 0x47, 0x21, 0xF4, 0xEB, 0xFF, 0xD7];

//https://github.com/CKS-Systems/manifest/blob/918d0aa3da6df95f300be987986b3f4542138417/programs/manifest/src/state/constants.rs#22
pub const MARKET_FIXED_DISCRIMINANT: u64 = 4859840929024028656;

//https://github.com/CKS-Systems/manifest/blob/918d0aa3da6df95f300be987986b3f4542138417/programs/manifest/src/logs.rs#L67
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct FillLog {
    pub market: Pubkey,
    pub maker: Pubkey,
    pub taker: Pubkey,
    //https://github.com/CKS-Systems/manifest/blob/918d0aa3da6df95f300be987986b3f4542138417/programs/manifest/src/logs.rs#L72
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    //https://github.com/CKS-Systems/manifest/blob/918d0aa3da6df95f300be987986b3f4542138417/programs/manifest/src/quantities.rs#L210
    pub price: [u64; 2],
    //https://github.com/CKS-Systems/manifest/blob/918d0aa3da6df95f300be987986b3f4542138417/programs/manifest/src/quantities.rs#L193
    pub base_atoms: u64,
    //https://github.com/CKS-Systems/manifest/blob/918d0aa3da6df95f300be987986b3f4542138417/programs/manifest/src/quantities.rs#L184
    pub quote_atoms: u64,
    pub maker_sequence_number: u64,
    pub taker_sequence_number: u64,
    //https://github.com/CKS-Systems/manifest/blob/918d0aa3da6df95f300be987986b3f4542138417/programs/manifest/src/logs.rs#L79
    pub taker_is_buy: u8,
    pub is_maker_global: u8,
    pub _padding: [u8; 14],
}
// https://github.com/CKS-Systems/manifest/blob/f42c5e8393cf41d3e1ed47d67ea0fb120fa4b108/lib/src/utils.rs#L25
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(transparent)]
pub struct PodBool(pub u8);
impl PodBool {
    pub const fn from_bool(b: bool) -> Self {
        Self(if b { 1 } else { 0 })
    }
}
impl From<bool> for PodBool {
    fn from(b: bool) -> Self {
        Self::from_bool(b)
    }
}
// https://github.com/CKS-Systems/manifest/blob/f42c5e8393cf41d3e1ed47d67ea0fb120fa4b108/programs/manifest/src/state/resting_order.rs#L29
unsafe impl bytemuck::Zeroable for OrderType {}
unsafe impl Pod for OrderType {}
#[derive(Debug, PartialEq, Clone, Copy, IntoPrimitive, TryFromPrimitive, Default)]
#[repr(u8)]
pub enum OrderType {
    // Normal limit order.
    #[default]
    Limit = 0,
    // Does not rest. Take only.
    ImmediateOrCancel = 1,
    // Fails if would cross the orderbook.
    PostOnly = 2,
    // Global orders are post only but use funds from the global account.
    Global = 3,
    // Reverse orders behave like an AMM. When filled, they place an order on
    // the other side of the book with a small fee (spread).
    Reverse = 4,
}
// https://github.com/CKS-Systems/manifest/blob/f42c5e8393cf41d3e1ed47d67ea0fb120fa4b108/programs/manifest/src/logs.rs#L92
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct PlaceOrderLog {
    pub market: Pubkey,
    pub trader: Pubkey,
    pub price: QuoteAtomsPerBaseAtom,
    pub base_atoms: BaseAtoms,
    pub order_sequence_number: u64,
    pub order_index: u32,
    pub last_valid_slot: u32,
    pub order_type: OrderType,
    pub is_bid: PodBool,
    pub _padding: [u8; 6],
}
// https://github.com/CKS-Systems/manifest/blob/main/programs/manifest/src/quantities.rs#L229
#[derive(Clone, Copy, Default, Zeroable, Pod, Debug)]
#[repr(C)]
pub struct QuoteAtomsPerBaseAtom {
    pub(crate) inner: [u64; 2],
}
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct PlaceOrderLogV2 {
    pub market: Pubkey,
    pub trader: Pubkey,
    pub payer: Pubkey,
    pub price: QuoteAtomsPerBaseAtom,
    pub base_atoms: BaseAtoms,
    pub order_sequence_number: u64,
    pub order_index: u32,
    pub last_valid_slot: u32,
    pub order_type: OrderType,
    pub is_bid: PodBool,
    pub _padding: [u8; 6],
}
// https://github.com/CKS-Systems/manifest/blob/f42c5e8393cf41d3e1ed47d67ea0fb120fa4b108/programs/manifest/src/quantities.rs#L526
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
#[repr(transparent)]
pub struct BaseAtoms {
    inner: u64,
}
// https://github.com/CKS-Systems/manifest/blob/f42c5e8393cf41d3e1ed47d67ea0fb120fa4b108/programs/manifest/src/quantities.rs#L547
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
#[repr(transparent)]
pub struct QuoteAtoms {
    inner: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct CancelOrderLog {
    pub market: Pubkey,
    pub trader: Pubkey,
    pub order_sequence_number: u64,
}
const MANIFEST_PROGRAM: &str = "MNFSTqtC93rEfYHB6hF82sKdZpUDFWkViLByLd1k1Ms";
pub struct Manifest;

#[async_trait]
impl TransactionParserTrait for Manifest {
    fn get_name(&self) -> &'static str {
        "Manifest"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            MANIFEST_PROGRAM.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<step_ingestooor_sdk::dooot::Dooot>,
        _step_utils: &StepUtils,
    ) -> anyhow::Result<()> {
        //can only fill if ix disc is 4 (swap) or 6 (batch update)
        //https://github.com/CKS-Systems/manifest/blob/918d0aa3da6df95f300be987986b3f4542138417/programs/manifest/src/program/instruction.rs#L8
        let any_ixs_that_fill = txn
            .get_non_parsed_program_ixs(&[MANIFEST_PROGRAM.into()])
            .any(|a| {
                if let Ok(data) = bs58::decode(&a.data).into_vec() {
                    data[0] == 4 || data[0] == 6
                } else {
                    log::error!("Could not decode ix data for manifest");
                    false
                }
            });

        if !any_ixs_that_fill {
            return Ok(());
        }

        //there was a batch update or swap
        //get all the logs
        let logs = txn.get_program_events_from_logs(MANIFEST_PROGRAM);
        //grab fills from the logs
        let fill_orders = logs
            .iter()
            .filter_map(|log| {
                if log.len() < 8 {
                    None
                } else if log[0..8] == *FILL_LOG_DISCRIMINANT {
                    let fill_log = bytemuck::try_from_bytes::<FillLog>(&log[8..]);
                    Some(fill_log)
                } else {
                    None
                }
            })
            .collect::<Result<Vec<&FillLog>, _>>()
            .map_err(|e| anyhow::anyhow!("Could not parse fill log: {}", e))?;
        let placed_orders = logs
            .iter()
            .filter_map(|log| {
                if log.len() < 8 {
                    None
                } else if log[..8] == *PLACE_ORDER_LOG_DISCRIMINANT {
                    let placed_order = bytemuck::try_from_bytes::<PlaceOrderLog>(&log[8..]);
                    Some(placed_order)
                } else {
                    None
                }
            })
            .collect::<Result<Vec<&PlaceOrderLog>, _>>()
            .map_err(|e| anyhow::anyhow!("Could not parse place order log: {}", e))?;

        let placed_orders_v2 = logs
            .iter()
            .filter_map(|log| {
                if log.len() < 8 {
                    None
                } else if log[..8] == *PLACE_ORDER_LOG_V2_DISCRIMINANT {
                    let placed_order = bytemuck::try_from_bytes::<PlaceOrderLogV2>(&log[8..]);
                    Some(placed_order)
                } else {
                    None
                }
            })
            .collect::<Result<Vec<&PlaceOrderLogV2>, _>>()
            .map_err(|e| anyhow::anyhow!("Could not parse place order log v2: {}", e))?;

        let cancelled_orders = logs
            .iter()
            .filter_map(|log| {
                if log.len() < 8 {
                    None
                } else if log[0..8] == *CANCEL_ORDER_LOG_DISCRIMINANT {
                    let cancel_log = bytemuck::try_from_bytes::<CancelOrderLog>(&log[8..]);
                    Some(cancel_log)
                } else {
                    None
                }
            })
            .collect::<Result<Vec<&CancelOrderLog>, _>>()
            .map_err(|e| anyhow::anyhow!("Could not parse cancelled order log: {}", e))?;

        if fill_orders.is_empty()
            && placed_orders.is_empty()
            && placed_orders_v2.is_empty()
            && cancelled_orders.is_empty()
        {
            return Ok(());
        }
        let mut market_set: HashSet<String> = HashSet::new();
        for f in &fill_orders {
            market_set.insert(f.market.to_string());
        }
        for p in &placed_orders {
            market_set.insert(p.market.to_string());
        }
        for p in &placed_orders_v2 {
            market_set.insert(p.market.to_string());
        }
        for c in &cancelled_orders {
            market_set.insert(c.market.to_string());
        }
        let mut market_mints: HashMap<String, (String, String)> = HashMap::new();

        for market_pubkey in &market_set {
            let market_data: Vec<u8> = txn
                .get_account_data_after(market_pubkey)?
                .ok_or_else(|| anyhow::anyhow!("Market {} post data empty", market_pubkey))?;

            if market_data.len() < PK_END {
                log::error!(
                    "Market {} data too short ({} bytes)",
                    market_pubkey,
                    market_data.len()
                );
                continue;
            }

            let base_mint = Pubkey::new_from_array(
                market_data[BASE_OFFSET..BASE_OFFSET + 32]
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("base mint not a pubkey"))?,
            )
            .to_string();

            let quote_mint = Pubkey::new_from_array(
                market_data[QUOTE_OFFSET..QUOTE_OFFSET + 32]
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("quote mint not a pubkey"))?,
            )
            .to_string();

            market_mints.insert(market_pubkey.clone(), (base_mint, quote_mint));
        }

        for (market_pk, group) in fill_orders
            .into_iter()
            .into_group_map_by(|l| l.market.to_string())
        {
            let Some((base_mint, quote_mint)) = market_mints.get(&market_pk) else {
                log::warn!("No mint cache for market {}; skipping fills", market_pk);
                continue;
            };
            for fill in group {
                let price = Decimal::from(u64_slice_to_u128(fill.price));

                dooot.push(Dooot::ClobFillEvent(ClobFillEventDooot {
                    time: txn.get_block_time(),
                    market_pubkey: market_pk.to_string().clone(),
                    base_mint_pubkey: base_mint.clone(),
                    quote_mint_pubkey: quote_mint.clone(),
                    maker_pubkey: fill.maker.to_string(),
                    taker_pubkey: fill.taker.to_string(),
                    referral_fee: 0.0,
                    maker_fee: 0.0,
                    taker_fee: 0.0,
                    program_id: MANIFEST_PROGRAM.to_string(),
                    buy_side: fill.taker_is_buy == 1,
                    referrer_pubkey: None,
                    base_qty: fill.base_atoms,
                    price,
                }));
            }
        }
        for (market_pk, group) in placed_orders
            .clone()
            .into_iter()
            .into_group_map_by(|l| l.market.to_string())
        {
            let Some((base_mint, quote_mint)) = market_mints.get(&market_pk) else {
                log::warn!(
                    "No mint cache for market {}; skipping placed_orders",
                    market_pk
                );
                continue;
            };
            for placed_order in group {
                let clob_dooot = ClobUserDooot {
                    time: txn.get_block_time(),
                    program: MANIFEST_PROGRAM.to_string(),
                    market: market_pk.to_string().clone(),
                    account: "".to_string(), // they don't have a specific order acc so left as empty string
                    owner: placed_order.trader.to_string(),
                    order_id: placed_order.order_sequence_number.to_string(),
                    is_buy: placed_order.is_bid.0 != 0,
                    price: Decimal::from(placed_order.price.inner[0]),
                    unfilled_qty: placed_order.base_atoms.inner,
                    base_mint: base_mint.clone(),
                    quote_mint: quote_mint.clone(),
                    expiry_ts: None,
                    closed: false,
                };
                dooot.push(Dooot::ClobUser(clob_dooot));
            }
        }
        for (market_pk, group) in placed_orders_v2
            .clone()
            .into_iter()
            .into_group_map_by(|l| l.market.to_string())
        {
            let Some((base_mint, quote_mint)) = market_mints.get(&market_pk) else {
                log::warn!(
                    "No mint cache for market {}; skipping placed_orders_v2",
                    market_pk
                );
                continue;
            };
            for placed_order in group {
                let clob_dooot = ClobUserDooot {
                    time: txn.get_block_time(),
                    program: MANIFEST_PROGRAM.to_string(),
                    market: market_pk.to_string().clone(),
                    account: "".to_string(), // they don't have a specific order acc on the program logs, where we got the data so left as empty string
                    owner: placed_order.trader.to_string(),
                    order_id: placed_order.order_sequence_number.to_string(),
                    is_buy: placed_order.is_bid.0 != 0,
                    price: Decimal::from(placed_order.price.inner[0]),
                    unfilled_qty: placed_order.base_atoms.inner,
                    base_mint: base_mint.clone(),
                    quote_mint: quote_mint.clone(),
                    expiry_ts: None,
                    closed: false,
                };
                dooot.push(Dooot::ClobUser(clob_dooot));
            }
        }
        for (market_pk, group) in cancelled_orders
            .clone()
            .into_iter()
            .into_group_map_by(|l| l.market.to_string())
        {
            let Some((base_mint, quote_mint)) = market_mints.get(&market_pk) else {
                log::warn!(
                    "No mint cache for market {}; skipping cancelled_orders",
                    market_pk
                );
                continue;
            };
            for cancelled_order in group {
                let clob_dooot = ClobUserDooot {
                    time: txn.get_block_time(),
                    program: MANIFEST_PROGRAM.to_string(),
                    market: market_pk.to_string().clone(),
                    account: "".to_string(),
                    owner: cancelled_order.trader.to_string(),
                    order_id: cancelled_order.order_sequence_number.to_string(),
                    is_buy: false,
                    price: Decimal::ZERO,
                    // For canceled orders, unfilled_qty should be 0 since nothing is left unfilled
                    // The actual filled amount is stored in filled_qty for reference
                    unfilled_qty: 0,
                    base_mint: base_mint.clone(),
                    quote_mint: quote_mint.clone(),
                    expiry_ts: None,
                    closed: true,
                };
                dooot.push(Dooot::ClobUser(clob_dooot));
            }
        }

        Ok(())
    }
}

//https://github.com/CKS-Systems/manifest/blob/918d0aa3da6df95f300be987986b3f4542138417/programs/manifest/src/quantities.rs#L224
#[inline]
fn u64_slice_to_u128(a: [u64; 2]) -> u128 {
    unsafe {
        let ptr: *const [u64; 2] = &a;
        *ptr.cast::<u128>()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, rc::Rc};

    use super::*;
    use serde_json::Value;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;

    #[tokio::test]
    async fn market_info() {
        let message_json = fs::read_to_string("./tests/manifest/market_info.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooots = vec![];
        let parser = Manifest;
        parser
            .invoke(&decoded_txn, &mut dooots, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooots.len(), 16);
    }
    #[tokio::test]
    async fn same_decimals() {
        let message_json = fs::read_to_string("./tests/manifest/same-decimals.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooots = vec![];
        let parser = Manifest;
        parser
            .invoke(&decoded_txn, &mut dooots, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooots.len(), 2);
        let dooot = dooots.first().unwrap();
        match dooot {
            Dooot::ClobFillEvent(fill) => {
                assert_eq!(
                    fill.program_id,
                    "MNFSTqtC93rEfYHB6hF82sKdZpUDFWkViLByLd1k1Ms"
                );
                assert_eq!(
                    fill.market_pubkey,
                    "FbvzQZheEPt7ppvLUxMYcyqqsyaFccLawpEk1NstidvS"
                );
                assert_eq!(
                    fill.base_mint_pubkey,
                    "6cvrZWgEUkr82yKAmxp5cQu7wgYYBPULf16EUBp4pump"
                );
                assert_eq!(
                    fill.quote_mint_pubkey,
                    "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
                );
                assert_eq!(
                    fill.taker_pubkey,
                    "DanayVvgHg3dFjhJgjNECxU589vGjzmwRBBZ7nCMVzUg"
                );
                assert_eq!(
                    fill.maker_pubkey,
                    "CDY3cxDRUrcJp8DNhPS8X6CR3FGDjrErYv1PcgsEeNMV"
                );
                assert!(fill.buy_side);
                assert_eq!(fill.base_qty, 3000000);
                assert_eq!(fill.price, Decimal::from(10248081000000000u128));
                assert_eq!(fill.taker_fee, 0.0);
                assert_eq!(fill.maker_fee, 0.0);
                assert_eq!(fill.referrer_pubkey, None);
                assert_eq!(fill.referral_fee, 0.0);
            }
            _ => panic!("wrong dooot type"),
        }
    }

    #[tokio::test]
    async fn diff_decimals() {
        let message_json = fs::read_to_string("./tests/manifest/diff-decimals.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooots = vec![];
        let parser = Manifest;
        parser
            .invoke(&decoded_txn, &mut dooots, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooots.len(), 2);
        let dooot = dooots.first().unwrap();
        match dooot {
            Dooot::ClobFillEvent(fill) => {
                assert_eq!(
                    fill.program_id,
                    "MNFSTqtC93rEfYHB6hF82sKdZpUDFWkViLByLd1k1Ms"
                );
                assert_eq!(
                    fill.market_pubkey,
                    "ENhU8LsaR7vDD2G1CsWcsuSGNrih9Cv5WZEk7q9kPapQ"
                );
                assert_eq!(
                    fill.base_mint_pubkey,
                    "So11111111111111111111111111111111111111112"
                );
                assert_eq!(
                    fill.quote_mint_pubkey,
                    "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
                );
                assert_eq!(
                    fill.taker_pubkey,
                    "DanayVvgHg3dFjhJgjNECxU589vGjzmwRBBZ7nCMVzUg"
                );
                assert_eq!(
                    fill.maker_pubkey,
                    "CVxGkSTTkmKCf1RvSw1S72HYe6P8SnWrdiSufoT65gBz"
                );
                assert!(fill.buy_side);
                assert_eq!(fill.base_qty, 4472347);
                assert_eq!(fill.price, Decimal::from(223596220000000000u128));
                assert_eq!(fill.taker_fee, 0.0);
                assert_eq!(fill.maker_fee, 0.0);
                assert_eq!(fill.referrer_pubkey, None);
                assert_eq!(fill.referral_fee, 0.0);
            }
            _ => panic!("wrong dooot type"),
        }
    }
}
