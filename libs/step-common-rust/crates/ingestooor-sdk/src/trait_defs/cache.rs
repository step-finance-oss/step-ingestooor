use anyhow::Result;
use async_trait::async_trait;
use chrono::NaiveDateTime;
use clickhouse::Client;
use deadpool_postgres::tokio_postgres::types::ToSql;
use deadpool_postgres::Config;
use serde::{de::DeserializeOwned, Serialize};
use std::future::Future;

use crate::{structs::cache_error::CacheError, trait_defs::transaction::TransactionLike};

pub enum CacheClientTypes {
    TSDB(Config),
    CH(Client),
}

#[async_trait]
pub trait TimeSeriesCacheLike: Send + Sync {
    async fn new(db_config: Option<CacheClientTypes>) -> Self;

    /// Get a value from the cache with optional time constraints
    ///
    /// Use cases:
    /// - Permanent mappings (e.g. open orders -> owner):
    ///     max_age = None, reference_time = None
    /// - Time-series lookups (e.g. token supply):
    ///     max_age = None, reference_time = Some(block_time)
    /// - Temporary matches (e.g. OpenBook fills):
    ///     max_age = Some(5), reference_time = Some(current_time)
    async fn get<'a, V, K>(
        &self,
        key: K,
        reference_time: Option<NaiveDateTime>,
        max_age_seconds: Option<u64>,
    ) -> Result<V, CacheError>
    where
        V: DeserializeOwned + Send,
        K: ToSql + Into<String> + Send + Sync;

    /// Set a value in the cache
    ///
    /// Set ttl to None to disable expiration (for permanent mappings)
    async fn basic_set<V: Serialize + Send + Sync>(
        &self,
        key: String,
        value: V,
        reference_time: NaiveDateTime,
    ) -> Result<(), CacheError>;

    /// Set a value in the cache with a timestamp
    ///
    /// Will not overwrite values with newer timestamps
    async fn set<V: Serialize + Send + Sync>(
        &self,
        key: &str,
        value: V,
        as_of: NaiveDateTime,
        max_age: Option<u64>, // Ignored for Postgres implementation
    ) -> Result<(), CacheError>;

    /// Get or set a value in the cache with a timestamp
    /// Used for caching on-chain state at specific times (like OpenBook market state)
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
        Fut: Future<Output = Result<V>> + Send;

    /// Get or set a permanent mapping in the cache
    /// Used for things like open orders -> owner mappings
    async fn get_or_set_basic<V, F, Fut>(
        &self,
        key: &str,
        reference_time: NaiveDateTime,
        get_value_callback: F,
    ) -> Result<V, CacheError>
    where
        V: Serialize + DeserializeOwned + Send + Sync,
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<V>> + Send;

    /// Delete a key from the cache
    async fn del<K: Into<String> + Send + Sync>(&self, key: K) -> Result<(), CacheError>;
    // Utility methods
    async fn get_mint_decimals(&self, mint: &str) -> Result<u64, CacheError>;

    async fn get_supply(
        &self,
        mint: &str,
        reference_time: NaiveDateTime,
    ) -> Result<Option<u64>, CacheError>;

    async fn get_token_account_balance(
        &self,
        token_account: &str,
        reference_time: NaiveDateTime,
    ) -> Result<Option<u64>, CacheError>;

    async fn get_post_lp_mint_supply_with_cache(
        &self,
        decoded_transaction: &impl TransactionLike,
        mint: &str,
    ) -> Result<Option<u64>, CacheError>;
}
