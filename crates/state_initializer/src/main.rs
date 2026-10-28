use std::{
    future::Future,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
};

use account_provider::ArchiveSnapshotAccountProvider;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ch_writer_lite::CHWriterLite;
use chrono::NaiveDateTime;
use clap::{Parser, Subcommand};
use itertools::Itertools;
use regex::Regex;
use serde::{de::DeserializeOwned, Serialize};
use solana_client::rpc_config::RpcBlockConfig;
use std::sync::mpsc;
use step_ingestooor_engine::ts_cache_manager::TsCacheManager;
use step_ingestooor_parsers::state_source::{StateSource, StateSourceParserManager};
use step_ingestooor_sdk::{
    structs::{cache_error::CacheError, simple_mem_cache::SimpleMemCache},
    trait_defs::{
        cache::{CacheClientTypes, TimeSeriesCacheLike},
        transaction::TransactionLike,
    },
};
use tokio_postgres::types::ToSql;

mod account_provider;
mod ch_writer_lite;

pub(crate) const SCHEMA_CHANNEL_BUFFER_SIZE: usize = 20_000;
pub(crate) const ACCT_READ_BUFFER_SIZE: usize = 25_000;
pub(crate) const DATA_WRITE_CHUNK_SIZE: usize = 1_000;

// Enum wrapper for the two implementations of TimeSeriesCacheLike
enum TSCache {
    Manager(TsCacheManager),
    MemCache(SimpleMemCache),
}

// Implement TimeSeriesCacheLike for our wrapper enum
#[async_trait]
impl TimeSeriesCacheLike for TSCache {
    async fn new(db_config: Option<CacheClientTypes>) -> Self {
        if let Some(config) = db_config {
            TSCache::Manager(TsCacheManager::new(Some(config)).await)
        } else {
            TSCache::MemCache(SimpleMemCache::new(None).await)
        }
    }

