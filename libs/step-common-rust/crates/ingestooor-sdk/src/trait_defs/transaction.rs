use std::{collections::HashMap, iter::Peekable, vec};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use base64::{prelude::BASE64_STANDARD, Engine};
use borsh::BorshDeserialize;
use bytemuck::{Pod, Zeroable};
use chrono::NaiveDateTime;
use regex::Regex;
use serde::Serialize;
use slab_tree::{NodeMut, Tree, TreeBuilder};
use solana_sdk::bs58;
use solana_transaction_status::{
    option_serializer::OptionSerializer, parse_accounts::ParsedAccount, UiInnerInstructions,
    UiInstruction, UiParsedInstruction, UiPartiallyDecodedInstruction, UiTransactionTokenBalance,
};

use crate::utils::{
    constants::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID},
    simple_stack::SimpleStack,
    spl_ixs::{get_parsed_token_program_instruction, HasAuthority, TokenParsedInstruction},
    util_fns::{
        decode_base64_compressed_data, get_parsed_variant, get_stack_height_of_ix, zerocopy_data,
    },
};

use super::{
    discriminatable::Discriminatable,
    pod_types::{AsBorsh, RefFromByteRef},
    step_option_helpers::StepOptionHelperTrait,
    typed_accounts::{
        make_accounts_typed, make_accounts_typed_2, make_accounts_typed_3, AccountTypedPrePost,
    },
};

#[derive(Clone, Debug)]
pub struct AccountDatumIterItem<'a> {
    pub pubkey: &'a str,
    /// Strictly is if `pre_datum != post_datum`.
    ///
    /// Does not consider pre_datum being `None` and post_datum being `Some` due to
    /// geyser config.
    /// This scenario would be `true`
    pub datum_changed: bool,
    pub pre_owner_program: Option<&'a str>,
    pub post_owner_program: Option<&'a str>,
    pub pre_datum: Option<&'a str>,
    pub post_datum: Option<&'a str>,
    pub was_created: bool,
    pub was_closed: bool,
}

impl AccountDatumIterItem<'_> {
    fn unpack_datum(&self, pre_or_post: TxAccountInfoStage) -> Result<Option<Vec<u8>>> {
        let datum = match pre_or_post {
            TxAccountInfoStage::Pre => self.pre_datum,
            TxAccountInfoStage::Post => self.post_datum,
        };

        // Some geyser sources can emit empty strings for datum. Treat these as an
        // existing, but empty byte vector instead of attempting to zstd-decompress,
        // which would error with an "incomplete frame".
        if let Some(s) = datum {
            if s.is_empty() {
                return Ok(Some(Vec::new()));
            }
        }

        datum.map(decode_base64_compressed_data).transpose()
    }

    pub fn unpack_post_datum(&self) -> Result<Option<Vec<u8>>> {
        self.unpack_datum(TxAccountInfoStage::Post)
    }

    pub fn unpack_pre_datum(&self) -> Result<Option<Vec<u8>>> {
        self.unpack_datum(TxAccountInfoStage::Pre)
    }
}

#[derive(Serialize, Clone, PartialEq, Eq, Debug)]
pub struct FlattenedIxWithIndex<'a> {
    pub instruction: &'a UiInstruction,
    pub is_inner: bool,
    pub stack_height: Option<&'a u32>,
    pub index: u8,
}

#[derive(Debug)]
pub struct GroupedInstruction<'a> {
    pub parent_instruction: FlattenedIxWithIndex<'a>,
    pub involved_instructions: Vec<FlattenedIxWithIndex<'a>>,
}

pub struct TokenAccountInfo {
    pub account_index: u8,
    pub mint: String,
    pub owner: Option<String>,
}

#[derive(PartialEq, Debug)]
pub enum TransferDirection {
    Send,
    Receive,
    Zero,
}

#[derive(Debug)]
pub struct DirectionalTransferAmounts {
    in_amount: i128,
    out_amount: i128,
}

impl DirectionalTransferAmounts {
    pub fn is_zero(&self) -> bool {
        self.in_amount == 0 && self.out_amount == 0
    }

    pub const fn direction(&self) -> TransferDirection {
        let in_amount = self.in_amount;
        let out_amount = self.out_amount;

        if in_amount > out_amount {
            TransferDirection::Send
        } else if out_amount > in_amount {
            TransferDirection::Receive
        } else {
            TransferDirection::Zero
        }
    }

    pub fn in_amount_u64(&self) -> u64 {
        self.in_amount as u64
    }

    pub fn out_amount_u64(&self) -> u64 {
        self.out_amount as u64
    }

    pub fn in_amount_i128(&self) -> i128 {
        self.in_amount
    }

    pub fn out_amount_i128(&self) -> i128 {
        self.out_amount
    }
}

#[derive(Debug)]
pub struct AggregatedTransferAmounts {
    inner_map: HashMap<String, HashMap<String, DirectionalTransferAmounts>>,
}

impl AggregatedTransferAmounts {
    pub fn new(inner_map: HashMap<String, HashMap<String, DirectionalTransferAmounts>>) -> Self {
        Self { inner_map }
    }

    pub fn get_is_zero(&self, mint: &str, auth_pk: &str) -> bool {
        self.inner_map
            .get(auth_pk)
            .and_then(|txfers| txfers.get(mint))
            .map_or(true, |v| v.is_zero())
    }

    pub fn get_direction(&self, mint: &str, auth_pk: &str) -> TransferDirection {
        self.inner_map
            .get(auth_pk)
            .and_then(|txfers| txfers.get(mint))
            .map_or(TransferDirection::Zero, |v| v.direction())
    }

