use anyhow::Result;
use async_trait::async_trait;
use borsh::{BorshDeserialize, BorshSerialize};

use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use step_ingestooor_sdk::{trait_defs::transaction::TransactionLike, utils::step_utils::StepUtils};

pub struct Openbook;

pub const OPENBOOK_PROGRAM: &str = "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX";
#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Serialize, Deserialize)]
pub struct OBMarket {
    what: [u8; 5],
    pub account_flags: u64,
    pub own_address: Pubkey,
    pub vault_signer_nonce: u64,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub base_vault: Pubkey,
    pub base_deposits_total: u64,
    pub base_fees_accrued: u64,
    pub quote_vault: Pubkey,
    pub quote_deposits_total: u64,
    pub quote_fees_accrued: u64,
    pub quote_dust_threshold: u64,
    pub request_queue: Pubkey,
    pub event_queue: Pubkey,
    pub bids: Pubkey,
    pub asks: Pubkey,
    pub base_lot_size: u64,
    pub quote_lot_size: u64,
    pub fee_rate_bps: u64,
}

// const MARKET_LEN: usize = 5 + 8 * 10 + 32 * 9;

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Deserialize, Serialize)]
pub struct OBFillEvent {
    discriminator: [u8; 8],
    pub market: Pubkey,
    pub open_orders: Pubkey,
    pub open_orders_owner: Pubkey,
    pub bid: bool,
    pub maker: bool,
    pub native_quantity_paid: u64,
    pub native_quantity_received: u64,
    pub native_fee_or_rebate: u64,
    pub order_id: u128,
    pub owner_slot: u8,
    pub fee_tier: u8,
    pub client_order_id: Option<u64>,
    pub referrer_rebate: Option<u64>,
}

// type OBFillEventPairs = Vec<(OBFillEvent, OBFillEvent)>;

// const FILL_EVENT_EXPIRATION_SECS: Option<u64> = Some(60);

#[async_trait]
impl crate::TransactionParserTrait for Openbook {
    fn get_name(&self) -> &'static str {
        "Openbook v1"
    }
    fn get_filters(&self) -> Vec<crate::TransactionParserFilter> {
        vec![crate::TransactionParserFilter::ProgramId(
            OPENBOOK_PROGRAM.to_string(),
        )]
    }
    async fn invoke(
        &self,
        _decoded_transaction: &impl TransactionLike,
        _dooot: &mut Vec<step_ingestooor_sdk::dooot::Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        // let block_time = decoded_transaction.get_block_time();
        // let events = decoded_transaction
        //     .get_program_events_from_logs(OPENBOOK_PROGRAM)
        //     .iter()
        //     .filter_map(|e| OBFillEvent::try_from_slice(e).ok())
        //     .collect::<Vec<OBFillEvent>>();
        // let pairs = Openbook::get_buy_sell_pairs(events, block_time).await?;

        // for (buy, sell) in pairs {
        //     let taker = (if buy.maker {
        //         sell.open_orders_owner
        //     } else {
        //         buy.open_orders_owner
        //     })
        //     .to_string();
        //     let maker = (if buy.maker {
        //         buy.open_orders_owner
        //     } else {
        //         sell.open_orders_owner
        //     })
        //     .to_string();

        //     let market_key = buy.market.to_string();
        //     let market_cache_key = format!("openbook:market:{}", market_key);
        //     let market: OBMarket = {
        //         let market_data = decoded_transaction
        //             .get_account_data_after(&market_key)?
        //             .ok_or_else(|| anyhow!("Market data not found: {}", market_key))?;
        //         let market_slice = &market_data[..];
        //         let market = OBMarket::try_from_slice(&market_slice[..MARKET_LEN])?;
        //         market
        //     };

        //     let base_mint = market.base_mint;
        //     let quote_mint = market.quote_mint;
        //     let quote_decimals = decoded_transaction.get_mint_decimals(&quote_mint.to_string());

        //     let quote_divisor = 10u64.pow(quote_decimals as u32) as f64;

        //     // Keep the raw native quantity as u64 for base_qty
        //     let size = buy.native_quantity_received;
        //     let native_paid_before_fees = if buy.maker {
        //         buy.native_quantity_paid + buy.native_fee_or_rebate
        //     } else {
        //         buy.native_quantity_paid - buy.native_fee_or_rebate
        //     };
        //     // atoms-per-atoms price, no decimal adjustment
        //     let price = Decimal::from(native_paid_before_fees);
        //     let taker_fee_native = if buy.maker {
        //         sell.native_fee_or_rebate
        //     } else {
        //         buy.native_fee_or_rebate
        //     };
        //     let taker_fee = taker_fee_native as f64 / quote_divisor;
        //     let maker_fee_native = -(if buy.maker {
        //         buy.native_fee_or_rebate as i128
        //     } else {
        //         sell.native_fee_or_rebate as i128
        //     });
        //     let maker_fee = maker_fee_native as f64 / quote_divisor;
        //     let referrer_fee_native = if buy.maker {
        //         buy.referrer_rebate.unwrap_or(0)
        //     } else {
        //         sell.referrer_rebate.unwrap_or(0)
        //     };
        //     let referrer_fee = referrer_fee_native as f64 / quote_divisor;

        //     dooot.push(Dooot::ClobFillEvent(ClobFillEventDooot {
        //         program_id: OPENBOOK_PROGRAM.to_string(),
        //         market_pubkey: market_key,
        //         base_mint_pubkey: base_mint.to_string(),
        //         quote_mint_pubkey: quote_mint.to_string(),
        //         taker_pubkey: taker,
        //         maker_pubkey: maker,
        //         buy_side: buy.maker,
        //         base_qty: size,
        //         price,
        //         taker_fee,
        //         maker_fee,
        //         referrer_pubkey: None,
        //         referral_fee: referrer_fee,
        //         time: decoded_transaction.get_block_time(),
        //     }))
        // }

        Ok(())
    }
}

