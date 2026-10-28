//! Ripped from https://ellipsis-labs.gitbook.io/phoenix-dex/tRIkEFlLUzWK9uKO3W2V/getting-started/technical-overview/key-structures
//! But is slightly modified!!

use borsh::{BorshDeserialize, BorshSerialize};

use solana_sdk::pubkey::Pubkey;

/*
*****************
Top-level structs
*****************
*/

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone)]
pub struct MarketHeader {
    pub discriminant: u64,

    pub status: u64,

    /// The size params of the market.
    pub market_params: MarketSizeParams,

    /// The specification of the base token of the market.
    pub base_params: TokenParams,

    /// The lot size of the base token of the market, in base atoms.
    pub base_lot_size: u64,

    /// The specification of the quote token of the market.
    pub quote_params: TokenParams,

    /// The lot size of the quote token of the market, in quote atoms.
    pub quote_lot_size: u64,

    /// The number of quote atoms per base unit per tick in the market.
    pub tick_size_in_quote_atoms_per_base_unit: u64,

    /// The Pubkey of the market authority.
    pub authority: Pubkey,

    /// The fee recipient of the market
    pub fee_recipient: Pubkey,

    /// The sequence number of the market.
    pub market_sequence_number: u64,

    /// The authority's successor. This key must sign the `ClaimAuthority`
    /// instruction to become the new market authority
    pub successor: Pubkey,

    pub raw_base_units_per_base_unit: u32,
    _padding1: u32,
    _padding2: [u64; 32],
}

/// AKA `FIFOMarket`, see https://ellipsis-labs.gitbook.io/phoenix-dex/tRIkEFlLUzWK9uKO3W2V/getting-started/technical-overview/key-structures
#[derive(BorshDeserialize, BorshSerialize, Debug, Clone)]
pub struct PhoenixMarket {
    pub header: MarketHeader,

    /// Padding
    pub _padding: [u64; 32],

    /// Number of base lots in a base unit. For example, if the base lot
    /// size (in base units) is 0.001 SOL, then base_lots_per_base_unit is
    /// 1000.
    pub base_lots_per_base_unit: u64,

    /// Tick size in quote lots per base unit. For example, if the tick size
    /// (in quote units per base units) is 0.01 USDC per SOL and the quote
    /// lot size (in quote units) is 0.001 USDC, then
    /// tick_size_in_quote_lots_per_base_unit is 10.
    pub tick_size_in_quote_lots_per_base_unit: u64,

    /// The sequence number of the next event.
    pub order_sequence_number: u64,

    /// There are no maker fees. Taker fees are charged on the quote lots
    /// transacted in the trade, in basis points.
    pub taker_fee_bps: u64,

    /// Amount of fees collected from the market in its lifetime, in quote
    /// lots.
    pub collected_quote_lot_fees: u64,

    /// Amount of unclaimed fees accrued to the market, in quote lots.
    pub unclaimed_quote_lot_fees: u64,
    // /// Red-black tree representing the bids in the order book.
    // pub bids: RedBlackTree<FIFOOrderId, FIFORestingOrder, BIDS_SIZE>,

    // /// Red-black tree representing the asks in the order book.
    // pub asks: RedBlackTree<FIFOOrderId, FIFORestingOrder, ASKS_SIZE>,

    // /// Red-black tree representing the authorized makers in the market.
    // pub traders: RedBlackTree<Pubkey, TraderState, NUM_SEATS>,
}

/*
*************
Inner structs
*************
*/

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone)]
pub struct MarketSizeParams {
    pub bids_size: u64,
    pub asks_size: u64,
    pub num_seats: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone)]
pub struct TokenParams {
    /// Number of decimals for the token (e.g. 9 for SOL, 6 for USDC).
    pub decimals: u32,

    /// Bump used for generating the PDA for the market's token vault.
    pub vault_bump: u32,

    /// Pubkey of the token mint.
    pub mint_key: Pubkey,

    /// Pubkey of the token vault.
    pub vault_key: Pubkey,
}
