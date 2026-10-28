use std::sync::mpsc::SyncSender;

use anyhow::Result;
use async_trait::async_trait;

use step_ingestooor_idl_gens::openbook_v2_gen::openbook_v2::accounts::{Market, OpenOrdersAccount};
use step_ingestooor_sdk::{
    dooot::Dooot, trait_defs::transaction::TransactionLike, utils::step_utils::StepUtils,
};

use crate::{
    state_source::{AccountProvider, StateSource},
    TransactionParserFilter,
};

use super::bookside::BookSide;

#[allow(clippy::large_enum_variant)]
pub enum OpenbookAccounts {
    BookSide(Box<BookSide>),
    Market(Market),
    OpenOrdersAccount(OpenOrdersAccount),
}

const _DISC_MARKET: [u8; 8] = [0xdb, 0xbe, 0xd5, 0x37, 0x00, 0xe3, 0xc6, 0x9a];
const _DISC_BOOKSIDE: [u8; 8] = [72, 44, 225, 141, 178, 130, 97, 57];
const _DISC_OPEN_ORDERS: [u8; 8] = [255, 194, 78, 123, 16, 105, 208, 165];

pub const OPENBOOK_V2_PROGRAM: &str = "opnb2LAfJYbRMAHHvqjCwQxanZn7ReEHp1k81EohpZb";

pub struct OpenbookV2;