    pub fn get_in_amount_u64(&self, mint: &str, auth_pk: &str) -> u64 {
        self.inner_map
            .get(auth_pk)
            .and_then(|txfers| txfers.get(mint))
            .map_or(0, |v| v.in_amount_u64())
    }

    pub fn get_out_amount_u64(&self, mint: &str, auth_pk: &str) -> u64 {
        self.inner_map
            .get(auth_pk)
            .and_then(|txfers| txfers.get(mint))
            .map_or(0, |v| v.out_amount_u64())
    }

    pub fn get_in_amount_i128(&self, mint: &str, auth_pk: &str) -> i128 {
        self.inner_map
            .get(auth_pk)
            .and_then(|txfers| txfers.get(mint))
            .map_or(0, |v| v.in_amount_i128())
    }

    pub fn get_out_amount_i128(&self, mint: &str, auth_pk: &str) -> i128 {
        self.inner_map
            .get(auth_pk)
            .and_then(|txfers| txfers.get(mint))
            .map_or(0, |v| v.out_amount_i128())
    }

    pub fn get_total_in_amount_u64(&self, mint: &str) -> u64 {
        self.inner_map
            .values()
            .map(|txfers| txfers.get(mint).map_or(0, |v| v.in_amount_u64()))
            .sum()
    }

    pub fn get_total_out_amount_u64(&self, mint: &str) -> u64 {
        self.inner_map
            .values()
            .map(|txfers| txfers.get(mint).map_or(0, |v| v.out_amount_u64()))
            .sum()
    }

    pub fn get_total_in_amount_i128(&self, mint: &str) -> i128 {
        self.inner_map
            .values()
            .map(|txfers| txfers.get(mint).map_or(0, |v| v.in_amount_i128()))
            .sum()
    }

    pub fn get_total_out_amount_i128(&self, mint: &str) -> i128 {
        self.inner_map
            .values()
            .map(|txfers| txfers.get(mint).map_or(0, |v| v.out_amount_i128()))
            .sum()
    }
}

pub enum TxAccountInfoStage {
    Pre,
    Post,
}

#[async_trait]
pub trait TransactionLike: Send + Sync + Serialize + std::fmt::Debug {
    // Basic getters for txn objects

    /// NaiveDateTime of block time + slots in same second * 0.1s + tx index_in_block as micros
    fn get_block_time(&self) -> NaiveDateTime;
    fn get_slot(&self) -> u64;
    fn get_signature(&self) -> &str;
    fn get_outer_instructions(&self) -> &Vec<UiInstruction>;
    fn get_inner_instructions(&self) -> &OptionSerializer<Vec<UiInnerInstructions>>;
    fn get_account_keys(&self) -> &Vec<ParsedAccount>;
    fn get_pre_token_balances(&self) -> &OptionSerializer<Vec<UiTransactionTokenBalance>>;
    fn get_post_token_balances(&self) -> &OptionSerializer<Vec<UiTransactionTokenBalance>>;
    fn get_pre_datums(&self) -> &Option<Vec<Option<String>>>;
    fn get_post_datums(&self) -> &Option<Vec<Option<String>>>;
    fn get_logs(&self) -> &OptionSerializer<Vec<String>>;
    fn get_pre_owners(&self) -> &Option<Vec<Option<String>>>;
    fn get_post_owners(&self) -> &Option<Vec<Option<String>>>;

    // Util fns

    /// Returns an iterator over every instruction in the transaction.
    /// First iterates through the outer instructions, then the inner instructions.
    /// You can check if the instruction you're iterating on is an inner by checking `is_inner`
    /// ```ignore
    /// for ix in transaction.get_flattened_ixs() {
    ///     if (ix.is_inner) {
    ///         // Do something with inner...
    ///     } else {
    ///         // Do something with outer...
    ///     }
    /// }
    /// ```
    fn get_flattened_ixs(&self) -> Box<dyn Iterator<Item = FlattenedIxWithIndex> + Send + '_> {
        let outer_ixs = self
            .get_outer_instructions()
            .iter()
            .enumerate()
            .map(|(idx, ix)| FlattenedIxWithIndex {
                index: idx as u8,
                is_inner: false,
                stack_height: get_stack_height_of_ix(ix),
                instruction: ix,
            });

