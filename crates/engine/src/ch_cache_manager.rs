use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use chrono::NaiveDateTime;
use deadpool_postgres::tokio_postgres::types::ToSql;
use log::debug;
use serde::de::DeserializeOwned;
use serde::Serialize;
use step_ingestooor_sdk::structs::cache_error::CacheError;
use step_ingestooor_sdk::trait_defs::cache::{CacheClientTypes, TimeSeriesCacheLike};
use step_ingestooor_sdk::trait_defs::transaction::TransactionLike;

use crate::ch_cache_sql::ChCacheSql;

const MAX_QUERY_SECONDS: Duration = Duration::from_secs(5);

pub struct ChCacheManager {
    ch: clickhouse::Client,
}

#[async_trait]
impl TimeSeriesCacheLike for ChCacheManager {
    #[inline]
    async fn new(ch: Option<CacheClientTypes>) -> Self {
        match ch {
            Some(CacheClientTypes::CH(ch)) => Self { ch },
            _ => panic!("No config provided to ChCacheManager"),
        }
    }

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
        V: serde::de::DeserializeOwned + Send,
        K: ToSql + Into<String> + Send + Sync,
    {
        let key_str = key.into();
        debug!(
            "Cache lookup for key: {}, max_age: {:?}, ref_time: {:?}",
            key_str, max_age_seconds, reference_time
        );

        // Construct query based on the presence of `max_age_seconds` and `reference_time`
        let result = match (max_age_seconds, reference_time) {
            (Some(max_age), Some(ref_time)) => self
                .ch
                .query(ChCacheSql::GET_WITH_MAX_AGE_AND_REF_TIME)
                .bind(&key_str)
                .bind(ref_time)
                .bind(max_age)
                .fetch_one::<String>()
                .await
                .map_err(|e| CacheError::QueryError(e.to_string()))?,
            (None, Some(ref_time)) => self
                .ch
                .query(ChCacheSql::GET_WITH_REF_TIME)
                .bind(&key_str)
                .bind(ref_time)
                .fetch_one::<String>()
                .await
                .map_err(|e| CacheError::QueryError(e.to_string()))?,
            (None, None) => self
                .ch
                .query(ChCacheSql::GET_LATEST)
                .bind(&key_str)
                .fetch_one::<String>()
                .await
                .map_err(|e| CacheError::QueryError(e.to_string()))?,
            _ => unreachable!(),
        };

        serde_json::from_str(&result)
            .map_err(|e| CacheError::DeserializeError(key_str.clone(), e.to_string()))
    }

    /// Set a value in the cache
    ///
    /// Set ttl to None to disable expiration (for permanent mappings)
    async fn basic_set<V: Serialize + Send + Sync>(
        &self,
        key: String,
        value: V,
        reference_time: NaiveDateTime,
    ) -> Result<(), CacheError> {
        let value_str =
            serde_json::to_string(&value).map_err(|_| CacheError::SerializeError(key.clone()))?;

        let query = ChCacheSql::BASIC_SET;

        match tokio::time::timeout(
            MAX_QUERY_SECONDS,
            self.ch
                .query(query)
                .bind(&key)
                .bind(&value_str)
                .bind(reference_time)
                .execute(),
        )
        .await
        {
            Ok(query_result) => match query_result {
                Ok(_) => {
                    debug!("Cache set for key: {}", key);
                    Ok(())
                }
                Err(e) => Err(CacheError::QueryError(e.to_string())),
            },
            Err(_) => Err(CacheError::QueryTimeout("basic_set".to_string(), key)),
        }
    }

    /// Set a value in the cache with a timestamp
    ///
    /// Will not overwrite values with newer timestamps
    async fn set<V: Serialize + Send + Sync>(
        &self,
        key: &str,
        value: V,
        as_of: NaiveDateTime,
        _max_age: Option<u64>,
    ) -> Result<(), CacheError> {
        let value_str = serde_json::to_string(&value)
            .map_err(|_| CacheError::SerializeError(key.to_string()))?;

        let query = ChCacheSql::SET_WITH_AS_OF;

        match tokio::time::timeout(
            MAX_QUERY_SECONDS,
            self.ch
                .query(query)
                .bind(key)
                .bind(&value_str)
                .bind(as_of)
                .execute(),
        )
        .await
        {
            Ok(query_result) => match query_result {
                Ok(_) => {
                    debug!("Cache set for key: {} at time: {}", key, as_of);
                    Ok(())
                }
                Err(e) => Err(CacheError::QueryError(e.to_string())),
            },
            Err(_) => Err(CacheError::QueryTimeout("set".to_string(), key.to_string())),
        }
    }

