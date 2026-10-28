use crate::staking::marinade::SOL_MINT;
use crate::state_source::{AccountProvider, AccountProviderAccount, StateSource};
use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::Context;
use anyhow::{anyhow, Ok};
use async_trait::async_trait;
use borsh::BorshDeserialize;
use num_traits::ToPrimitive;
use rust_decimal::Decimal;
use solana_sdk::account::ReadableAccount;
use solana_sdk::{program_pack::Pack, pubkey::Pubkey};
use spl_token::state::Mint;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::string::ToString;
use std::sync::mpsc::SyncSender;
use step_ingestooor_idl_gens::jupiter_perp_gen::jup_perps::accounts::{Custody, Pool};
use step_ingestooor_sdk::dooot::yield_bearing::YieldBearingGlobalDooot;
use step_ingestooor_sdk::dooot::{Dooot, MintUnderlyingsGlobalDooot};
use step_ingestooor_sdk::utils::util_fns::convert_blocktime_to_micros;
use step_ingestooor_sdk::{trait_defs::transaction::TransactionLike, utils::step_utils::StepUtils};

pub struct JLP;
const JLP_MINT: &str = "27G8MtK7VtTcCHkpASjSDdkWWYfoqT6ggEuKidVJidD4";
const PERPETUALS_PROGRAM_ID: &str = "PERPHjGBqRHArX4DySjwM6UJHiR3sWAatqfdBS2qQJu";
const POOL_STATE_PK: &str = "5BUwFW4nRbftYTDMbgxykoFWqWHPzahFSNAaaaJtVKsq";

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
enum Token {
    Usdc,
    Sol,
    Usdt,
    Wbtc,
    Weth,
}

impl Token {
    fn all() -> &'static [Token] {
        use Token::*;
        static ALL: [Token; 5] = [Usdc, Sol, Usdt, Wbtc, Weth];
        &ALL
    }
    fn decimals(&self) -> u8 {
        match self {
            Token::Usdc => 6,
            Token::Sol => 9,
            Token::Usdt => 6,
            Token::Wbtc => 8,
            Token::Weth => 8,
        }
    }

    fn mint_address(&self) -> &'static str {
        match self {
            Token::Usdc => "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            Token::Sol => SOL_MINT,
            Token::Usdt => "Es9vMFrzaCERv7rhbGX2h7LCcTnPvxXqqzqfHWTyoYR",
            Token::Wbtc => "9n4nbM75f5Ui33ZbPYXn59EwSgE8CGsHtAeTH5YFeJ9E",
            Token::Weth => "7vfCXTUXx5WJV2o5aH8uG8Urx8eVrqQD5oKU68ecj4Mu",
        }
    }
    fn custody_address(&self) -> &'static str {
        match self {
            Token::Usdc => "G18jKKXQwBbrHeiK3C9MRXhkHsLHf7XgCSisykV46EZa",
            Token::Sol => "7xS2gz2bTp3fwCC7knJvUWTEU9Tycczu6VhJYKgi1wdz",
            Token::Usdt => "4vkNeXiYEUizLdrpdPS1eC2mccyM4NUPRtERrk6ZETkk",
            Token::Wbtc => "5Pv3gM9JrFFH883SWAhvJC9RPYmo8UNxuFtv5bMMALkm",
            Token::Weth => "AQCGyheWPLeo6Qp9WpYS9m3Qj479t7R636N9ey1rEjEn",
        }
    }

    pub fn custodies() -> Vec<String> {
        Token::all()
            .iter()
            .map(|tok| tok.custody_address().to_string())
            .collect()
    }
}

impl JLP {
    fn fetch_custody_assets(tok: &Token, txn: &impl TransactionLike) -> anyhow::Result<(u64, u64)> {
        let acc = tok.custody_address();
        let data = txn
            .get_account_data_after(acc)?
            .ok_or_else(|| anyhow!("missing custody account {}", acc))?;
        let c: Custody = Custody::deserialize(&mut &data[8..])
            .map_err(|e| anyhow!("custody deserialize failed: {}", e))?;
        Ok((c.assets.owned, c.assets.locked))
    }

