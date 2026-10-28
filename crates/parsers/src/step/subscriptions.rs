use async_trait::async_trait;
use borsh::BorshDeserialize;
use solana_sdk::pubkey::Pubkey;
use spl_token::solana_program::pubkey;
use std::collections::HashMap;
use std::sync::mpsc::SyncSender;
use step_ingestooor_sdk::{
    dooot::{Dooot, SubscriptionUserDooot},
    trait_defs::{discriminatable::Discriminatable, transaction::TransactionLike},
    utils::step_utils::StepUtils,
};

use crate::state_source::{AccountProvider, StateSource};
use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::{anyhow, Context, Result};
use solana_sdk::account::ReadableAccount;
use step_ingestooor_sdk::dooot::SubscriptionPurchaseEventDooot;
use step_ingestooor_sdk::utils::util_fns::convert_blocktime_to_micros;

pub struct StepSubscriptions;

pub const STEP_SUBS_PROGRAM_ID: &str = "SsubM7pGA83VRnCodXVrxU2GZNBQzBYgzPUYBssCwU1";
pub const STEP_SUBS_PROGRAM_ID_PK: Pubkey = pubkey!("SsubM7pGA83VRnCodXVrxU2GZNBQzBYgzPUYBssCwU1");
pub const PURCHASE_DISC: &[u8] = &[0x21, 0xcb, 0x01, 0xfc, 0xe7, 0xe4, 0x08, 0x43];
pub const PRODUCT_DISC: &[u8] = &[0x66, 0x4c, 0x37, 0xfb, 0x26, 0x49, 0xe0, 0xe5];

#[derive(BorshDeserialize)]
pub struct PurchaseHistory {
    pub amount_paid: u64,
    pub quantity: u8,
    pub purchased_ts: i64,
    pub refunded: Option<u64>,
}

#[derive(BorshDeserialize)]
pub struct Purchase {
    pub bump: u8,
    pub owner: Pubkey,
    pub product: Pubkey,
    pub regarding: Pubkey,
    pub expires_ts: i64,
    pub history_10: Vec<PurchaseHistory>,
    pub reserved: [u8; 64],
}

impl Discriminatable for Purchase {
    fn discriminator() -> (Pubkey, &'static [u8]) {
        (
            STEP_SUBS_PROGRAM_ID_PK,
            &[0x21, 0xcb, 0x01, 0xfc, 0xe7, 0xe4, 0x08, 0x43],
        )
    }
}

#[derive(BorshDeserialize)]
pub struct Product {
    pub bump: u8,
    pub active: bool,
    pub code_16: String,
    pub name_32: String,
    pub blurb_128: String,
    pub family_32: String,
    pub upgrade_group_32: Option<String>,
    pub created_ts: i64,
    pub cost_mint: Pubkey,
    pub cost: u64,
    pub discount_bps: u64,
    pub qty_for_discount: u8,
    pub purchase_duration_secs: Option<i64>,
    pub max_owned: u8,
    pub total_paid: i64,
    pub assigned_roles_6_a10: Vec<String>,
    pub i64_a5: [i64; 5],
    pub str_20_a5: Vec<String>,
    pub purchase_qty: u64,
    pub purchase_revenue: u64,
    pub reserved: [u8; 240],
}

impl Discriminatable for Product {
    fn discriminator() -> (Pubkey, &'static [u8]) {
        (
            STEP_SUBS_PROGRAM_ID_PK,
            &[0x66, 0x4c, 0x37, 0xfb, 0x26, 0x49, 0xe0, 0xe5],
        )
    }
}
#[async_trait]
impl TransactionParserTrait for StepSubscriptions {
    fn get_name(&self) -> &'static str {
        "StepSubscriptions"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            STEP_SUBS_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let tx_id = txn.get_signature();
        let time = txn.get_block_time();
        let (purchase_iter, product_iter) =
            txn.get_accounts_typed_borsh_2::<Purchase, Product>()?;

        let mut product_map = HashMap::new();
        for product in product_iter {
            product_map.insert(product.pubkey.to_string(), product);
        }