        if let OptionSerializer::Some(inner_ixs_vec) = self.get_inner_instructions() {
            let chained = outer_ixs.chain(inner_ixs_vec.iter().flat_map(|inner_ixs| {
                inner_ixs
                    .instructions
                    .iter()
                    .map(|ix| FlattenedIxWithIndex {
                        index: inner_ixs.index,
                        is_inner: true,
                        stack_height: get_stack_height_of_ix(ix),
                        instruction: ix,
                    })
            }));
            Box::new(chained)
        } else {
            Box::new(outer_ixs)
        }
    }

    #[inline]
    fn get_non_parsed_program_ixs<'a>(
        &'a self,
        program_pubkeys: &'a [String],
    ) -> impl Iterator<Item = &'a UiPartiallyDecodedInstruction> {
        self.get_flattened_ixs()
            .filter_map(move |ix| match ix.instruction {
                UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(ref ix)) => {
                    if program_pubkeys.contains(&ix.program_id) {
                        Some(ix)
                    } else {
                        None
                    }
                }
                _ => None,
            })
    }

    /// Returns a tree of the transaction's instructions.
    ///
    /// This includes the full nested tree structure of inner txs with the proper
    /// parents using their call stacks.
    /// The root node is None, and the children are all Some(_). Thus, you can safely unwrap any node that has a parent.
    fn get_ix_tree(&self) -> Result<Tree<Option<&UiInstruction>>> {
        let mut tree = TreeBuilder::new().with_root(None).build();
        let mut root = tree
            .root_mut()
            .context("UNREACHABLE - root was just inserted above")?;

        let outer_ixs = self.get_outer_instructions().iter().enumerate();

        let mut inner_ixs = match self.get_inner_instructions() {
            OptionSerializer::Some(inner_ixs_vec) => inner_ixs_vec.iter(),
            _ => [].iter(),
        };

        //popped out into a function to allow recursion
        fn process_inner<'a>(
            node: &mut NodeMut<Option<&'a UiInstruction>>,
            stack_depth: u32,
            inner_ixs: &mut Peekable<impl Iterator<Item = &'a UiInstruction>>,
        ) -> Result<()> {
            let stack_depth_to_process = stack_depth + 1;
            loop {
                let curr_ix = inner_ixs.peek();
                if curr_ix.is_none() {
                    //actually iterate to move the cursor forward to the None
                    _ = inner_ixs.next();
                    break;
                }
                let curr_ix_inner = curr_ix.context("UNREACHABLE - curr_ix should exist")?;
                let curr_ix_stack_depth =
                    *get_stack_height_of_ix(curr_ix_inner).ok_or_else(|| {
                        anyhow!("get_ix_tree process_inner - No stack height present on inner ix?")
                    })?;
                match curr_ix_stack_depth.cmp(&stack_depth_to_process) {
                    //this is a child, so we append it
                    std::cmp::Ordering::Equal => {
                        //actually iterate the inner because we're consuming it
                        let curr_ix = inner_ixs.next();
                        _ = node.append(curr_ix);
                    }
                    //this is a grandchild, so we recurse
                    std::cmp::Ordering::Greater => {
                        //unwrap is safe because we know we have a child
                        let mut last_node = node
                            .last_child()
                            .context("UNREACHABLE - last child should exist")?;
                        //recurse
                        process_inner(&mut last_node, stack_depth_to_process, inner_ixs)?;
                    }
                    //this is a sibling or uncle, so we break
                    std::cmp::Ordering::Less => {
                        break;
                    }
                }
            }
            Ok(())
        }

        //setup to manually iterate the inners
        let mut curr_inners = inner_ixs.next();
        //loop
        for (idx, ix) in outer_ixs {
            let mut node = root.append(Some(ix));
            if curr_inners.is_some_and(|a| a.index == idx as u8) {
                let mut inners_iter = curr_inners
                    .context("UNREACHABLE- curr_inners should be Some here")?
                    .instructions
                    .iter()
                    .peekable();
                process_inner(&mut node, 1, &mut inners_iter)?;
                //inner processed, iterate to next one
                curr_inners = inner_ixs.next();
            }
        }
        Ok(tree)
    }

    fn get_anchor_log_cpi_events(&self, program_id: &str) -> Result<Vec<Vec<u8>>> {
        let tree = self.get_ix_tree()?;
        let ixs = tree
            .root()
            .context("No root node found in ix tree")?
            .traverse_level_order()
            .skip(1);
        let mut events = Vec::new();
        for ix in ixs {
            if let UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(parsed)) =
                ix.data().context("No ix found in node")?
            {
                if parsed.program_id != program_id {
                    continue;
                }
                let data = bs58::decode(parsed.data.clone()).into_vec()?;
                if data.len() < 8 {
                    continue;
                }
                let disc = &data[0..8];
                //e445a52e51cb9a1d
                if disc != [0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d] {
                    continue;
                }
                events.push(data[8..].to_vec());
            }
        }
        Ok(events)
    }

    fn get_anchor_log_cpi_events_typed<T: for<'a> TryFrom<&'a [u8]>>(
        &self,
        program_id: &str,
    ) -> Result<Vec<T>> {
        let events = self.get_anchor_log_cpi_events(program_id)?;
        let mut deserialized_events = Vec::new();
        for event in events.into_iter() {
            let deserialized_event =
                T::try_from(&event).map_err(|_| anyhow!("Failed to deserialize event"))?;
            deserialized_events.push(deserialized_event);
        }
        Ok(deserialized_events)
    }

    /// Use get_account_data_after or get_account_data_before
    #[doc(hidden)]
    fn get_acct_data(
        &self,
        account: &str,
        pre_post: TxAccountInfoStage,
    ) -> Result<Option<Vec<u8>>> {
        let sig = self.get_signature();
        let acct_idx = self
            .get_account_keys()
            .iter()
            .position(|a| a.pubkey == *account)
            .ok_or_else(|| {
                anyhow!("get_acct_data: Account {account} not found in account keys for txn {sig}")
            })?;

        let datums = match pre_post {
            TxAccountInfoStage::Pre => self.get_pre_datums(),
            TxAccountInfoStage::Post => self.get_post_datums(),
        };

        let datum = datums
            .as_ref()
            .and_then(|datums| datums.get(acct_idx))
            .and_then(|o| o.as_deref())
            .map(decode_base64_compressed_data)
            .transpose();

        datum
    }

    fn get_first_signer(&self) -> Option<String> {
        let keys = self.get_account_keys();
        let signer_acc = keys.iter().find(|k| k.signer);
        if let Some(signer) = signer_acc {
            return Some(signer.pubkey.to_string());
        }
        None
    }

    fn get_token_decimals(&self, target_mint: &str) -> Result<u64> {
        if let OptionSerializer::Some(post_balances) = self.get_post_token_balances() {
            if let Some(bal) = post_balances.iter().find(|b| b.mint == target_mint) {
                return Ok(bal.ui_token_amount.decimals as u64);
            }
        }
        if let OptionSerializer::Some(pre_balances) = self.get_pre_token_balances() {
            if let Some(bal) = pre_balances.iter().find(|b| b.mint == target_mint) {
                return Ok(bal.ui_token_amount.decimals as u64);
            }
        }
        if let Ok(Some(data)) = self.get_account_data_after(target_mint) {
            if data.len() >= 45 {
                return Ok(data[44] as u64);
            }
        }
        Err(anyhow!(
            "No token balance entry for mint `{}` found",
            target_mint
        ))
    }

    fn get_token_account_data_by_key(&self, token_account: &str) -> Result<TokenAccountInfo> {
        let index = self
            .get_account_keys()
            .iter()
            .position(|x| x.pubkey == *token_account)
            .ok_or_else(|| anyhow!("Token account not found in account keys"))?;
        let mut post_balances = &vec![];
        if let OptionSerializer::Some(balances) = self.get_post_token_balances() {
            post_balances = balances;
        }

        let mut pre_balances = &vec![];
        if let OptionSerializer::Some(balances) = self.get_pre_token_balances() {
            pre_balances = balances;
        }

        let account_data = post_balances
            .iter()
            .chain(pre_balances.iter())
            .find(|p| p.account_index == index as u8);
        if let Some(data) = account_data {
            return Ok(TokenAccountInfo {
                account_index: index as u8,
                mint: data.mint.clone(),
                owner: data.owner.clone().into_option(),
            });
        }

        // Not found in balances, search for temp creations
        for ix in self.get_flattened_ixs() {
            let Some(info) = get_parsed_token_program_instruction(ix.instruction)? else {
                continue;
            };
            if let TokenParsedInstruction::TokenInitializeAccount(info) = info {
                if info.account == *token_account {
                    return Ok(TokenAccountInfo {
                        account_index: index as u8,
                        mint: info.mint,
                        owner: Some(info.owner),
                    });
                }
            }
        }

        Err(anyhow!("Token account not found"))
    }

    #[doc(hidden)]
    fn get_token_account_balance(
        &self,
        account: &str,
        pre_post: TxAccountInfoStage,
    ) -> Result<Option<u64>> {
        let acct_idx = self
            .get_account_keys()
            .iter()
            .position(|a| a.pubkey == *account)
            .ok_or(anyhow!(
                "get_token_account_balance: Account {account} not found in account keys for txn {}",
                self.get_signature()
            ))?;
        let balances = match pre_post {
            TxAccountInfoStage::Pre => self.get_pre_token_balances(),
            TxAccountInfoStage::Post => self.get_post_token_balances(),
        };
        let OptionSerializer::Some(balances) = balances else {
            return Err(anyhow!(
                "Requested pre/post balance type not found in tx {}",
                self.get_signature()
            ));
        };
        let token_account_balance =
            match balances.iter().find(|b| b.account_index == acct_idx as u8) {
                Some(b) => Some(b.ui_token_amount.amount.parse()?),
                None => return Ok(None),
            };
        Ok(token_account_balance)
    }

    /// Retrieves the pre-token balance for the specified account.
    /// If the balance is None, checks for a post-token balance; if found, returns 0, otherwise returns None.
    #[inline]
    fn get_pre_token_account_balance(&self, account: &str) -> Result<Option<u64>> {
        match self.get_token_account_balance(account, TxAccountInfoStage::Pre)? {
            Some(balance) => Ok(Some(balance)),
            None => {
                if self.get_post_token_account_balance(account)?.is_some() {
                    Ok(Some(0))
                } else {
                    Ok(None)
                }
            }
        }
    }

    #[inline]
    fn get_post_token_account_balance(&self, account: &str) -> Result<Option<u64>> {
        self.get_token_account_balance(account, TxAccountInfoStage::Post)
    }

    /// Returns the uncompressed `post_datum` account bytes for a given account key
    #[inline]
    fn get_account_data_after(&self, account: &str) -> Result<Option<Vec<u8>>> {
        self.get_acct_data(account, TxAccountInfoStage::Post)
    }

    /// Returns the uncompressed `pre_datum` account bytes for a given account key
    #[inline]
    fn get_account_data_before(&self, account: &str) -> Result<Option<Vec<u8>>> {
        self.get_acct_data(account, TxAccountInfoStage::Pre)
    }

    fn get_account_data_parsed_zerocopy<T: Pod + Zeroable>(
        &self,
        account_pubkey: &str,
        account_context: &str,
        ix_context: &str,
        pre_post: TxAccountInfoStage,
        disc_offset: Option<usize>,
    ) -> Result<Box<T>> {
        let data = match pre_post {
            TxAccountInfoStage::Pre => self.get_account_data_before(account_pubkey),
            TxAccountInfoStage::Post => self.get_account_data_after(account_pubkey),
        };
        let data = data?.ok_or_else(|| {
            anyhow!(
                "{} data not found in {}: {}",
                account_context,
                ix_context,
                account_pubkey
            )
        })?;
        zerocopy_data(data, disc_offset)
    }

    fn get_account_data_parsed_borsh<T: BorshDeserialize>(
        &self,
        account_pubkey: &str,
        account_context: &str,
        ix_context: &str,
        pre_post: TxAccountInfoStage,
        custom_disc: Option<usize>,
    ) -> Result<T> {
        let data = match pre_post {
            TxAccountInfoStage::Pre => self.get_account_data_before(account_pubkey),
            TxAccountInfoStage::Post => self.get_account_data_after(account_pubkey),
        };
        let data = data?.ok_or_else(|| {
            anyhow!(
                "{} data not found in {}: {}",
                account_context,
                ix_context,
                account_pubkey
            )
        })?;

        let parsed_data = T::try_from_slice(&data[custom_disc.unwrap_or(8)..]).map_err(|e| {
            anyhow!(
                "Failed to parse {} data in {}: {}",
                account_context,
                ix_context,
                e
            )
        })?;
        Ok(parsed_data)
    }

    fn get_post_balances(&self) -> &Vec<u64>;
    fn get_pre_balances(&self) -> &Vec<u64>;

    #[inline]
    fn iter_accounts(&self) -> impl Iterator<Item = AccountDatumIterItem> + '_ + Send + Sync {
        self.iter_accounts_filtered(|_| true)
    }

    #[inline]
    fn iter_accounts_filtered<'a, F>(
        &'a self,
        filter: F,
    ) -> impl Iterator<Item = AccountDatumIterItem<'a>> + 'a + Send + Sync
    where
        F: Fn(&ParsedAccount) -> bool + 'a + Send + Sync,
    {
        let keys = self.get_account_keys();
        let pre_dat = self.get_pre_datums().as_ref();
        let post_dat = self.get_post_datums().as_ref();
        let pre_owners = self.get_pre_owners().as_ref();
        let post_owners = self.get_post_owners().as_ref();
        let pre_balances = self.get_pre_balances();
        let post_balances = self.get_post_balances();

        // `keys` will always be guaranteed to be the longest (or eq) array with valid entries,
        // Enumerate that and use `get`
        let iter = keys
            .iter()
            .enumerate()
            .filter(move |&(_, a)| filter(a))
            .map(move |(idx, acc)| {
                let pubkey = &acc.pubkey;
                let pre_datum = pre_dat.and_then(|v| v.get(idx)).and_then(|o| o.as_deref());
                let post_datum = post_dat.and_then(|v| v.get(idx)).and_then(|o| o.as_deref());
                let pre_owner_program = pre_owners
                    .and_then(|v| v.get(idx))
                    .and_then(|o| o.as_deref());
                let post_owner_program = post_owners
                    .and_then(|v| v.get(idx))
                    .and_then(|o| o.as_deref());
                let pre_sol_balance = pre_balances.get(idx);
                let post_sol_balance = post_balances.get(idx);
                let was_created = (pre_datum.is_none()
                    || pre_datum.as_ref().is_some_and(|d| d.is_empty()))
                    && post_datum.as_ref().is_some_and(|d| !d.is_empty());
                let was_closed = ((post_datum.is_none()
                    || post_datum.is_some_and(|d| d.is_empty()))
                    && pre_datum.is_some_and(|d| !d.is_empty()))
                    || (post_sol_balance.is_none() || post_sol_balance.is_some_and(|d| *d == 0))
                        && (pre_sol_balance.is_some_and(|d| *d > 0));
                AccountDatumIterItem {
                    pubkey,
                    pre_datum,
                    post_datum,
                    pre_owner_program,
                    post_owner_program,
                    datum_changed: pre_datum != post_datum, // Eq on `Option` does, in fact, check Eq on inner values!
                    was_created,
                    was_closed,
                }
            });

        iter
    }

    #[inline]
    fn get_accounts_typed<'a, T>(&'a self) -> Result<Vec<AccountTypedPrePost<'a, T>>>
    where
        T: Discriminatable + 'a,
        T: RefFromByteRef,
    {
        make_accounts_typed(self.iter_accounts())
    }

    #[inline]
    fn get_accounts_typed_borsh<'a, T>(&'a self) -> Result<Vec<AccountTypedPrePost<'a, AsBorsh<T>>>>
    where
        T: Discriminatable + 'a,
        T: BorshDeserialize,
    {
        self.get_accounts_typed::<AsBorsh<T>>()
    }

    #[inline]
    #[allow(clippy::type_complexity)]
    fn get_accounts_typed_2<'a, T1, T2>(
        &'a self,
    ) -> Result<(
        Vec<AccountTypedPrePost<'a, T1>>,
        Vec<AccountTypedPrePost<'a, T2>>,
    )>
    where
        T1: Discriminatable + 'a,
        T1: RefFromByteRef,
        T2: Discriminatable + 'a,
        T2: RefFromByteRef,
    {
        make_accounts_typed_2(self.iter_accounts())
    }

    #[inline]
    #[allow(clippy::type_complexity)]
    fn get_accounts_typed_borsh_2<'a, T1, T2>(
        &'a self,
    ) -> Result<(
        Vec<AccountTypedPrePost<'a, AsBorsh<T1>>>,
        Vec<AccountTypedPrePost<'a, AsBorsh<T2>>>,
    )>
    where
        T1: Discriminatable + 'a,
        T1: BorshDeserialize,
        T2: Discriminatable + 'a,
        T2: BorshDeserialize,
    {
        self.get_accounts_typed_2::<AsBorsh<T1>, AsBorsh<T2>>()
    }

    #[inline]
    #[allow(clippy::type_complexity)]
    fn get_accounts_typed_3<'a, T1, T2, T3>(
        &'a self,
    ) -> Result<(
        Vec<AccountTypedPrePost<'a, T1>>,
        Vec<AccountTypedPrePost<'a, T2>>,
        Vec<AccountTypedPrePost<'a, T3>>,
    )>
    where
        T1: Discriminatable + 'a,
        T1: RefFromByteRef,
        T2: Discriminatable + 'a,
        T2: RefFromByteRef,
        T3: Discriminatable + 'a,
        T3: RefFromByteRef,
    {
        make_accounts_typed_3(self.iter_accounts())
    }

    #[inline]
    #[allow(clippy::type_complexity)]
    fn get_accounts_typed_borsh_3<'a, T1, T2, T3>(
        &'a self,
    ) -> Result<(
        Vec<AccountTypedPrePost<'a, AsBorsh<T1>>>,
        Vec<AccountTypedPrePost<'a, AsBorsh<T2>>>,
        Vec<AccountTypedPrePost<'a, AsBorsh<T3>>>,
    )>
    where
        T1: Discriminatable + 'a,
        T1: BorshDeserialize,
        T2: Discriminatable + 'a,
        T2: BorshDeserialize,
        T3: Discriminatable + 'a,
        T3: BorshDeserialize,
    {
        self.get_accounts_typed_3::<AsBorsh<T1>, AsBorsh<T2>, AsBorsh<T3>>()
    }

    fn get_program_events_from_logs(&self, program_id: &str) -> Vec<Vec<u8>> {
        let logs = self.get_logs();
        let OptionSerializer::Some(logs) = logs else {
            return vec![];
        };

        let mut program_stack: SimpleStack<&str> = SimpleStack::new();
        let mut events: Vec<Vec<u8>> = Vec::new();

        for log in logs {
            let handle_log_data = handle_log(&program_stack, log, program_id);
            if let Some(event) = handle_log_data.event {
                events.push(event);
            }
            if let Some(new_program) = handle_log_data.new_program {
                program_stack.push(new_program);
            }
            if handle_log_data.did_pop {
                program_stack.pop();
            }
        }

        events
    }

    fn get_grouped_instructions<F>(&self, predicate: F) -> Result<Vec<GroupedInstruction>>
    where
        F: Fn(&FlattenedIxWithIndex) -> bool,
    {
        let mut result: Vec<GroupedInstruction> = vec![];
        for (idx, ix) in self.get_flattened_ixs().enumerate() {
            if !predicate(&ix) {
                continue;
            }

            let mut matching_ixs: Vec<FlattenedIxWithIndex> = vec![];
            if ix.is_inner {
                let this_stack_height = ix
                    .stack_height
                    .ok_or_else(|| anyhow!("No stack height present on inner ix???"))?;
                for future_inner_ix in self.get_flattened_ixs().skip(idx + 1) {
                    let stack_height_cmp = future_inner_ix
                        .stack_height
                        .ok_or_else(|| anyhow!("No stack height present on inner ix???"))?;
                    if stack_height_cmp <= this_stack_height {
                        break;
                    }
                    matching_ixs.push(future_inner_ix.clone());
                }
            } else {
                matching_ixs = self
                    .get_flattened_ixs()
                    .filter(|v| v.index == idx as u8 && v.is_inner)
                    .collect();
            }

            result.push(GroupedInstruction {
                involved_instructions: matching_ixs,
                parent_instruction: ix,
            });
        }

        Ok(result)
    }

    fn get_total_transfer_amounts_for_pubkey_flattened_ixs<'a>(
        &'a self,
        ixs: impl Iterator<Item = &'a FlattenedIxWithIndex<'a>>,
        authority_keys: &'a [&'a str],
        in_mints: &[&str],
        out_mints: &[&str],
    ) -> Result<AggregatedTransferAmounts> {
        let ixs_iter = ixs.map(|f| f.instruction);
        self.get_total_transfer_amounts_for_pubkey_ui_ixs(
            ixs_iter,
            authority_keys,
            in_mints,
            out_mints,
        )
    }

    fn get_total_transfer_amounts_for_pubkey_ui_ixs<'a>(
        &'a self,
        ixs: impl Iterator<Item = &'a UiInstruction>,
        authority_keys: &'a [&'a str],
        in_mints: &'a [&'a str],
        out_mints: &'a [&'a str],
    ) -> Result<AggregatedTransferAmounts> {
        let mut result = HashMap::new();

        for ix in ixs {
            let Some(parsed) = get_parsed_variant(ix) else {
                continue;
            };
            if parsed.program_id != TOKEN_PROGRAM_ID && parsed.program_id != TOKEN_2022_PROGRAM_ID {
                continue;
            }

            // If something makes one of these guaranteed, please change to a variable declaration instead of an Option
            let mut cmp_mint: Option<String> = None;
            let mut source: Option<String> = None;
            let source_authority: String;
            let mut destination: Option<String> = None;
            let mut destination_authority: Option<String> = None;
            let transfer_amount: u64;

            // Grab max amount of information we can just from the JSON
            match serde_json::from_value::<TokenParsedInstruction>(parsed.parsed.clone())? {
                TokenParsedInstruction::TokenBurn(ix) => {
                    source_authority = ix.get_authority()?.to_string();
                    source = Some(ix.account);
                    cmp_mint = Some(ix.mint);
                    transfer_amount = ix.amount;
                }
                TokenParsedInstruction::TokenBurnChecked(ix) => {
                    source_authority = ix.get_authority()?.to_string();
                    source = Some(ix.account);
                    cmp_mint = Some(ix.mint);
                    transfer_amount = ix.token_amount.amount.parse()?;
                }
                TokenParsedInstruction::TokenMintTo(ix) => {
                    source_authority = ix.get_authority()?.to_string();
                    cmp_mint = Some(ix.mint);
                    destination = Some(ix.account);
                    transfer_amount = ix.amount;
                }
                TokenParsedInstruction::TokenMintToChecked(ix) => {
                    source_authority = ix.get_authority()?.to_string();
                    cmp_mint = Some(ix.mint);
                    destination = Some(ix.account);
                    transfer_amount = ix.token_amount.amount.parse()?;
                }
                TokenParsedInstruction::TokenTransfer(ix) => {
                    source_authority = ix.get_authority()?.to_string();
                    source = Some(ix.source);
                    destination = Some(ix.destination);
                    transfer_amount = ix.amount;
                }
                TokenParsedInstruction::TokenTransferChecked(ix) => {
                    source_authority = ix.get_authority()?.to_string();
                    source = Some(ix.source);
                    destination = Some(ix.destination);
                    transfer_amount = ix.token_amount.amount.parse()?;
                }
                _ => {
                    continue;
                }
            }

            // Try to fill in missing info
            if let Some((_, mint)) =
                self.get_missing_ata_info(source, Some(source_authority.clone()), cmp_mint.clone())?
            {
                cmp_mint = Some(mint);
            }

            if authority_keys.contains(&source_authority.as_str()) {
                if let Some(ref cmp_mint) = cmp_mint.clone() {
                    let auth_entry = result.entry(source_authority).or_insert_with(HashMap::new);
                    let txfer_entry = auth_entry.entry(cmp_mint.clone()).or_insert_with(|| {
                        DirectionalTransferAmounts {
                            in_amount: 0,
                            out_amount: 0,
                        }
                    });
                    if in_mints.contains(&cmp_mint.as_str()) {
                        txfer_entry.in_amount += transfer_amount as i128;
                    }
                    if out_mints.contains(&cmp_mint.as_str()) {
                        txfer_entry.out_amount -= transfer_amount as i128;
                    }
                }
            }

            if let Some((auth, mint)) = self.get_missing_ata_info(
                destination,
                destination_authority.clone(),
                cmp_mint.clone(),
            )? {
                destination_authority = Some(auth);
                cmp_mint = Some(mint);
            }

            if let Some(authority) = destination_authority {
                if authority_keys.contains(&authority.as_str()) {
                    if let Some(cmp_mint) = cmp_mint {
                        let auth_entry = result.entry(authority).or_insert_with(HashMap::new);
                        let txfer_entry = auth_entry.entry(cmp_mint.clone()).or_insert_with(|| {
                            DirectionalTransferAmounts {
                                in_amount: 0,
                                out_amount: 0,
                            }
                        });
                        if in_mints.contains(&cmp_mint.as_str()) {
                            txfer_entry.in_amount -= transfer_amount as i128;
                        }
                        if out_mints.contains(&cmp_mint.as_str()) {
                            txfer_entry.out_amount += transfer_amount as i128;
                        }
                    }
                }
            }
        }

        Ok(AggregatedTransferAmounts::new(result))
    }

    /// see `get_total_transfer_amounts_for_pubkey`
    ///
    /// returns `(acct_authority: String, cmp_mint: String)`
    fn get_missing_ata_info(
        &self,
        token_account_key: Option<String>,
        authority: Option<String>,
        cmp_mint: Option<String>,
    ) -> Result<Option<(String, String)>> {
        match (token_account_key, authority, cmp_mint) {
            (_, Some(auth), Some(mint)) => {
                // We have auth and mint already
                Ok(Some((auth, mint)))
            }
            (Some(acct), auth, cmp_mint) => {
                // Possibly no auth or no mint but we have the account key, so go find the info
                let acct_data = self.get_token_account_data_by_key(&acct)?;

                let auth = auth.unwrap_or(
                    acct_data
                        .owner
                        .ok_or_else(|| anyhow!("Missing owner for token account {acct} in tx"))?,
                );

                let cmp_mint = cmp_mint.unwrap_or(acct_data.mint);
                Ok(Some((auth, cmp_mint)))
            }
            (None, _, _) => {
                // No account key and at least one of the other values is None, so we're screwed.
                Ok(None)
            }
        }
    }

    /// Returns (pre_balance, post_balance) for a given account key, if found
    fn get_pre_and_post_token_balance<'a>(
        &'a self,
        account: &str,
    ) -> Result<(
        Option<&'a UiTransactionTokenBalance>,
        Option<&'a UiTransactionTokenBalance>,
    )> {
        let acct_idx = self
            .get_account_keys()
            .iter()
            .position(|a| a.pubkey == *account)
            .context("Account not found in account keys")?;

        let pre_token_balances = self.get_pre_token_balances().as_ref().into_option();
        let pre_balance =
            pre_token_balances.and_then(|v| v.iter().find(|a| a.account_index == acct_idx as u8));

        let post_token_balances = self.get_post_token_balances().as_ref().into_option();
        let post_balance =
            post_token_balances.and_then(|v| v.iter().find(|a| a.account_index == acct_idx as u8));

        Ok((pre_balance, post_balance))
    }

    fn get_account_owner(&self, account: &str) -> Option<&String> {
        let owners = self.get_post_owners().as_ref()?;
        self.get_account_keys()
            .iter()
            .position(|a| a.pubkey == account)
            .and_then(|idx| owners.get(idx))
            .and_then(|opt| opt.as_ref())
    }
    fn get_position_nft_owner(&self, position_mint: &str) -> Option<&String> {
        // First check post_balances
        if let OptionSerializer::Some(post_balances) = self.get_post_token_balances() {
            if let Some(balance) = post_balances
                .iter()
                .find(|balance| balance.mint == position_mint)
            {
                if balance.ui_token_amount.amount == "0" {
                    return None;
                }
                if let OptionSerializer::Some(owner) = &balance.owner {
                    return Some(owner);
                }
            }
        }

        // If nothing found in post_balances, try pre_balances
        if let OptionSerializer::Some(pre_balances) = self.get_pre_token_balances() {
            if let Some(balance) = pre_balances
                .iter()
                .find(|balance| balance.mint == position_mint)
            {
                if balance.ui_token_amount.amount == "0" {
                    return None;
                }
                if let OptionSerializer::Some(owner) = &balance.owner {
                    return Some(owner);
                }
            }
        }

        None
    }

    fn get_token_supply(&self, mint: &str) -> Option<u64> {
        let data = self.get_account_data_after(mint).ok()??;

        data.get(36..44)
            .map(|b| b.try_into())
            .transpose()
            .ok()?
            .map(u64::from_le_bytes)
    }
}