#[async_trait]
impl crate::TransactionParserTrait for OpenbookV2 {
    fn get_name(&self) -> &'static str {
        "OpenbookV2"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![crate::TransactionParserFilter::ProgramId(
            OPENBOOK_V2_PROGRAM.to_string(),
        )]
    }

    async fn invoke(
        &self,
        _decoded_transaction: &impl TransactionLike,
        _dooot: &mut Vec<step_ingestooor_sdk::dooot::Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        // // Using a tx wide cache to store specific accounts so we don't process more than once
        // let mut processed_cache: HashMap<String, bool> = HashMap::new();
        // let time = decoded_transaction.get_block_time();

        // let fills = decoded_transaction
        //     .get_program_events_from_logs(OPENBOOK_V2_PROGRAM)
        //     .iter()
        //     .filter_map(|e| {
        //         Event::try_from_slice(e).ok().and_then(|eu| match eu {
        //             Event::FillLog(fill_log) => Some(fill_log),
        //             _ => None,
        //         })
        //     })
        //     .collect::<Vec<FillLog>>();

        // for fill in &fills {
        //     let market_pubkey = fill.market.to_string();
        //     let market: Market = decoded_transaction.get_account_data_parsed_borsh(
        //         &market_pubkey,
        //         "Market",
        //         "FillLog",
        //         TxAccountInfoStage::Post,
        //         None,
        //     )?;

        //     let price = Decimal::from(fill.price);
        //     let converted_quantity = fill
        //         .quantity
        //         .checked_mul(market.base_lot_size)
        //         .ok_or_else(|| anyhow!("Overflow during base_qty calculation"))?;
        //     let buy_side = fill.taker_side == 1;
        //     let maker_fee = fill.maker_fee as f64 / 100000.0;
        //     let taker_fee = fill.taker_fee_ceil as f64 / 100000.0;

        //     let clob = ClobFillEventDooot {
        //         price,
        //         base_qty: converted_quantity as u64,
        //         maker_fee,
        //         referral_fee: 0.0,
        //         taker_fee,
        //         base_mint_pubkey: market.base_mint.to_string(),
        //         maker_pubkey: fill.maker.to_string(),
        //         program_id: OPENBOOK_V2_PROGRAM.to_string(),
        //         buy_side,
        //         market_pubkey,
        //         quote_mint_pubkey: market.quote_mint.to_string(),
        //         taker_pubkey: fill.taker.to_string(),
        //         time,
        //         referrer_pubkey: None,
        //     };
        //     dooot.push(Dooot::ClobFillEvent(clob));
        // }

        // for ix in decoded_transaction.get_flattened_ixs() {
        //     let Some(part_dec) = get_partially_decoded_variant(ix.instruction) else {
        //         continue;
        //     };

        //     if part_dec.program_id != OPENBOOK_V2_PROGRAM {
        //         continue;
        //     }

        //     let ix_bytes = bs58::decode(&part_dec.data).into_vec()?;
        //     let ix_decoded = match InstructionUnion::try_from_slice(&ix_bytes) {
        //         Ok(union) => union,
        //         Err(_) => {
        //             log::warn!(
        //                 "Unknown ix variant for Openbook v2. Bytes: {:?} Sig: {}",
        //                 &ix_bytes[..8],
        //                 decoded_transaction.get_signature(),
        //             );
        //             continue;
        //         }
        //     };

        //     // Handle IX's that don't have a market
        //     match ix_decoded {
        //         InstructionUnion::CloseOpenOrdersAccount(_) => {
        //             let oo_account = &part_dec.accounts[2];
        //             let key = self.get_cache_key(oo_account);
        //             self.delete_from_cache(ts_cache, &key).await?;
        //             continue;
        //         }
        //         InstructionUnion::CreateOpenOrdersIndexer(_) => continue,
        //         InstructionUnion::CloseOpenOrdersIndexer(_) => continue,
        //         InstructionUnion::SweepFees(_) => continue,
        //         InstructionUnion::SetMarketExpired(_) => continue,
        //         InstructionUnion::CloseMarket(_) => continue,
        //         InstructionUnion::StubOracleCreate(_) => continue,
        //         InstructionUnion::StubOracleClose(_) => continue,
        //         InstructionUnion::StubOracleSet(_) => continue,
        //         InstructionUnion::SetDelegate(_) => continue,
        //         _ => (),
        //     }

        //     // if we made it here this is a openbook ix. Now we need to find the relevant accounts
        //     // this will get simplified later when we add owners to these accounts
        //     let transaction_accounts = decoded_transaction.get_account_keys();
        //     let pre_datum = decoded_transaction
        //         .get_pre_datums()
        //         .as_ref()
        //         .ok_or_else(|| anyhow!("Failed to get pre datums"))?;
        //     let post_datum = decoded_transaction
        //         .get_post_datums()
        //         .as_ref()
        //         .ok_or_else(|| anyhow!("Failed to get post datums"))?;

        //     let ix_accounts = &part_dec.accounts;

        //     let mut market: Option<(&String, Market)> = None;
        //     // idk what side of the book it is so using a vec
        //     let mut pre_books: Vec<(&String, Box<BookSide>)> = Vec::with_capacity(2);
        //     let mut pre_user_accounts: Vec<(&String, OpenOrdersAccount)> = Vec::new();
        //     let mut post_books: Vec<(&String, Box<BookSide>)> = Vec::with_capacity(2);
        //     let mut post_user_accounts: Vec<(&String, OpenOrdersAccount)> = Vec::new();

        //     for account in ix_accounts.iter() {
        //         let tx_index = transaction_accounts
        //             .iter()
        //             .position(|a| a.pubkey == *account)
        //             .ok_or_else(|| anyhow!("Failed to get get tx_index"))?;
        //         let pre_data = &pre_datum[tx_index];
        //         let post_data = &post_datum[tx_index];
        //         let pre_datum = handle_data_parse(pre_data)?;
        //         let post_datum = handle_data_parse(post_data)?;
        //         if let Some((pre_disc, pre_datum)) = pre_datum {
        //             let ob_account = handle_disc_parse(pre_disc, &mut pre_datum.as_slice())?;
        //             if let Some(ob_account) = ob_account {
        //                 match ob_account {
        //                     OpenbookAccounts::BookSide(acc) => {
        //                         pre_books.push((account, acc));
        //                     }
        //                     OpenbookAccounts::OpenOrdersAccount(acc) => {
        //                         pre_user_accounts.push((account, acc));
        //                     }
        //                     // skipping market in pre datums
        //                     _ => (),
        //                 }
        //             }
        //         };
        //         if let Some((post_disc, post_datum)) = post_datum {
        //             let ob_account = handle_disc_parse(post_disc, &mut post_datum.as_slice())?;
        //             if let Some(ob_account) = ob_account {
        //                 match ob_account {
        //                     OpenbookAccounts::Market(acc) => {
        //                         market = Some((account, acc));
        //                     }
        //                     OpenbookAccounts::BookSide(acc) => {
        //                         post_books.push((account, acc));
        //                     }
        //                     OpenbookAccounts::OpenOrdersAccount(acc) => {
        //                         post_user_accounts.push((account, acc));
        //                     }
        //                 }
        //             }
        //         };
        //     }

        //     let Some((market_pubkey, market)) = market else {
        //         // market exists on every ix we care about so error if we didn't find it
        //         return Err(anyhow!("Expected a market here!"));
        //     };

        //     let bids_pubkey = market.bids.to_string();
        //     let base_mint = market.base_mint.to_string();
        //     let quote_mint = market.quote_mint.to_string();
        //     let base_decimal = market.base_decimals;
        //     let quote_decimal = market.quote_decimals;
        //     let big_base_decimal = Decimal::new(10, 0).powu(base_decimal as u64);
        //     let big_quote_decimal = Decimal::new(10, 0).powu(quote_decimal as u64);
        //     let now_ts = decoded_transaction.get_block_time().and_utc().timestamp() as u64;
        //     // setting these keys in ts_cache early so we are certain they exist later
        //     // only setting post as pre accounts should already be in ts_cache
        //     for oo in post_user_accounts.iter() {
        //         let key = self.get_cache_key(oo.0);
        //         let owner = oo.1.owner.to_string();
        //         // setting into ts_cache. Could exist already. Is just only writing better than checking
        //         // existance and writing?
        //         ts_cache.basic_set(key, &owner, time).await?;
        //         let base_amount: Decimal = oo.1.position.base_free_native.into();
        //         let wc_base = ClaimableBalanceUserDooot {
        //             time: decoded_transaction.get_block_time(),
        //             owner: owner.clone(),
        //             program: OPENBOOK_V2_PROGRAM.to_string(),
        //             mint: base_mint.clone(),
        //             amount: base_amount / big_base_decimal,
        //             relates_to: market_pubkey.to_string(),
        //             relates_to_type: "m".to_string(),
        //         };
        //         self.submit_wallet_claimable(dooot, oo.0.clone(), wc_base, &mut processed_cache)?;
        //         let quote_amount: Decimal = oo.1.position.quote_free_native.into();
        //         let wc_quote = ClaimableBalanceUserDooot {
        //             time: decoded_transaction.get_block_time(),
        //             owner: owner.clone(),
        //             program: OPENBOOK_V2_PROGRAM.to_string(),
        //             mint: quote_mint.clone(),
        //             amount: quote_amount / big_quote_decimal,
        //             relates_to: market_pubkey.to_string(),
        //             relates_to_type: "m".to_string(),
        //         };
        //         self.submit_wallet_claimable(dooot, oo.0.clone(), wc_quote, &mut processed_cache)?;
        //     }
        //     if pre_books.len() < 2 || post_books.len() < 2 {
        //         // likely was just a consume event here or a settle funds. So all we needed to do
        //         // was submit a WalletClaimableBalance
        //         // debug!("Less than 2 pre/post books here {:#?}", ix_decoded);
        //         continue;
        //     }
        //     //They should be matching but just in case!
        //     if pre_books[0].0 != post_books[0].0 || pre_books[1].0 != post_books[1].0 {
        //         return Err(anyhow!("Pre and Post BookSides did not match!"));
        //     }
        //     let is_buy = *post_books[0].0 == bids_pubkey;
        //     // we only want to check the invalids on the pre books
        //     let pre_one: Vec<BookSideIterItem> = pre_books[0]
        //         .1
        //         .iter_all_including_invalid(now_ts, None)?
        //         .collect();
        //     let post_one: Vec<BookSideIterItem> =
        //         post_books[0].1.iter_valid(now_ts, None)?.collect();

        //     // we only want to check the invalids on the pre books
        //     let pre_two: Vec<BookSideIterItem> = pre_books[1]
        //         .1
        //         .iter_all_including_invalid(now_ts, None)?
        //         .collect();
        //     let post_two: Vec<BookSideIterItem> =
        //         post_books[1].1.iter_valid(now_ts, None)?.collect();

        //     self.compare_order_books(
        //         ts_cache,
        //         &mut processed_cache,
        //         decoded_transaction,
        //         dooot,
        //         &market,
        //         market_pubkey,
        //         &pre_one,
        //         &post_one,
        //         is_buy,
        //     )
        //     .await?;
        //     self.compare_order_books(
        //         ts_cache,
        //         &mut processed_cache,
        //         decoded_transaction,
        //         dooot,
        //         &market,
        //         market_pubkey,
        //         &pre_two,
        //         &post_two,
        //         !is_buy,
        //     )
        //     .await?;
        // }

        Ok(())
    }
}