    fn fetch_jlp_mint_info(txn: &impl TransactionLike) -> Option<(u64, u8)> {
        let data = txn.get_account_data_after(JLP_MINT).ok()??;
        let m = Mint::unpack(&data).ok()?;
        Some((m.supply, m.decimals))
    }

    fn get_mint_underlyings_global(
        txn: &impl TransactionLike,
    ) -> anyhow::Result<(HashMap<String, f64>, HashMap<String, Decimal>)> {
        let Some((supply_raw, supply_dec)) = Self::fetch_jlp_mint_info(txn) else {
            return Ok((HashMap::new(), HashMap::new()));
        };
        let jlp_supply = supply_raw as f64 / 10f64.powi(supply_dec as i32);
        if jlp_supply == 0.0 {
            return Err(anyhow!("JLP supply is zero"));
        }

        let mut per_parent = HashMap::new();
        let mut totals = HashMap::new();

        for &tok in Token::all() {
            let dec = tok.decimals();
            let mint = tok.mint_address().to_string();

            let (owned, _) = Self::fetch_custody_assets(&tok, txn)?;
            totals.insert(mint.clone(), Decimal::from(owned));
            let hr_amount = owned as f64 / 10f64.powi(dec as i32);
            per_parent.insert(mint.clone(), hr_amount / jlp_supply);
        }

        Ok((per_parent, totals))
    }
}
#[async_trait]
impl TransactionParserTrait for JLP {
    fn get_name(&self) -> &'static str {
        "JLP"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            PERPETUALS_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> anyhow::Result<()> {
        let account_keys = txn.get_account_keys();
        let has_all_custodies = Token::custodies()
            .iter()
            .all(|custody| account_keys.iter().any(|acc| acc.pubkey == *custody));
        let has_pool_state = account_keys.iter().any(|acc| acc.pubkey == POOL_STATE_PK);

        if !(has_all_custodies && has_pool_state) {
            return Ok(());
        }
        let pre_data = txn
            .get_account_data_before(POOL_STATE_PK)?
            .ok_or_else(|| anyhow!("missing pool state"))?;
        let post_data = txn
            .get_account_data_after(POOL_STATE_PK)?
            .ok_or_else(|| anyhow!("missing pool state"))?;

        if post_data != pre_data {
            let pool_acc = Pool::deserialize(&mut &post_data[8..])?;
            let apr_u64 = pool_acc.pool_apr.fee_apr_bps;
            let apr_dec = Decimal::from(apr_u64)
                .checked_div(Decimal::new(100, 0))
                .context("Division by zero when calculating APR")?;
            let apr_f64 = apr_dec
                .to_f64()
                .context("Failed to convert APR Decimal to f64")?; //weekly apr
            let apr_dooot = YieldBearingGlobalDooot {
                time: txn.get_block_time(),
                yield_bearing_mint: JLP_MINT.to_string(),
                program_pubkey: PERPETUALS_PROGRAM_ID.to_string(),
                apr: apr_f64,
            };
            dooot.push(Dooot::YieldBearingGlobal(apr_dooot));
            let (per_parent, totals) = Self::get_mint_underlyings_global(txn)?;

            if per_parent.is_empty() || totals.is_empty() {
                // in the price feed update txns, we dont get the jlp's supply via mint info in the txn, so it's not related, skipping sliently
                return Ok(());
            }
            let mints: Vec<String> = Token::all()
                .iter()
                .map(|tok| tok.mint_address().to_string())
                .collect();

            let mints_qty_per_one_parent: Vec<f64> = mints
                .iter()
                .map(|mint| {
                    per_parent
                        .get(mint)
                        .copied()
                        .context(format!("Missing ratio for token {}", mint))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;

            let total_underlying_amounts: Vec<Decimal> = mints
                .iter()
                .map(|mint| {
                    totals
                        .get(mint)
                        .cloned()
                        .context(format!("Missing total for token {}", mint))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;

            let jlp_dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                time: txn.get_block_time(),
                mint_pubkey: JLP_MINT.to_string(),
                platform_program_pubkey: PERPETUALS_PROGRAM_ID.to_string(),
                mints,
                mints_qty_per_one_parent,
                total_underlying_amounts,
                discriminant_id: JLP_MINT.to_string(),
            });
            dooot.push(jlp_dooot);
        }

        Ok(())
    }
}

impl StateSource for JLP {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> anyhow::Result<()> {
        let timestamp = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let mut wanted: HashSet<Pubkey> = Token::custodies()
            .into_iter()
            .map(|s| Pubkey::from_str(&s).with_context(|| format!("Failed to parse pubkey {}", s)))
            .collect::<anyhow::Result<HashSet<_>>>()?;

        wanted.insert(Pubkey::from_str(JLP_MINT)?);
        let all_accs: Vec<AccountProviderAccount> = acct_provider
            .all_accounts()
            .filter(|acct| wanted.contains(&acct.pubkey))
            .collect();

        let mut owned_map: HashMap<Token, u64> = HashMap::new();
        let mut mints = Vec::with_capacity(5);
        let mut ratios = Vec::with_capacity(5);
        let mut totals = Vec::with_capacity(5);

        for acct in all_accs {
            let mut jlp_supply = 0.0;
            for &tok in Token::all() {
                let data = acct.account.data();
                let c: Custody = Custody::deserialize(&mut &data[8..])?;
                owned_map.insert(tok, c.assets.owned);
            }

            if acct.pubkey.to_string() == JLP_MINT {
                let mint_acc = &acct.account.data();
                let mint = Mint::unpack(mint_acc)?;
                jlp_supply = mint.supply as f64 / 10f64.powi(mint.decimals as i32);
            }

            for &tok in Token::all() {
                let raw = owned_map[&tok];
                let dec = tok.decimals() as i32;
                let hr = raw as f64 / 10f64.powi(dec);

                mints.push(tok.mint_address().to_string());
                totals.push(Decimal::from(raw));
                ratios.push(hr / jlp_supply);
            }
        }
        let dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
            time: timestamp,
            mint_pubkey: JLP_MINT.to_string(),
            platform_program_pubkey: PERPETUALS_PROGRAM_ID.to_string(),
            mints,
            mints_qty_per_one_parent: ratios,
            total_underlying_amounts: totals,
            discriminant_id: JLP_MINT.to_string(),
        });
        tx.send(dooot)?;

        Ok(())
    }