impl Openbook {
    // // Not taking a MutexGuard here because it is possible to not even need the cache
    // pub async fn get_buy_sell_pairs(
    //     events: Vec<OBFillEvent>,

    //     block_time: NaiveDateTime,
    // ) -> Result<OBFillEventPairs> {
    //     let mut events = events;
    //     let mut curr_len = events.len();
    //     let mut pairs = vec![];
    //     loop {
    //         let mut idxs_to_remove = vec![];
    //         for (idx, this_event) in events.iter().enumerate() {
    //             if idxs_to_remove.contains(&idx) {
    //                 // Already found the other side of this trade
    //                 continue;
    //             }

    //             let mut other_event: Option<OBFillEvent> = None;

    //             let this_idx = idx;
    //             let other_side_event_info =
    //                 events.iter().enumerate().skip(idx + 1).find(|(_, e)| {
    //                     let is_different_side = e.bid != this_event.bid;
    //                     let is_different_maker = e.maker != this_event.maker;
    //                     let is_same_market = e.market == this_event.market;
    //                     let matching_amounts = if e.bid {
    //                         e.native_quantity_received == this_event.native_quantity_paid
    //                     } else {
    //                         e.native_quantity_paid == this_event.native_quantity_received
    //                     };
    //                     is_different_side
    //                         && is_different_maker
    //                         && is_same_market
    //                         && matching_amounts
    //                 });

    //             if let Some((other_side_idx, event)) = other_side_event_info {
    //                 // Found the other side of the trade within this txn
    //                 other_event = Some(event.clone());
    //                 idxs_to_remove.push(other_side_idx);
    //             } else {
    //                 let market = this_event.market.to_string();
    //                 let maker_taker = if this_event.maker { "taker" } else { "maker" };
    //                 let side = if this_event.bid { "ask" } else { "bid" };
    //                 let qty = if this_event.bid {
    //                     this_event.native_quantity_received
    //                 } else {
    //                     this_event.native_quantity_paid
    //                 };
    //                 let other_side_cache_key =
    //                     format!("openbook:fill:{market}:{maker_taker}:{side}:{qty}");
    //                 let wrapped: Option<OBFillEvent> = match ts_cache
    //                     .get(&other_side_cache_key, Some(block_time), Some(30))
    //                     .await
    //                 {
    //                     Ok(wrapped) => wrapped,
    //                     Err(CacheError::NoValueFound(_, _)) => None,
    //                     Err(e) => return Err(e.into()),
    //                 };
    //                 if let Some(wrapped) = wrapped {
    //                     other_event = Some(wrapped);
    //                     // delete from cache
    //                     ts_cache.del(&other_side_cache_key).await?;
    //                 } else {
    //                     // New cache key, but flipped to our side
    //                     let maker_taker = if this_event.maker { "maker" } else { "taker" };
    //                     let side = if this_event.bid { "bid" } else { "ask" };
    //                     // Same qty
    //                     let this_side_cache_key =
    //                         format!("openbook:fill:{market}:{maker_taker}:{side}:{qty}");
    //                     // store in cache
    //                     ts_cache
    //                         .set(
    //                             &this_side_cache_key,
    //                             this_event.clone(),
    //                             block_time,
    //                             FILL_EVENT_EXPIRATION_SECS,
    //                         )
    //                         .await?;
    //                 }
    //             }