// fn handle_disc_parse(disc: Vec<u8>, data: &mut &[u8]) -> Result<Option<OpenbookAccounts>> {
//     if disc == DISC_BOOKSIDE {
//         let bookside: Box<BookSide> = zerocopy_data::<BookSide>(data.to_vec(), Some(0))?;
//         let bookside_wrap = OpenbookAccounts::BookSide(bookside);
//         return Ok(Some(bookside_wrap));
//     } else if disc == DISC_MARKET {
//         let market = Market::deserialize(data)?;
//         let market_wrap = OpenbookAccounts::Market(market);
//         return Ok(Some(market_wrap));
//     } else if disc == DISC_OPEN_ORDERS {
//         let user_acc = OpenOrdersAccount::deserialize(data)?;
//         let user_wrap = OpenbookAccounts::OpenOrdersAccount(user_acc);
//         return Ok(Some(user_wrap));
//     }
//     Ok(None)
// }

// fn handle_data_parse(datum: &Option<String>) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
//     let Some(datum) = datum else {
//         return Ok(None);
//     };
//     if datum.is_empty() {
//         return Ok(None);
//     };

//     let bytes = decode_base64_compressed_data(datum.as_str())?;
//     //sanity
//     if bytes.len() < 8 {
//         return Ok(None);
//     }
//     let (disc, data) = bytes.split_at(8);
//     Ok(Some((disc.to_vec(), data.to_vec())))
// }