    /// Get or set a value in the cache with a timestamp
    /// Used for caching on-chain state at specific times (like OpenBook market state)
    #[inline]
    async fn get_or_set<V, F, Fut>(
        &self,
        key: &str,
        reference_time: NaiveDateTime,
        max_age_seconds: Option<u64>,
        get_value_callback: F,
    ) -> Result<V, CacheError>
    where
        V: Serialize + DeserializeOwned + Send + Sync,
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<V>> + Send,
    {
        // Try to get the value first
        match self
            .get::<V, &str>(key, Some(reference_time), max_age_seconds)
            .await
        {
            Ok(value) => {
                debug!("Cache hit for key: {} at time {}", key, reference_time);
                Ok(value)
            }
            Err(_) => {
                // If not found, call the callback to get the value
                debug!(
                    "Cache miss for key: {} at time {}, fetching value",
                    key, reference_time
                );
                let value = get_value_callback()
                    .await
                    .map_err(|_| CacheError::GetValueCallBackError)?;
                // Store the value with the timestamp
                self.set(key, &value, reference_time, None).await?;
                debug!(
                    "Cached new value for key: {} at time {}",
                    key, reference_time
                );
                Ok(value)
            }
        }
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
        // Try to get the value first (no time constraints for basic)
        match self.get::<V, &str>(key, None, None).await {
            Ok(value) => {
                debug!("Cache hit for key: {}", key);
                Ok(value)
            }
            Err(_) => {
                // If not found, call the callback to get the value
                debug!("Cache miss for key: {}, fetching value", key);
                let value = get_value_callback()
                    .await
                    .map_err(|_| CacheError::GetValueCallBackError)?;
                // Store the value in cache as a permanent mapping
                self.basic_set(key.to_string(), &value, reference_time)
                    .await?;
                debug!("Cached new value for key: {}", key);
                Ok(value)
            }
        }
    }

    #[inline]
    async fn del<K: Into<String> + Send + Sync>(&self, key: K) -> Result<(), CacheError> {
        let key_str = key.into();
        debug!("Deleting cache entry for key: {}", key_str);

        let query = ChCacheSql::DEL;

        match tokio::time::timeout(
            MAX_QUERY_SECONDS,
            self.ch.query(query).bind(&key_str).execute(),
        )
        .await
        {
            Ok(query_result) => match query_result {
                Ok(_) => Ok(()),
                Err(e) => Err(CacheError::QueryError(e.to_string())),
            },
            Err(_) => Err(CacheError::QueryTimeout("del".to_string(), key_str)),
        }
    }

    #[inline]
    async fn get_mint_decimals(&self, mint: &str) -> Result<u64, CacheError> {
        let decimals: Option<i16> = self
            .ch
            .query(ChCacheSql::GET_MINT_DECIMALS)
            .bind(mint)
            .fetch_optional()
            .await
            .map_err(|e| CacheError::QueryError(e.to_string()))?;

        let decimals = decimals.ok_or_else(|| CacheError::NoDecimalsFound(mint.to_string()))?;

        if decimals < 0 {
            return Err(CacheError::NegativeDecimals(decimals));
        }

        Ok(decimals as u64)
    }

    #[inline]
    async fn get_supply(
        &self,
        mint: &str,
        reference_time: NaiveDateTime,
    ) -> Result<Option<u64>, CacheError> {
        let supply: Option<u64> = self
            .ch
            .query(ChCacheSql::GET_SUPPLY)
            .bind(mint)
            .bind(reference_time)
            .fetch_optional()
            .await
            .map_err(|e| CacheError::QueryError(e.to_string()))?;

        Ok(supply)
    }

    #[inline]
    async fn get_token_account_balance(
        &self,
        token_account: &str,
        reference_time: NaiveDateTime,
    ) -> Result<Option<u64>, CacheError> {
        debug!(
            "Looking up balance for account: {} at time: {}",
            token_account, reference_time
        );

        let balance: Option<u64> = self
            .ch
            .query(ChCacheSql::GET_TOKEN_ACCOUNT_BALANCE)
            .bind(token_account)
            .bind(reference_time)
            .fetch_optional()
            .await
            .map_err(|e| CacheError::QueryError(e.to_string()))?;

        Ok(balance)
    }

    /// Get supply of LP mint, but it might not be in this txn. If not, grab from cache.
    #[inline]
    async fn get_post_lp_mint_supply_with_cache(
        &self,
        decoded_transaction: &impl TransactionLike,
        mint: &str,
    ) -> Result<Option<u64>, CacheError> {
        // First try to get supply from transaction accounts
        let supply_from_accts = decoded_transaction
            .get_account_data_after(mint)
            .ok()
            .flatten()
            .and_then(|bytes| {
                bytes
                    .get(36..44)
                    .map(|b| b.try_into())
                    .transpose()
                    .ok()
                    .flatten()
                    .map(u64::from_le_bytes)
            });

        match supply_from_accts {
            Some(supply) => {
                debug!("Found LP mint supply in transaction: {}", supply);
                Ok(Some(supply))
            }
            None => {
                debug!("LP mint not in transaction, checking cache/TSDB");
                // If not in transaction, try to get from timeseries DB
                let reference_time = decoded_transaction.get_block_time();

                match self.get_supply(mint, reference_time).await {
                    Ok(supply) => Ok(supply),
                    // throw the error. We don't want to silently skip this
                    Err(e) => {
                        println!("Failed to get supply from cache/TSDB: {:#?}", mint);
                        Err(e)
                    }
                }
            }
        }
    }
}