    fn get_name(&self) -> &'static str {
        "JLP"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    use step_ingestooor_sdk::utils::test_utils::init_test_extras_bare;
    use TransactionParserTrait;

    #[tokio::test]
    async fn failing_test() {
        let path = "./tests/jlp/failing.json";
        let message_json = fs::read_to_string(path).unwrap();
        let f_json: DecodedTransaction = DecodedTransaction::from_rabbit_message_json(
            serde_json::from_str(&message_json).unwrap(),
        )
        .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: JLP = JLP;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 1);
    }

    #[tokio::test]
    async fn invoke_test() {
        let path = "./tests/jlp/jlp.json";
        let message_json = fs::read_to_string(path).unwrap();
        let f_json: DecodedTransaction = DecodedTransaction::from_rabbit_message_json(
            serde_json::from_str(&message_json).unwrap(),
        )
        .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: JLP = JLP;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 0); // if pool acc didnt change or the 5 custody acc doesnt exist, that means we'll getting 0 dooot.
    }
    #[tokio::test]
    async fn add_liq() {
        let path = "./tests/jlp/jlp-add-liq.json";
        let message_json = fs::read_to_string(path).unwrap();
        let f_json: DecodedTransaction = DecodedTransaction::from_rabbit_message_json(
            serde_json::from_str(&message_json).unwrap(),
        )
        .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: JLP = JLP;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        assert_eq!(dooot.len(), 2);
    }

    #[tokio::test]
    async fn remove_liq() {
        let path = "./tests/jlp/jlp-rem-liq.json";
        let message_json = fs::read_to_string(path).unwrap();
        let f_json: DecodedTransaction = DecodedTransaction::from_rabbit_message_json(
            serde_json::from_str(&message_json).unwrap(),
        )
        .unwrap();
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooot = vec![];
        let parser: JLP = JLP;
        parser
            .invoke(&f_json, &mut dooot, &step_utils)
            .await
            .unwrap();
        println!("{:#?}", dooot);
        assert_eq!(dooot.len(), 2);
    }
}