impl OpenbookV2 {
    // pub fn get_cache_key(&self, oo_acc: &String) -> String {
    //     format!("ingestooor:openbook_v2:oo_owners:{}", oo_acc)
    // }

    // async fn read_from_cache(
    //     &self,
    //     ts_cache: &impl TimeSeriesCacheLike,
    //     oo_acc: &String,
    // ) -> Result<String> {
    //     let key = self.get_cache_key(oo_acc);
    //     let oo_acc: Option<String> = ts_cache.get(&key, None, None).await?;
    //     let Some(oo_acc) = oo_acc else {
    //         return Err(anyhow!(format!(
    //             "Expected open order account to already be cached {}",
    //             &key
    //         )));
    //     };
    //     Ok(oo_acc)
    // }

    // async fn delete_from_cache(
    //     &self,
    //     ts_cache: &impl TimeSeriesCacheLike,
    //     oo_acc: &String,
    // ) -> Result<()> {
    //     let key = self.get_cache_key(oo_acc);
    //     ts_cache.del(key).await?;
    //     Ok(())
    // }

    // // Handles diffs between pre- and post-data to determine what user dooots need to be published
    // #[allow(clippy::too_many_arguments)]
    // async fn compare_order_books<'a>(
    //     &self,
    //     ts_cache: &impl TimeSeriesCacheLike,
    //     processed_cached: &mut HashMap<String, bool>,
    //     decoded_transaction: &impl TransactionLike,
    //     dooot: &mut Vec<Dooot>,
    //     market: &Market,
    //     market_pubkey: &String,
    //     before: &[BookSideIterItem<'a>],
    //     after: &[BookSideIterItem<'a>],
    //     is_buy: bool,
    // ) -> Result<()> {
    //     let before_map: HashMap<u128, &LeafNode> =
    //         before.iter().map(|o| (o.node.key, o.node)).collect();
    //     let after_map: HashMap<u128, &LeafNode> =
    //         after.iter().map(|o| (o.node.key, o.node)).collect();
    //     let time = decoded_transaction.get_block_time();

    //     let mut all_ids: HashMap<u128, bool> = HashMap::new();

    //     // add all id's from pre & post to all map
    //     for id in before_map.keys() {
    //         all_ids.insert(*id, true);
    //     }
    //     for id in after_map.keys() {
    //         all_ids.insert(*id, true);
    //     }