    //             if let Some(other_event) = other_event {
    //                 pairs.push((this_event.clone(), other_event));
    //                 idxs_to_remove.push(this_idx);
    //             }
    //         }

    //         // Create new vec of events with all idxs in the list removed
    //         let mut new_events = vec![];
    //         for (idx, event) in events.iter().enumerate() {
    //             if !idxs_to_remove.contains(&idx) {
    //                 new_events.push(event.clone());
    //             }
    //         }

    //         events = new_events;
    //         if events.len() == curr_len {
    //             // Did not reduce in size, so no new pairs found, we are done
    //             return Ok(pairs);
    //         }

    //         curr_len = events.len();
    //     }
    // }
}

// #[cfg(test)]
// mod tests {
//     use step_ingestooor_sdk::structs::simple_mem_cache::SimpleMemCache;

//     use super::*;

//     fn make_two_fills() -> (OBFillEvent, OBFillEvent) {
//         let market_id = Pubkey::new_unique();
//         let alice = Pubkey::new_unique();
//         let bob = Pubkey::new_unique();

//         let event_1 = OBFillEvent {
//             bid: true,
//             maker: true,
//             open_orders_owner: alice,
//             market: market_id,
//             native_quantity_paid: 69,
//             native_quantity_received: 420,
//             // Doesn't matter
//             client_order_id: None,
//             discriminator: [0; 8],
//             fee_tier: 1,
//             native_fee_or_rebate: 0,
//             open_orders: Pubkey::new_unique(),
//             order_id: 0,
//             owner_slot: 0,
//             referrer_rebate: None,
//         };

//         let event_2 = OBFillEvent {
//             bid: false,
//             maker: false,
//             open_orders_owner: bob,
//             market: market_id,
//             native_quantity_paid: 420,
//             native_quantity_received: 69,
//             // Doesn't matter
//             client_order_id: None,
//             discriminator: [0; 8],
//             fee_tier: 1,
//             native_fee_or_rebate: 0,
//             open_orders: Pubkey::new_unique(),
//             order_id: 0,
//             owner_slot: 0,
//             referrer_rebate: None,
//         };

//         (event_1, event_2)
//     }

//     async fn get_cache() -> impl TimeSeriesCacheLike {
//         SimpleMemCache::new(None).await
//     }

//     // /// Finds fill pairs within the same transaction
//     // #[tokio::test]
//     // async fn same_vec_pairs() {
//     //     let cache = get_cache().await;
//     //     let (event_1, event_2) = make_two_fills();
//     //     let events = vec![event_1.clone(), event_2.clone()];
//     //     let time = NaiveDateTime::and_utc(&Utc::now().naive_utc()).naive_utc();
//     //     let pairs = Openbook::get_buy_sell_pairs(events, &cache, time)
//     //         .await
//     //         .unwrap();
//     //     assert_eq!(pairs.len(), 1);
//     // }

//     // /// Finds fill pairs within a different transaction
//     // #[tokio::test]
//     // async fn different_txs() {
//     //     let cache = get_cache().await;
//     //     let time = NaiveDateTime::and_utc(&Utc::now().naive_utc()).naive_utc();
//     //     let (event_1, event_2) = make_two_fills();

//     //     let v1 = vec![event_1.clone()];
//     //     let v2 = vec![event_2.clone()];

//     //     let pairs_none = Openbook::get_buy_sell_pairs(v1, &cache, time)
//     //         .await
//     //         .unwrap();
//     //     assert_eq!(pairs_none.len(), 0);

//     //     let pairs_one = Openbook::get_buy_sell_pairs(v2, &cache, time)
//     //         .await
//     //         .unwrap();
//     //     assert_eq!(pairs_one.len(), 1);
//     // }
// }