        for purchase in purchase_iter {
            if !purchase.is_changed {
                continue;
            }

            let Some(post_data) = purchase.get_post()? else {
                continue;
            };

            let purchase = &post_data.0;

            let Some(product) = product_map.get(&purchase.product.to_string()) else {
                continue;
            };

            let product = product.get_post()?.context("Product not found")?;
            let product = &product.0;

            let product_name = &product.name_32;
            let product_code = &product.code_16;
            let product_family = &product.family_32;
            let description = &product.blurb_128;
            let payment_mint = &product.cost_mint;
            let regarding = &purchase.regarding;
            let owner = &purchase.owner;
            let expires_ts = purchase.expires_ts;
            let amount_paid = product.cost;

            let total_paid = purchase
                .history_10
                .iter()
                .map(|h| h.amount_paid - h.refunded.unwrap_or(0))
                .sum();
            let quantity_purchased = purchase.history_10.iter().map(|h| h.quantity as u64).sum();

            let sub_purchase_dooot = Dooot::SubscriptionUser(SubscriptionUserDooot {
                time,
                regarding_pubkey: regarding.to_string(),
                user_pubkey: owner.to_string(),
                payment_mint: payment_mint.to_string(),
                total_paid,
                name: product_name.to_string(),
                code: product_code.to_string(),
                description: description.to_string(),
                category: product_family.to_string(),
                quantity_purchased,
                expires_ts,
            });
            let purchase_dooot = Dooot::SubscriptionPurchaseEvent(SubscriptionPurchaseEventDooot {
                time,
                tx_id: tx_id.to_string(),
                regarding_pubkey: regarding.to_string(),
                user_pubkey: owner.to_string(),
                payment_mint: payment_mint.to_string(),
                amount_paid,
                name: product_name.to_string(),
                code: product_code.to_string(),
                description: description.to_string(),
                category: product_family.to_string(),
                quantity: quantity_purchased,
                expires_ts,
            });
            dooots.push(sub_purchase_dooot);
            dooots.push(purchase_dooot);
        }

        return Ok(());
    }
}
impl StateSource for StepSubscriptions {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)
            .map_err(|e| anyhow!("Failed to convert blocktime: {}", e))?;

        let mut product_map: HashMap<String, Vec<u8>> = HashMap::new();
        for product in acct_provider.program_accounts(STEP_SUBS_PROGRAM_ID_PK, Some(PRODUCT_DISC)) {
            product_map.insert(product.pubkey.to_string(), product.account.data().to_vec());
        }

        for purchase in acct_provider.program_accounts(STEP_SUBS_PROGRAM_ID_PK, Some(PURCHASE_DISC))
        {
            let purchase_data = &purchase.account.data();
            let purchase = Purchase::try_from_slice(&purchase_data[8..])?;

            let Some(product_data) = product_map.get(&purchase.product.to_string()) else {
                continue;
            };
            let product = Product::try_from_slice(&product_data[8..])?;

            let product_name = &product.name_32;
            let product_code = &product.code_16;
            let product_family = &product.family_32;
            let description = &product.blurb_128;
            let payment_mint = &product.cost_mint;
            let regarding = &purchase.regarding;
            let owner = &purchase.owner;
            let expires_ts = purchase.expires_ts;

            let total_paid = purchase
                .history_10
                .iter()
                .map(|h| h.amount_paid - h.refunded.unwrap_or(0))
                .sum();
            let quantity_purchased = purchase.history_10.iter().map(|h| h.quantity as u64).sum();

            let sub_purchase_dooot = Dooot::SubscriptionUser(SubscriptionUserDooot {
                time,
                regarding_pubkey: regarding.to_string(),
                user_pubkey: owner.to_string(),
                payment_mint: payment_mint.to_string(),
                total_paid,
                name: product_name.to_string(),
                code: product_code.to_string(),
                description: description.to_string(),
                category: product_family.to_string(),
                quantity_purchased,
                expires_ts,
            });
            tx.send(sub_purchase_dooot)?;
        }

        Ok(())
    }

    fn get_name(&self) -> &'static str {
        "Step Subscriptions"
    }
}
#[cfg(test)]
mod tests {
    use std::fs;
    use step_ingestooor_sdk::{
        decoded_transaction::DecodedTransaction, utils::test_utils::init_test_extras_bare,
    };

    use crate::{StepSubscriptions, TransactionParserTrait as _};

    #[tokio::test]
    async fn invoke_test() {
        let txn_file = fs::read_to_string("tests/step/step_subs.json").unwrap();
        let json = serde_json::from_str(&txn_file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();

        let (step_utils, _) = init_test_extras_bare().await;

        let parser = StepSubscriptions;
        let mut dooots = vec![];
        parser.invoke(&txn, &mut dooots, &step_utils).await.unwrap();
        assert_eq!(dooots.len(), 2);
    }
}