    //     // Iterate over all IDs and compare orders
    //     for id in all_ids.keys() {
    //         match (before_map.get(id), after_map.get(id)) {
    //             // exists in both
    //             (Some(before_order), Some(after_order)) => {
    //                 if before_order != after_order {
    //                     // After order was modified.
    //                     // read owner from ts_cache it should already exist
    //                     let oo_acc = after_order.owner.to_string();
    //                     let owner = self.read_from_cache(ts_cache, &oo_acc).await?;
    //                     let (price, quantity) =
    //                         self.calculate_price_and_quantity(after_order, market)?;
    //                     let key = after_order.key;
    //                     let oo = ClobUserDooot {
    //                         time,
    //                         program: OPENBOOK_V2_PROGRAM.to_string(),
    //                         owner,
    //                         account: after_order.owner.to_string(),
    //                         market: market_pubkey.to_string(),
    //                         is_buy,
    //                         order_id: key.to_string(),
    //                         price,
    //                         unfilled_qty: quantity,
    //                         base_mint: market.base_mint.to_string(),
    //                         quote_mint: market.quote_mint.to_string(),
    //                         expiry_ts: None,
    //                         closed: false,
    //                     };
    //                     self.submit_user_order(dooot, oo, processed_cached)?;
    //                 }
    //             }
    //             // only existed in pre
    //             (Some(before_order), None) => {
    //                 // TODO maybe not delete this from ts_cache unless the account is closed
    //                 let oo_acc = before_order.owner.to_string();
    //                 let owner = self.read_from_cache(ts_cache, &oo_acc).await?;
    //                 let (price, _) = self.calculate_price_and_quantity(before_order, market)?;
    //                 let key = before_order.key;
    //                 let oo = ClobUserDooot {
    //                     time,
    //                     program: OPENBOOK_V2_PROGRAM.to_string(),
    //                     owner,
    //                     account: before_order.owner.to_string(),
    //                     market: market_pubkey.to_string(),
    //                     is_buy,
    //                     order_id: key.to_string(),
    //                     price,
    //                     unfilled_qty: 0,
    //                     base_mint: market.base_mint.to_string(),
    //                     quote_mint: market.quote_mint.to_string(),
    //                     expiry_ts: None,
    //                     closed: true,
    //                 };
    //                 self.submit_user_order(dooot, oo, processed_cached)?;
    //             }
    //             // only existed in post
    //             (None, Some(after_order)) => {
    //                 let oo_acc = after_order.owner.to_string();
    //                 // it's fine to read here. Should already be added from above
    //                 // may be a redudant read, but it's prob fine
    //                 let owner = self.read_from_cache(ts_cache, &oo_acc).await?;
    //                 let (price, quantity) =
    //                     self.calculate_price_and_quantity(after_order, market)?;
    //                 let key = after_order.key;
    //                 let oo = ClobUserDooot {
    //                     time,
    //                     program: OPENBOOK_V2_PROGRAM.to_string(),
    //                     owner,
    //                     account: after_order.owner.to_string(),
    //                     market: market_pubkey.to_string(),
    //                     is_buy,
    //                     order_id: key.to_string(),
    //                     price,
    //                     unfilled_qty: quantity,
    //                     base_mint: market.base_mint.to_string(),
    //                     quote_mint: market.quote_mint.to_string(),
    //                     expiry_ts: None,
    //                     closed: false,
    //                 };
    //                 self.submit_user_order(dooot, oo, processed_cached)?;
    //             }
    //             //??
    //             (None, None) => {
    //                 // This case should not occur?
    //                 debug!("the case that shouldn't occur occured!")
    //             }
    //         }
    //     }
    //     Ok(())
    // }

    // pub fn calculate_price_and_quantity(
    //     &self,
    //     leaf: &LeafNode,
    //     market: &Market,
    // ) -> Result<(Decimal, u64)> {
    //     let quantity = leaf.quantity as u64;

    //     let price = Decimal::from(leaf.price_data());

    //     let base_decimals_big = 10u64.pow(market.base_decimals as u32);
    //     let qty = quantity
    //         .checked_mul(market.base_lot_size as u64)
    //         .ok_or_else(|| anyhow!("overflow in multiplication"))?
    //         .checked_div(base_decimals_big)
    //         .ok_or_else(|| anyhow!("overflow in division"))?;
    //     Ok((price, qty))
    // }

    // pub fn submit_user_order(
    //     &self,
    //     dooot: &mut Vec<Dooot>,
    //     open_order: ClobUserDooot,
    //     cache: &mut HashMap<String, bool>,
    // ) -> Result<()> {
    //     let processed_key = format!(
    //         "{}-{}-{}-{}",
    //         &open_order.account, open_order.owner, open_order.order_id, &open_order.is_buy,
    //     );
    //     if cache.contains_key(&processed_key) {
    //         return Ok(());
    //     }
    //     cache.insert(processed_key, true);
    //     dooot.push(Dooot::ClobUser(open_order));
    //     Ok(())
    // }

    // pub fn submit_wallet_claimable(
    //     &self,
    //     dooot: &mut Vec<Dooot>,
    //     oo_acc: String,
    //     claimbable: ClaimableBalanceUserDooot,
    //     cache: &mut HashMap<String, bool>,
    // ) -> Result<()> {
    //     let processed_key = format!("{}-{}", oo_acc, claimbable.mint);
    //     if cache.contains_key(&processed_key) {
    //         return Ok(());
    //     }
    //     cache.insert(processed_key, true);
    //     dooot.push(Dooot::ClaimableBalanceUser(claimbable));
    //     Ok(())
    // }
}