    async fn get<'a, V, K>(
        &self,
        key: K,
        reference_time: Option<NaiveDateTime>,
        max_age_seconds: Option<u64>,
    ) -> Result<V, CacheError>
    where
        V: DeserializeOwned + Send,
        K: ToSql + Into<String> + Send + Sync,
    {
        match self {
            TSCache::Manager(cache) => cache.get(key, reference_time, max_age_seconds).await,
            TSCache::MemCache(cache) => cache.get(key, reference_time, max_age_seconds).await,
        }
    }

    async fn basic_set<V: Serialize + Send + Sync>(
        &self,
        key: String,
        value: V,
        reference_time: NaiveDateTime,
    ) -> Result<(), CacheError> {
        match self {
            TSCache::Manager(cache) => cache.basic_set(key, value, reference_time).await,
            TSCache::MemCache(cache) => cache.basic_set(key, value, reference_time).await,
        }
    }

    async fn set<V: Serialize + Send + Sync>(
        &self,
        key: &str,
        value: V,
        as_of: NaiveDateTime,
        max_age: Option<u64>,
    ) -> Result<(), CacheError> {
        match self {
            TSCache::Manager(cache) => cache.set(key, value, as_of, max_age).await,
            TSCache::MemCache(cache) => cache.set(key, value, as_of, max_age).await,
        }
    }

    async fn get_or_set<V, F, Fut>(
        &self,
        key: &str,
        reference_time: NaiveDateTime,
        ttl: Option<u64>,
        get_value_callback: F,
    ) -> Result<V, CacheError>
    where
        V: Serialize + DeserializeOwned + Send + Sync,
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<V, anyhow::Error>> + Send,
    {
        match self {
            TSCache::Manager(cache) => {
                cache
                    .get_or_set(key, reference_time, ttl, get_value_callback)
                    .await
            }
            TSCache::MemCache(cache) => {
                cache
                    .get_or_set(key, reference_time, ttl, get_value_callback)
                    .await
            }
        }
    }

    async fn get_or_set_basic<V, F, Fut>(
        &self,
        key: &str,
        reference_time: NaiveDateTime,
        get_value_callback: F,
    ) -> Result<V, CacheError>
    where
        V: Serialize + DeserializeOwned + Send + Sync,
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<V, anyhow::Error>> + Send,
    {
        match self {
            TSCache::Manager(cache) => {
                cache
                    .get_or_set_basic(key, reference_time, get_value_callback)
                    .await
            }
            TSCache::MemCache(cache) => {
                cache
                    .get_or_set_basic(key, reference_time, get_value_callback)
                    .await
            }
        }
    }

    async fn del<K: Into<String> + Send + Sync>(&self, key: K) -> Result<(), CacheError> {
        match self {
            TSCache::Manager(cache) => cache.del(key).await,
            TSCache::MemCache(cache) => cache.del(key).await,
        }
    }

    async fn get_mint_decimals(&self, mint: &str) -> Result<u64, CacheError> {
        match self {
            TSCache::Manager(cache) => cache.get_mint_decimals(mint).await,
            TSCache::MemCache(cache) => cache.get_mint_decimals(mint).await,
        }
    }

    async fn get_supply(
        &self,
        mint: &str,
        reference_time: NaiveDateTime,
    ) -> Result<Option<u64>, CacheError> {
        match self {
            TSCache::Manager(cache) => cache.get_supply(mint, reference_time).await,
            TSCache::MemCache(cache) => cache.get_supply(mint, reference_time).await,
        }
    }

    async fn get_token_account_balance(
        &self,
        token_account: &str,
        reference_time: NaiveDateTime,
    ) -> Result<Option<u64>, CacheError> {
        match self {
            TSCache::Manager(cache) => {
                cache
                    .get_token_account_balance(token_account, reference_time)
                    .await
            }
            TSCache::MemCache(cache) => {
                cache
                    .get_token_account_balance(token_account, reference_time)
                    .await
            }
        }
    }

    async fn get_post_lp_mint_supply_with_cache(
        &self,
        decoded_transaction: &impl TransactionLike,
        mint: &str,
    ) -> Result<Option<u64>, CacheError> {
        match self {
            TSCache::Manager(cache) => {
                cache
                    .get_post_lp_mint_supply_with_cache(decoded_transaction, mint)
                    .await
            }
            TSCache::MemCache(cache) => {
                cache
                    .get_post_lp_mint_supply_with_cache(decoded_transaction, mint)
                    .await
            }
        }
    }
}

#[derive(Parser, Debug)]
pub struct Args {
    /// The name of the parser to run
    #[clap(short, long)]
    parser: String,

    /// The data to pass to the parser, if any.
    /// Interpreted as a `String`, but can be deserialized by the parser (manually) as needed
    #[clap(short, long)]
    data: Option<String>,

    #[clap(subcommand)]
    pub target: StateTarget,

    #[clap(long, env, parse(try_from_str), default_value = "false")]
    pub dry_run: bool,

    #[clap(long, env)]
    pub clickhouse_url: String,

    #[clap(long, env)]
    pub clickhouse_user: String,

    #[clap(long, env)]
    pub clickhouse_password: String,
}

#[derive(Subcommand, Debug)]
pub enum StateTarget {
    DB(DBTarget),
}

#[derive(Parser, Debug)]
pub struct DBTarget {
    #[clap(long, env)]
    pub snapshot_path: String,

    #[clap(long, env)]
    pub rpc_url: String,

    #[clap(long, env, default_value = "5")]
    pub pg_deadlock_retries: u8,

    #[clap(long, env, parse(try_from_str), default_value = "false")]
    pub dry_run: bool,

    #[clap(long, env, default_value = "1")]
    pub read_threads: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();
    let args = Args::parse();

    log::debug!("args: {:?}", args);

    // Clickhouse
    let ch_client = clickhouse::Client::default()
        .with_url(args.clickhouse_url)
        .with_user(args.clickhouse_user)
        .with_password(args.clickhouse_password);

