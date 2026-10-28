//! Phoenix Layouts ripped from https://github.com/Ellipsis-Labs/phoenix-v1/blob/master/src/program/events.rs

use borsh::{BorshDeserialize, BorshSerialize};
use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct AuditLogHeader {
    pub instruction: u8,
    pub sequence_number: u64,
    pub timestamp: i64,
    pub slot: u64,
    pub market: Pubkey,
    pub signer: Pubkey,
    pub total_events: u16,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct FillEvent {
    pub index: u16,
    pub maker_id: Pubkey,
    pub order_sequence_number: u64,
    pub price_in_ticks: u64,
    pub base_lots_filled: u64,
    pub base_lots_remaining: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct ReduceEvent {
    pub index: u16,
    pub order_sequence_number: u64,
    pub price_in_ticks: u64,
    pub base_lots_removed: u64,
    pub base_lots_remaining: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct PlaceEvent {
    pub index: u16,
    pub order_sequence_number: u64,
    pub client_order_id: u128,
    pub price_in_ticks: u64,
    pub base_lots_placed: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct EvictEvent {
    pub index: u16,
    pub maker_id: Pubkey,
    pub order_sequence_number: u64,
    pub price_in_ticks: u64,
    pub base_lots_evicted: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct FillSummaryEvent {
    pub index: u16,
    pub client_order_id: u128,
    pub total_base_lots_filled: u64,
    pub total_quote_lots_filled: u64,
    pub total_fee_in_quote_lots: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct FeeEvent {
    pub index: u16,
    pub fees_collected_in_quote_lots: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct TimeInForceEvent {
    pub index: u16,
    pub order_sequence_number: u64,
    pub last_valid_slot: u64,
    pub last_valid_unix_timestamp_in_seconds: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct ExpiredOrderEvent {
    pub index: u16,
    pub maker_id: Pubkey,
    pub order_sequence_number: u64,
    pub price_in_ticks: u64,
    pub base_lots_removed: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub enum PhoenixMarketEvent {
    Uninitialized,
    Header(AuditLogHeader),
    Fill(FillEvent),
    Place(PlaceEvent),
    Reduce(ReduceEvent),
    Evict(EvictEvent),
    FillSummary(FillSummaryEvent),
    Fee(FeeEvent),
    TimeInForce(TimeInForceEvent),
    ExpiredOrder(ExpiredOrderEvent),
}