impl StateSource for OpenbookV2 {
    fn get_name(&self) -> &'static str {
        "OpenbookV2"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        _acct_provider: impl AccountProvider,
        _tx: SyncSender<Dooot>,
    ) -> Result<()> {
        // info!("begin get state for Openbook v2");

        // let program_id = Pubkey::from_str(OPENBOOK_V2_PROGRAM)?;

        // let all_accounts = acct_provider.program_accounts(program_id, None);
        // let block_time = acct_provider.get_block_time();
        // let time = convert_blocktime_to_micros(block_time, Some(0), None)?;
        // let now_ts = time.and_utc().timestamp() as u64;

        // let mut markets: HashMap<String, Market> = HashMap::new();
        // let mut book_sides: HashMap<String, Box<BookSide>> = HashMap::new();
        // // open_orders is market_string -> Hashmap<(OO String, OpenOrdersAccount)>
        // let mut open_orders: HashMap<String, HashMap<String, OpenOrdersAccount>> = HashMap::new();
        // let mut oo_to_owner_cache: HashMap<String, String> = HashMap::new();

        // for acc in all_accounts {
        //     let account_data = acc.account.data();
        //     if account_data.len() < 8 {
        //         continue;
        //     }
        //     let (acc_disc, mut data) = account_data.split_at(8);
        //     if acc_disc == DISC_MARKET {
        //         debug!("found openbook market account {}", acc.pubkey.to_string());
        //         let market: Market = Market::deserialize(&mut data)?;
        //         markets.insert(acc.pubkey.to_string(), market);
        //     } else if acc_disc == DISC_BOOKSIDE {
        //         debug!("found openbook BookSide account");
        //         let book_side: Box<BookSide> = zerocopy_data::<BookSide>(data.to_vec(), Some(0))?;
        //         book_sides.insert(acc.pubkey.to_string(), book_side);
        //     } else if acc_disc == DISC_OPEN_ORDERS {
        //         debug!("found openbook OpenOrdersAccount account");
        //         let open_order: OpenOrdersAccount = OpenOrdersAccount::deserialize(&mut data)?;
        //         let market = open_order.market.to_string();
        //         debug!("OpenOrdersAccount market: {}", market);
        //         open_orders
        //             .entry(market)
        //             .or_default()
        //             .insert(acc.pubkey.to_string(), open_order);
        //     }
        // }

        // for (market_acc, market) in markets.iter() {
        //     let bids_key = market.bids.to_string();
        //     let asks_key = market.asks.to_string();
        //     let base_mint = market.base_mint.to_string();
        //     let quote_mint = market.quote_mint.to_string();
        //     let base_decimal = market.base_decimals;
        //     let quote_decimal = market.quote_decimals;
        //     let big_base_decimal = Decimal::new(10, 0).powu(base_decimal as u64);
        //     let big_quote_decimal = Decimal::new(10, 0).powu(quote_decimal as u64);
        //     let Some(bids) = book_sides.get(&bids_key) else {
        //         debug!(
        //             "This bids branch shouldn't hit with a full snapshot {}",
        //             bids_key
        //         );
        //         continue;
        //     };
        //     let Some(asks) = book_sides.get(&asks_key) else {
        //         debug!(
        //             "This asks branch shouldn't hit with a full snapshot {}",
        //             asks_key
        //         );
        //         continue;
        //     };
        //     let Some(market_open_orders) = open_orders.get(market_acc) else {
        //         // this branch will hit likely due to markets exist but not one open order.
        //         // that is fine. The market will get created once there are orders during realtime.
        //         debug!("Unable to get open orders from market: {}", market_acc);
        //         continue;
        //     };

        //     for (open_order_key, open_order) in market_open_orders.iter() {
        //         let cache_key = self.get_cache_key(open_order_key);
        //         let owner = open_order.owner.to_string();
        //         oo_to_owner_cache.insert(cache_key, owner);

        //         let base_amount: Decimal = open_order.position.base_free_native.into();
        //         let claimable_base = ClaimableBalanceUserDooot {
        //             time,
        //             mint: base_mint.to_string(),
        //             owner: open_order.owner.to_string(),
        //             relates_to: market_acc.to_string(),
        //             relates_to_type: "m".to_string(),
        //             amount: base_amount / big_base_decimal,
        //             program: OPENBOOK_V2_PROGRAM.to_string(),
        //         };
        //         let quote_amount: Decimal = open_order.position.quote_free_native.into();
        //         let claimable_quote = ClaimableBalanceUserDooot {
        //             time,
        //             mint: quote_mint.to_string(),
        //             owner: open_order.owner.to_string(),
        //             relates_to: market_acc.to_string(),
        //             relates_to_type: "m".to_string(),
        //             amount: quote_amount / big_quote_decimal,
        //             program: OPENBOOK_V2_PROGRAM.to_string(),
        //         };

        //         tx.send(Dooot::ClaimableBalanceUser(claimable_base))?;
        //         tx.send(Dooot::ClaimableBalanceUser(claimable_quote))?;

        //         for bid in bids.iter_valid(now_ts, None)? {
        //             let leaf = bid.node;
        //             let (price, quantity) = self
        //                 .calculate_price_and_quantity(leaf, market)
        //                 .expect("expected to have price, qty");
        //             let key = leaf.key;
        //             if leaf.owner.to_string() == *open_order_key {
        //                 let oo = ClobUserDooot {
        //                     time,
        //                     price,
        //                     unfilled_qty: quantity,
        //                     market: market_acc.to_string(),
        //                     owner: open_order.owner.to_string(),
        //                     order_id: key.to_string(),
        //                     is_buy: true,
        //                     program: OPENBOOK_V2_PROGRAM.to_string(),
        //                     account: open_order_key.to_string(),
        //                     base_mint: base_mint.to_string(),
        //                     quote_mint: quote_mint.to_string(),
        //                     expiry_ts: None,
        //                     closed: false,
        //                 };
        //                 tx.send(Dooot::ClobUser(oo))?;
        //             }
        //         }

        //         for ask in asks.iter_valid(now_ts, None)? {
        //             let leaf = ask.node;
        //             let (price, quantity) = self
        //                 .calculate_price_and_quantity(leaf, market)
        //                 .expect("expected to have price, qty");
        //             let key = leaf.key;
        //             if leaf.owner.to_string() == *open_order_key {
        //                 let oo = ClobUserDooot {
        //                     time,
        //                     price,
        //                     unfilled_qty: quantity,
        //                     market: market_acc.to_string(),
        //                     owner: open_order.owner.to_string(),
        //                     order_id: key.to_string(),
        //                     is_buy: false,
        //                     program: OPENBOOK_V2_PROGRAM.to_string(),
        //                     account: open_order_key.to_string(),
        //                     base_mint: base_mint.to_string(),
        //                     quote_mint: quote_mint.to_string(),
        //                     expiry_ts: None,
        //                     closed: false,
        //                 };
        //                 tx.send(Dooot::ClobUser(oo))?;
        //             }
        //         }
        //     }
        // }
        // if !oo_to_owner_cache.is_empty() {
        //     let runtime = Runtime::new().expect("expected to create runtime");
        //     runtime.block_on(async {
        //         for (key, owner) in oo_to_owner_cache.iter() {
        //             ts_cache
        //                 .set(key, owner, time, None)
        //                 .await
        //                 .expect("expected to set oo to owner cache");
        //         }
        //     });
        // }
        Ok(())
    }
}

