use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::NaiveDateTime;
use deadpool_postgres::tokio_postgres::types::ToSql;
use serde::{de::DeserializeOwned, Serialize};
use std::future::Future;
use step_ingestooor_sdk::{
    structs::cache_error::CacheError,
    trait_defs::{
        cache::{CacheClientTypes, TimeSeriesCacheLike},
        transaction::TransactionLike,
    },
};

use crate::{ch_cache_manager::ChCacheManager, ts_cache_manager::TsCacheManager};

// pub type CacheInstance = Arc<HybridCacheManager>;

pub struct HybridCacheManager {
    pub ts_cache: Arc<TsCacheManager>,
    pub ch_cache: Arc<ChCacheManager>,
}

#[async_trait]
impl TimeSeriesCacheLike for HybridCacheManager {
    #[inline]
    async fn new(_config: Option<CacheClientTypes>) -> Self {
        panic!("Use HybridCacheManager::new_manager instead");
    }

    #[inline]
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
        self.ts_cache
            .get(key, reference_time, max_age_seconds)
            .await
    }

    #[inline]
    async fn basic_set<V: Serialize + Send + Sync>(
        &self,
        key: String,
        value: V,
        reference_time: NaiveDateTime,
    ) -> Result<(), CacheError> {
        self.ts_cache.basic_set(key, value, reference_time).await
    }

    #[inline]
    async fn set<V: Serialize + Send + Sync>(
        &self,
        key: &str,
        value: V,
        as_of: NaiveDateTime,
        max_age: Option<u64>,
    ) -> Result<(), CacheError> {
        self.ts_cache.set(key, value, as_of, max_age).await
    }

    #[inline]
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
        Fut: Future<Output = Result<V>> + Send,
    {
        self.ts_cache
            .get_or_set(key, reference_time, ttl, get_value_callback)
            .await
    }

    #[inline]
    async fn get_or_set_basic<V, F, Fut>(
        &self,
        key: &str,
        reference_time: NaiveDateTime,
        get_value_callback: F,
    ) -> Result<V, CacheError>
    where
        V: Serialize + DeserializeOwned + Send + Sync,
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<V>> + Send,
    {
        self.ts_cache
            .get_or_set_basic(key, reference_time, get_value_callback)
            .await
    }

    #[inline]
    async fn del<K: Into<String> + Send + Sync>(&self, key: K) -> Result<(), CacheError> {
        self.ts_cache.del(key).await
    }

    #[inline]
    async fn get_mint_decimals(&self, mint: &str) -> Result<u64, CacheError> {
        self.ts_cache.get_mint_decimals(mint).await
    }

    #[inline]
    async fn get_supply(
        &self,
        mint: &str,
        reference_time: NaiveDateTime,
    ) -> Result<Option<u64>, CacheError> {
        self.ts_cache.get_supply(mint, reference_time).await
    }

    #[inline]
    async fn get_token_account_balance(
        &self,
        token_account: &str,
        reference_time: NaiveDateTime,
    ) -> Result<Option<u64>, CacheError> {
        self.ch_cache
            .get_token_account_balance(token_account, reference_time)
            .await
    }

    #[inline]
    async fn get_post_lp_mint_supply_with_cache(
        &self,
        decoded_transaction: &impl TransactionLike,
        mint: &str,
    ) -> Result<Option<u64>, CacheError> {
        self.ts_cache
            .get_post_lp_mint_supply_with_cache(decoded_transaction, mint)
            .await
    }
}

impl HybridCacheManager {
    //     pub async fn new_manager(ts_cache: Option<Config>, ch_cache: Option<Client>) -> Self {
    //         let ch_cache = ChCacheManager::new(Some(CacheClientTypes::CH(
    //             ch_cache.expect("No ch cache provided"),
    //         )))
    //         .await;

    //         let ts_cache = TsCacheManager::new(Some(CacheClientTypes::TSDB(
    //             ts_cache.expect("No ts cache provided"),
    //         )))
    //         .await;
    //         Self {
    //             ts_cache: Arc::new(ts_cache),
    //             ch_cache: Arc::new(ch_cache),
    //         }
    //     }
}
