use num_enum::{IntoPrimitive, TryFromPrimitive, TryFromPrimitiveError};
use step_ingestooor_idl_gens::openbook_v2_gen::openbook_v2::types::NodeHandle;

use super::{
    nodes::{AnyNode, InnerNode, LeafNode, NodeRef, NodeTag},
    order_type::Side,
};

pub const MAX_ORDERTREE_NODES: usize = 1024;

#[derive(
    Eq, PartialEq, Copy, Clone, TryFromPrimitive, IntoPrimitive, Debug, borsh::BorshDeserialize,
)]
#[repr(u8)]
pub enum OrderTreeType {
    Bids,
    Asks,
}

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable, borsh::BorshDeserialize,
)]
#[repr(C)]
pub struct OrderTreeRoot {
    pub maybe_node: NodeHandle,
    pub leaf_count: u32,
}

impl OrderTreeRoot {
    pub fn node(&self) -> Option<NodeHandle> {
        if self.leaf_count == 0 {
            None
        } else {
            Some(self.maybe_node)
        }
    }
}

/// Iterate over orders in order (bids=descending, asks=ascending)
pub struct OrderTreeIter<'a> {
    order_tree: &'a OrderTreeNodes,
    /// InnerNodes where the right side still needs to be iterated on
    stack: Vec<&'a InnerNode>,
    /// To be returned on `next()`
    next_leaf: Option<(NodeHandle, &'a LeafNode)>,

    /// either 0, 1 to iterate low-to-high, or 1, 0 to iterate high-to-low
    left: usize,
    right: usize,
}

impl<'a> OrderTreeIter<'a> {
    pub fn new(order_tree: &'a OrderTreeNodes, root: &OrderTreeRoot) -> anyhow::Result<Self> {
        let (left, right) = if order_tree.order_tree_type()? == OrderTreeType::Bids {
            (1, 0)
        } else {
            (0, 1)
        };
        let stack = vec![];

        let mut iter = Self {
            order_tree,
            stack,
            next_leaf: None,
            left,
            right,
        };
        if let Some(r) = root.node() {
            iter.next_leaf = iter.find_leftmost_leaf(r);
        }
        Ok(iter)
    }

    fn find_leftmost_leaf(&mut self, start: NodeHandle) -> Option<(NodeHandle, &'a LeafNode)> {
        let mut current = start;
        loop {
            match self
                .order_tree
                .node(current)
                .expect("Node should exist")
                .case()
                .expect("Node should exist")
            {
                NodeRef::Inner(inner) => {
                    self.stack.push(inner);
                    current = inner.children[self.left];
                }
                NodeRef::Leaf(leaf) => {
                    return Some((current, leaf));
                }
            }
        }
    }

    pub fn peek(&self) -> Option<(NodeHandle, &'a LeafNode)> {
        self.next_leaf
    }

    pub fn side(&self) -> Side {
        if self.left == 1 {
            Side::Bid
        } else {
            Side::Ask
        }
    }
}

impl<'a> Iterator for OrderTreeIter<'a> {
    type Item = (NodeHandle, &'a LeafNode);

    fn next(&mut self) -> Option<Self::Item> {
        // no next leaf? done
        self.next_leaf?;

        // start popping from stack and get the other child
        let current_leaf = self.next_leaf;
        self.next_leaf = match self.stack.pop() {
            None => None,
            Some(inner) => {
                let start = inner.children[self.right];
                // go down the left branch as much as possible until reaching a leaf
                self.find_leftmost_leaf(start)
            }
        };

        current_leaf
    }
}

/// A binary tree on AnyNode::key()
///
/// The key encodes the price in the top 64 bits.
#[derive(Copy, Clone, Debug, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
pub struct OrderTreeNodes {
    pub order_tree_type: u8, // OrderTreeType, but that's not POD
    pub padding: [u8; 3],
    pub bump_index: u32,
    pub free_list_len: u32,
    pub free_list_head: NodeHandle,
    pub reserved: [u8; 512],
    pub nodes: [AnyNode; MAX_ORDERTREE_NODES],
}

impl OrderTreeNodes {
    pub fn order_tree_type(&self) -> Result<OrderTreeType, TryFromPrimitiveError<OrderTreeType>> {
        OrderTreeType::try_from(self.order_tree_type)
    }

    /// Iterate over all entries, including invalid orders
    ///
    /// smallest to highest for asks
    /// highest to smallest for bids
    pub fn iter(&self, root: &OrderTreeRoot) -> anyhow::Result<OrderTreeIter> {
        OrderTreeIter::new(self, root)
    }

    pub fn node(&self, handle: NodeHandle) -> Option<&AnyNode> {
        let node = &self.nodes[handle as usize];
        let tag = NodeTag::try_from(node.tag);
        match tag {
            Ok(NodeTag::InnerNode) | Ok(NodeTag::LeafNode) => Some(node),
            _ => None,
        }
    }
}