    let parser_name = args.parser;
    let precursor_data = args.data;

    match args.target {
        StateTarget::DB(db) => init_database(ch_client, db, parser_name, precursor_data).await,
    }
}

async fn init_database(
    ch_client: clickhouse::Client,
    db: DBTarget,
    parser_name: String,
    precursor_data: Option<String>,
) -> anyhow::Result<()> {
    log::info!("PG deadlock retries: {}", db.pg_deadlock_retries);

    // Find the parser matching this name
    log::info!("setting up parser");
    let parser = StateSourceParserManager::get_parser_by_name(&parser_name)
        .context(format!("parser not found: {}", parser_name))?;
    log::info!("found parser");

    let (tx, rx) = mpsc::sync_channel(SCHEMA_CHANNEL_BUFFER_SIZE);

    thread::spawn(move || {
        //Setup bank
        let snapshot_path = db.snapshot_path.clone();
        let slot_regex = Regex::new(r"snapshot-(\d+)-").expect("Failed to create slot regex");
        let slot = slot_regex
            .captures(&snapshot_path)
            .expect("No slot found in snapshot path")
            .get(1)
            .expect("No slot found in snapshot path")
            .as_str()
            .parse::<u64>()
            .expect("Failed to parse slot from snapshot path");

        log::info!("getting block time for slot {}", slot);
        let solana_client = solana_client::rpc_client::RpcClient::new(db.rpc_url);
        let block = solana_client
            .get_block_with_config(
                slot,
                RpcBlockConfig {
                    max_supported_transaction_version: Some(2),
                    ..Default::default()
                },
            )
            .expect("Failed to get block");
        let block_time = block.block_time.expect("No block time found");
        log::info!("blocktime is: {}", block_time);
        log::info!("setting up snapshot extrator from {}", &snapshot_path);
        let acct_provider = ArchiveSnapshotAccountProvider {
            snapshot_path,
            block_time,
            threads: db.read_threads,
        };

        log::info!("getting state data stream from parser");
        parser
            .get_state(precursor_data, acct_provider, tx)
            .expect("Failed to get state data stream");
    });

    let (data_tx, data_rx) = crossbeam::channel::bounded(100);

    log::info!("starting chunker loop");
    tokio::spawn(async move {
        for dooots in &rx.into_iter().chunks(DATA_WRITE_CHUNK_SIZE) {
            let dooots = dooots.collect::<Vec<_>>();
            data_tx.send(dooots).expect("Failed to send dooots");
        }
    });

    log::info!(
        "{} starting data writer tasks",
        if db.dry_run { "[DRY RUN]" } else { "" }
    );
    let cnt = Arc::new(AtomicU64::new(0));
    let tasks = (0..3).map(|_| {
        let data_rx = data_rx.clone();
        let cnt = cnt.clone();
        let ch_client_clone = ch_client.clone();
        tokio::spawn(async move {
            while let Ok(dooots) = data_rx.recv() {
                let mut data_writer = CHWriterLite::new(ch_client_clone.clone());
                if !db.dry_run {
                    data_writer
                        .write_doots(&dooots)
                        .await
                        .expect("Failed to write dooots");
                    data_writer.flush().await.expect("Failed to flush");
                }
                cnt.fetch_add(dooots.len() as u64, Ordering::Relaxed);
                let cnt = cnt.load(Ordering::Relaxed);
                if cnt % (5 * DATA_WRITE_CHUNK_SIZE as u64) == 0 {
                    log::info!(
                        "{} wrote {} dooots",
                        if db.dry_run { "[DRY RUN]" } else { "" },
                        cnt
                    );
                }
                data_writer.end().await.expect("Failed to end");
            }
        })
    });

    log::info!("waiting for data writer tasks to complete");
    futures::future::join_all(tasks).await;

    log::info!("writing state completed");

    Ok(())
}