impl FlattenedIxWithIndex<'_> {
    pub fn get_ix_program_id<'a>(&'a self, parent_tx: &'a impl TransactionLike) -> &'a String {
        match self.instruction {
            UiInstruction::Parsed(UiParsedInstruction::Parsed(parsed)) => &parsed.program_id,
            UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(part_deco)) => {
                &part_deco.program_id
            }
            UiInstruction::Compiled(ref ix) => {
                &parent_tx.get_account_keys()[ix.program_id_index as usize].pubkey
            }
        }
    }
    pub fn get_ix_parent_program_id<'a>(
        &'a self,
        parent_tx: &'a impl TransactionLike,
    ) -> Option<&'a String> {
        if self.is_inner {
            let parent_ix = &parent_tx.get_outer_instructions()[self.index as usize];
            match parent_ix {
                UiInstruction::Parsed(UiParsedInstruction::Parsed(parsed)) => {
                    Some(&parsed.program_id)
                }
                UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(part_deco)) => {
                    Some(&part_deco.program_id)
                }
                UiInstruction::Compiled(compiled) => {
                    Some(&parent_tx.get_account_keys()[compiled.program_id_index as usize].pubkey)
                }
            }
        } else {
            None
        }
    }
    pub fn get_parsed_system_program_instruction(
        &self,
    ) -> Result<Option<crate::utils::sys_ixs::SystemProgramParsedInstruction>> {
        crate::utils::sys_ixs::get_parsed_system_program_instruction(self.instruction)
    }
    pub fn get_parsed_token_program_instruction(
        &self,
    ) -> Result<Option<crate::utils::spl_ixs::TokenParsedInstruction>> {
        crate::utils::spl_ixs::get_parsed_token_program_instruction(self.instruction)
    }
    pub fn get_parsed_stake_program_instruction(
        &self,
    ) -> Result<Option<crate::utils::stake_ixs::StakeProgramParsedInstruction>> {
        crate::utils::stake_ixs::get_parsed_stake_program_instruction(self.instruction)
    }
}

