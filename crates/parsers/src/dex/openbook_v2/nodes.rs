use borsh::BorshDeserialize;
use bytemuck::cast_ref;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use solana_sdk::pubkey::Pubkey;
use step_ingestooor_idl_gens::openbook_v2_gen::openbook_v2::types::NodeHandle;

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable, borsh::BorshDeserialize,
)]
#[repr(C, packed(8))]
pub struct LeafNode {
    /// NodeTag
    pub tag: u8,

    /// Index into the owning OpenOrdersAccount's OpenOrders
    pub owner_slot: u8,

    /// Time in seconds after `timestamp` at which the order expires.
    /// A value of 0 means no expiry.
    pub time_in_force: u16,

    pub padding: [u8; 4],

    /// The binary tree key, see new_node_key()
    pub key: u128,

    /// Address of the owning OpenOrdersAccount
    pub owner: Pubkey,

    /// Number of base lots to buy or sell, always >=1
    pub quantity: i64,

    /// The time the order was placed
    pub timestamp: u64,

    /// If the effective price of an oracle pegged order exceeds this limit,
    /// it will be considered invalid and may be removed.
    ///
    /// Only applicable in the oracle_pegged OrderTree
    pub peg_limit: i64,

    /// User defined id for this order, used in FillEvents
    pub client_order_id: u64,
}

impl LeafNode {
    /// The order's price_data as stored in the key
    ///
    /// Needs to be unpacked differently for fixed and oracle pegged orders.
    #[inline(always)]
    pub fn price_data(&self) -> u64 {
        (self.key >> 64) as u64
    }

    /// Time at which this order will expire, u64::MAX if never
    #[inline(always)]
    pub fn expiry(&self) -> u64 {
        if self.time_in_force == 0 {
            u64::MAX
        } else {
            self.timestamp + self.time_in_force as u64
        }
    }

    /// Returns if the order is expired at `now_ts`
    #[inline(always)]
    pub fn is_expired(&self, now_ts: u64) -> bool {
        self.time_in_force > 0 && now_ts >= self.timestamp + self.time_in_force as u64
    }
}

#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, BorshDeserialize)]
#[repr(C, packed(8))]
pub struct InnerNode {
    pub tag: u8, // NodeTag
    pub padding: [u8; 3],
    /// number of highest `key` bits that all children share
    /// e.g. if it's 2, the two highest bits of `key` will be the same on all children
    pub prefix_len: u32,

    /// only the top `prefix_len` bits of `key` are relevant
    pub key: u128,

    /// indexes into `BookSide::nodes`
    pub children: [NodeHandle; 2],

    /// The earliest expiry timestamp for the left and right subtrees.
    ///
    /// Needed to be able to find and remove expired orders without having to
    /// iterate through the whole bookside.
    pub child_earliest_expiry: [u64; 2],

    pub reserved: [u8; 40],
}

pub(crate) enum NodeRef<'a> {
    Inner(&'a InnerNode),
    Leaf(&'a LeafNode),
}

#[derive(IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum NodeTag {
    Uninitialized = 0,
    InnerNode = 1,
    LeafNode = 2,
    FreeNode = 3,
    LastFreeNode = 4,
}

#[derive(Copy, Clone, Debug, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
pub struct AnyNode {
    pub tag: u8,
    pub data: [u8; 79],
    // essential to make AnyNode alignment the same as other node types
    pub force_align: u64,
}

impl AnyNode {
    #[inline]
    pub fn as_leaf(&self) -> Option<&LeafNode> {
        match self.case() {
            Some(NodeRef::Leaf(leaf_ref)) => Some(leaf_ref),
            _ => None,
        }
    }
    pub(crate) fn case(&self) -> Option<NodeRef> {
        match NodeTag::try_from(self.tag) {
            Ok(NodeTag::InnerNode) => Some(NodeRef::Inner(cast_ref(self))),
            Ok(NodeTag::LeafNode) => Some(NodeRef::Leaf(cast_ref(self))),
            _ => None,
        }
    }
}

/// Creates price data for an oracle pegged order from the price offset
///
/// Reverse of oracle_pegged_price_offset()
pub fn oracle_pegged_price_data(price_offset_lots: i64) -> u64 {
    // Price data is used for ordering in the bookside's top bits of the u128 key.
    // Map i64::MIN to be 0 and i64::MAX to u64::MAX, that way comparisons on the
    // u64 produce the same result as on the source i64.
    // Equivalent: (price_offset_lots as i128 - (i64::MIN as i128) as u64
    (price_offset_lots as u64).wrapping_add(u64::MAX / 2 + 1)
}

/// Retrieves the price offset (in lots) from an oracle pegged order's price data
///
/// Reverse of oracle_pegged_price_data()
pub fn oracle_pegged_price_offset(price_data: u64) -> i64 {
    price_data.wrapping_sub(u64::MAX / 2 + 1) as i64
}

/// Creates price data for a fixed order's price
///
/// Reverse of fixed_price_lots()
pub fn fixed_price_data(price_lots: i64) -> u64 {
    price_lots as u64
}

/// Retrieves the price (in lots) from a fixed order's price data
///
/// Reverse of fixed_price_data().
pub fn fixed_price_lots(price_data: u64) -> i64 {
    assert!(price_data <= i64::MAX as u64);
    price_data as i64
}
