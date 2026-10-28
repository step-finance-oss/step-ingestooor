use std::ops::{Div, Mul};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use borsh::BorshDeserialize;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

use crate::dex::phoenix::events::PhoenixMarketEvent;
use log::warn;
use solana_transaction_status::option_serializer::OptionSerializer;
use step_ingestooor_sdk::{
    dooot::{ClobFillEventDooot, Dooot},
    trait_defs::transaction::TransactionLike,
    utils::{step_utils::StepUtils, util_fns::get_partially_decoded_variant},
};

use super::{events::FillEvent, markets::PhoenixMarket};

pub const PHOENIX_V1_PROGRAM: &str = "PhoeNiXZ8ByJGLkxNfZRnkUfjvmuYqLR89jjFHGqdXY";

const MARKET_LEN: usize = std::mem::size_of::<PhoenixMarket>();
const AUDIT_LOG_HEADER_LEN: usize = 92;

pub struct PhoenixV1;

#[async_trait]
impl crate::TransactionParserTrait for PhoenixV1 {
    fn get_name(&self) -> &'static str {
        "Phoenix v1"
    }
    fn get_filters(&self) -> Vec<crate::TransactionParserFilter> {
        vec![crate::TransactionParserFilter::ProgramId(
            PHOENIX_V1_PROGRAM.to_string(),
        )]
    }
    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let fill_events = self.get_fill_events(txn)?;
        for group in fill_events {
            let Some(market_data) = txn.get_account_data_after(&group.market)? else {
                // Closed market
                continue;
            };

            let market = PhoenixMarket::try_from_slice(&market_data[..MARKET_LEN])
                .map_err(|e| anyhow!("Failed to parse market: {}", e))?;

            for fill_event in group.events {
                let base_decimals_divisor = 10u64.pow(market.header.base_params.decimals);
                let base_atoms = market
                    .header
                    .base_lot_size
                    .checked_mul(fill_event.base_lots_filled)
                    .ok_or_else(|| anyhow!("Overflow {}", txn.get_signature()))?;
                // Store the raw base_atoms as u64 for base_qty
                let base_units_f64 = base_atoms as f64 / base_decimals_divisor as f64;

                let _quote_decimals_divisor = 10u64.pow(market.header.quote_params.decimals);
                let price_atoms = market
                    .header
                    .tick_size_in_quote_atoms_per_base_unit
                    .checked_mul(fill_event.price_in_ticks)
                    .ok_or_else(|| anyhow!("Overflow {}", txn.get_signature()))?;
                // atoms-per-atoms price, no decimal adjustment
                let price = Decimal::from(price_atoms);

                // still need fee calculation as float for now
                let price_f64 = price
                    .to_f64()
                    .ok_or_else(|| anyhow!("Failed to convert Decimal price to f64"))?;
                let fee_units = base_units_f64
                    .mul(price_f64)
                    .mul(market.taker_fee_bps as f64)
                    .div(10000.0);
                let buy_side = fill_event.order_sequence_number.to_le_bytes()[0] == 1;

                dooot.push(Dooot::ClobFillEvent(ClobFillEventDooot {
                    time: txn.get_block_time(),
                    program_id: PHOENIX_V1_PROGRAM.to_string(),
                    market_pubkey: group.market.clone(),
                    base_mint_pubkey: market.header.base_params.mint_key.to_string(),
                    quote_mint_pubkey: market.header.quote_params.mint_key.to_string(),
                    taker_pubkey: group.signer.clone(),
                    maker_pubkey: fill_event.maker_id.to_string(),
                    buy_side,
                    base_qty: base_atoms,
                    price,
                    taker_fee: fee_units,
                    maker_fee: 0.0,
                    referrer_pubkey: None,
                    referral_fee: 0.0,
                }))
            }
        }

        Ok(())
    }
}

pub struct PhoenixFillEventsGroup {
    market: String,
    signer: String,
    events: Vec<FillEvent>,
}

impl PhoenixV1 {
    pub fn get_fill_events(
        &self,
        txn: &impl TransactionLike,
    ) -> Result<Vec<PhoenixFillEventsGroup>> {
        let mut events = Vec::<PhoenixFillEventsGroup>::new();

        let OptionSerializer::Some(inners) = txn.get_inner_instructions() else {
            return Ok(events);
        };

        for ix in inners {
            for inner_ix in &ix.instructions {
                let Some(partial_dec) = get_partially_decoded_variant(inner_ix) else {
                    continue;
                };
                if partial_dec.program_id != PHOENIX_V1_PROGRAM {
                    continue;
                }

                // Ripped from https://github.com/Ellipsis-Labs/phoenix-sdk/blob/afb23a9db6c745159e12781bea363044c8831cb7/rust/phoenix-sdk-core/src/sdk_client_core.rs#L468
                let decoded = bs58::decode(&partial_dec.data).into_vec()?;
                let Some((tag, event)) = decoded.split_first() else {
                    continue;
                };
                if *tag != 15 {
                    // Not a log ix
                    continue;
                }

                let Some(PhoenixMarketEvent::Header(header_event)) =
                    PhoenixMarketEvent::try_from_slice(&event[..AUDIT_LOG_HEADER_LEN]).ok()
                else {
                    warn!("Failed to parse header event");
                    continue;
                };

                let offset = AUDIT_LOG_HEADER_LEN;
                let mut phoenix_event_bytes =
                    (header_event.total_events as u32).to_le_bytes().to_vec();
                phoenix_event_bytes.extend_from_slice(&event[offset..]);

                let Some(market_events) =
                    Vec::<PhoenixMarketEvent>::try_from_slice(&phoenix_event_bytes).ok()
                else {
                    warn!("Failed to parse market events");
                    continue;
                };
                let fill_events = market_events.iter().filter_map(|e| match e {
                    PhoenixMarketEvent::Fill(fill_event) => Some(*fill_event),
                    _ => None,
                });
                events.push(PhoenixFillEventsGroup {
                    market: header_event.market.to_string(),
                    signer: header_event.signer.to_string(),
                    events: fill_events.collect(),
                });
            }
        }

        Ok(events)
    }
}