// #[cfg(test)]
// mod tests {
//     use std::{fs, rc::Rc};

//     use serde_json::Value;
//     use step_ingestooor_sdk::{decoded_transaction::DecodedTransaction, dooot::Dooot};

//     use crate::TransactionParserTrait;
//     use step_ingestooor_sdk::structs::simple_mem_cache::SimpleMemCache;

//     use super::*;

//     #[tokio::test]
//     async fn openbook_v2_consume_events() {
//         let cache = SimpleMemCache::new(None).await;

//         let message_json = fs::read_to_string("./tests/openbook_v2/consume_events.json").unwrap();
//         let json_val: Value = serde_json::from_str(&message_json).unwrap();
//         let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();

//         let step_utils = Rc::new(StepUtils::new(None));

//         let mut dooot = vec![];
//         let parser = OpenbookV2;

//         parser
//             .invoke(&decoded_txn, &mut dooot, &step_utils)
//             .await
//             .unwrap();

//         assert_eq!(dooot.len(), 1);
//         // first event
//         let first_event = dooot.first().unwrap();
//         if let Dooot::ClobFillEvent(clob) = first_event {
//             assert_eq!(clob.base_qty, 27508820000);
//             assert_eq!(clob.price, Decimal::from(22));
//             assert!(clob.buy_side);
//             assert_eq!(
//                 clob.base_mint_pubkey,
//                 "PRSMNsEPqhGVCH1TtWiJqPjJyh2cKrLostPZTNy1o5x"
//             );
//             assert_eq!(
//                 clob.quote_mint_pubkey,
//                 "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
//             );
//             assert_eq!(
//                 clob.taker_pubkey,
//                 "benRLpbWCL8P8t51ufYt522419hGF5zif3CqgWGbEUm"
//             );
//             assert_eq!(
//                 clob.maker_pubkey,
//                 "4GmzR9Loo2MGsfR7g4QuCXvWa7YRQ8pcP5VBREaG3smc"
//             );
//             assert_eq!(clob.program_id, OPENBOOK_V2_PROGRAM.to_string());
//             assert_eq!(
//                 clob.market_pubkey,
//                 "G6veo6MpQXc9vbM6p3wcRYyBee2na1uuD9y6CT1ZRgjw"
//             );
//             assert_eq!(clob.referrer_pubkey, None);
//         }
//     }