// EVENT LOG HANDLING
// Ripped from https://github.com/coral-xyz/anchor/blob/v0.29.0/ts/packages/anchor/src/program/event.ts#L183

const PROGRAM_LOG: &str = "Program log: ";
const PROGRAM_DATA: &str = "Program data: ";
const PROGRAM_LOG_START_INDEX: usize = PROGRAM_LOG.len();
const PROGRAM_DATA_START_INDEX: usize = PROGRAM_DATA.len();

struct HandleLogData<'a> {
    event: Option<Vec<u8>>,
    new_program: Option<&'a str>,
    did_pop: bool,
}

#[allow(clippy::unwrap_used)]
fn handle_log<'a>(stack: &SimpleStack<&str>, log: &str, program_id: &'a str) -> HandleLogData<'a> {
    if stack.length() > 0 && stack.current().unwrap() == &program_id {
        handle_program_log(log, program_id)
    } else {
        handle_system_log(log, program_id)
    }
}

fn handle_program_log<'a>(log: &str, program_id: &'a str) -> HandleLogData<'a> {
    let log_str;
    if log.starts_with(PROGRAM_LOG) {
        log_str = &log[PROGRAM_LOG_START_INDEX..];
    } else if log.starts_with(PROGRAM_DATA) {
        log_str = &log[PROGRAM_DATA_START_INDEX..];
    } else {
        return handle_system_log(log, program_id); // handle_system_log stuff
    }

    let event = BASE64_STANDARD.decode(log_str).ok();

    HandleLogData {
        event,
        new_program: None,
        did_pop: false,
    }
}

#[allow(clippy::unwrap_used)]
fn handle_system_log<'a>(log: &str, program_id: &'a str) -> HandleLogData<'a> {
    let log_start = log.split(':').next().unwrap();
    let success_regex = Regex::new(r"Program (.*) success").unwrap();

    if success_regex.is_match(log_start) {
        HandleLogData {
            event: None,
            new_program: None,
            did_pop: true,
        }
    } else if log_start.starts_with(&format!("Program {program_id} invoke")) {
        HandleLogData {
            event: None,
            new_program: Some(program_id),
            did_pop: false,
        }
    } else if log_start.contains("invoke") {
        HandleLogData {
            event: None,
            new_program: Some("cpi"), // Any string, doesn't matter
            did_pop: false,
        }
    } else {
        HandleLogData {
            event: None,
            new_program: None,
            did_pop: false,
        }
    }
}