//     #[tokio::test]
//     async fn openbook_v2_place_order() {
//         let simple_cache = SimpleMemCache::new(None).await;

//         let message_json = fs::read_to_string("./tests/openbook_v2/place_order.json").unwrap();
//         let json_val: Value = serde_json::from_str(&message_json).unwrap();
//         let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
//         let time = decoded_txn.get_block_time();
//         let mut dooot = vec![];
//         let parser = OpenbookV2;
//         let oo_accounts_to_owner = vec![
//             (
//                 "6WvZQynLRDb1x3Ft9AbcAdVvtB8hhJSW3ff1ik1dcPkW".to_string(),
//                 "58sZ8qWbutYpiRzUb2AJ9LE7PtRewvR1zYLU21gQZea1".to_string(),
//             ),
//             (
//                 "6PE85DKASz2THzbyRzoU17uDTDZ5NVagbnjoiTZfRikt".to_string(),
//                 "CjjA4gwKvGZ1N2BrNC6t1ChyReabrkc6z9FFLJoLf8KD".to_string(),
//             ),
//         ];

//         for (oo, owner) in oo_accounts_to_owner {
//             let key = parser.get_cache_key(&oo);
//             simple_cache
//                 .basic_set(key, owner, time)
//                 .await
//                 .expect("expected to seed cache");
//         }

//         let step_utils = Rc::new(StepUtils::new(None));

//         parser
//             .invoke(&decoded_txn, &mut dooot, &step_utils)
//             .await
//             .unwrap();

//         let wc = &dooot[0];
//         let is_wallet_claimable = match wc {
//             Dooot::ClaimableBalanceUser(wc) => {
//                 assert_eq!(wc.amount.to_string(), "55.95");
//                 assert_eq!(wc.mint, "StepAscQoEioFxxWGnh2sLBDFp9d8rvKz2Yp39iDpyT");
//                 assert_eq!(wc.relates_to_type, "m");
//                 true
//             }
//             _ => false,
//         };
//         assert!(is_wallet_claimable);
//         assert_eq!(dooot.len(), 3);
//     }

//     #[tokio::test]
//     async fn openbook_v2_cancel_order() -> Result<()> {
//         let cache = SimpleMemCache::new(None).await;
//         let message_json = fs::read_to_string("./tests/openbook_v2/cancel_order.json").unwrap();
//         let json_val: Value = serde_json::from_str(&message_json).unwrap();
//         let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
//         let step_utils = Rc::new(StepUtils::new(None));

//         let mut dooot = vec![];
//         let parser = OpenbookV2;

//         parser
//             .invoke(&decoded_txn, &mut dooot, &step_utils)
//             .await
//             .unwrap();

//         let time = decoded_txn.get_block_time();

//         let oo_account = "CdzdkiAbadKkVhcT1rnRb2JYCsps6vUci6eWQUggoBkH".to_string();
//         let key = parser.get_cache_key(&oo_account);
//         let data: Option<String> = cache.get(key, Some(time), None).await?;

//         // make sure it has cached correctly
//         if let Some(key) = data {
//             assert_eq!(key, "JAMESC37CTVoFEt7TAEcqBjdjAfAWZiPR1YdWotAFjeQ");
//         }

//         let oo_dooot = &dooot[2];

//         let oo_dooot = match oo_dooot {
//             Dooot::ClobUser(oo) => {
//                 assert_eq!(oo.price.to_string(), "830");
//                 assert!(!oo.is_buy);
//                 true
//             }
//             _ => false,
//         };

//         assert!(oo_dooot);
//         assert_eq!(dooot.len(), 3);
//         Ok(())
//     }
// }
