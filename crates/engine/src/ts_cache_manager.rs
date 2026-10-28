use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use chrono::NaiveDateTime;
use deadpool_postgres::tokio_postgres::types::ToSql;
use deadpool_postgres::tokio_postgres::NoTls;
use deadpool_postgres::GenericClient;
use deadpool_postgres::Runtime;
use log::debug;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::de::DeserializeOwned;
use serde::Serialize;
use step_ingestooor_sdk::structs::cache_error::CacheError;
use step_ingestooor_sdk::trait_defs::cache::{CacheClientTypes, TimeSeriesCacheLike};
use step_ingestooor_sdk::trait_defs::transaction::TransactionLike;
use tokio::time;

use crate::ts_cache_sql::TSCacheSql;

const MAX_QUERY_SECONDS: Duration = Duration::from_secs(5);

pub struct TsCacheManager {
    pg: deadpool_postgres::Pool,
}

impl TsCacheManager {
    // Helper function to execute queries with consistent error handling
    async fn execute_query<T>(
        &self,
        query: &str,
        params: &[&(dyn ToSql + Sync)],
        key: &str,
        row_handler: impl FnOnce(deadpool_postgres::tokio_postgres::Row) -> Result<T, CacheError>,
    ) -> Result<T, CacheError> {
        let connection = match self.pg.get().await {
            Ok(connection) => connection,
            Err(_) => return Err(CacheError::GetPgConnectionError),
        };
        let cancel_token = connection.cancel_token();
        debug!("Executing query: {}", query);
        let row = match time::timeout(MAX_QUERY_SECONDS, connection.query_opt(query, params)).await
        {
            Ok(query_result) => match query_result {
                Ok(row) => row,
                Err(e) => return Err(CacheError::QueryError(e.to_string())),
            },
            Err(_) => {
                let params_str = format!("key: {:?}, params: {:?}", key, params);
                // Make sure to cancel the query on DB side if we timeout
                if let Err(e) = cancel_token.cancel_query(NoTls).await {
                    return Err(CacheError::CancelQueryError(e.to_string()));
                }
                return Err(CacheError::QueryTimeout(query.to_string(), params_str));
            }
        };
        let row = row.ok_or_else(|| {
            let params_str = format!("key: {:?}, params: {:?}", key, params);
            CacheError::NoValueFound(query.to_string(), params_str)
        })?;
        row_handler(row)
    }
}

#[async_trait]
impl TimeSeriesCacheLike for TsCacheManager {
    #[inline]
    async fn new(config: Option<CacheClientTypes>) -> Self {
        match config {
            Some(CacheClientTypes::TSDB(config)) => {
                let pg = config
                    .create_pool(Some(Runtime::Tokio1), NoTls)
                    .expect("expected to create pg pool");
                Self { pg }
            }
            _ => panic!("No config provided to TsCacheManager"),
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
        let query = match (max_age_seconds, reference_time) {
            (Some(max_age), Some(_)) => TSCacheSql::get_with_max_age_and_ref_time(max_age),
            (None, Some(_)) => TSCacheSql::GET_WITH_REF_TIME.to_string(),
            (None, None) => TSCacheSql::GET_LATEST.to_string(),
            _ => unreachable!(),
        };

        let params: Vec<&(dyn ToSql + Sync)> = match reference_time {
            Some(ref ref_time) => vec![&key_str, ref_time],
            None => vec![&key_str],
        };

        self.execute_query(&query, &params, &key_str, |row| {
            let value_str: String = row.get(0);
            serde_json::from_str(&value_str)
                .map_err(|e| CacheError::DeserializeError(key_str.clone(), e.to_string()))
        })
        .await
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

        let connection = self
            .pg
            .get()
            .await
            .map_err(|_| CacheError::GetPgConnectionError)?;

        match time::timeout(
            MAX_QUERY_SECONDS,
            connection.execute(TSCacheSql::BASIC_SET, &[&key, &value_str, &reference_time]),
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

        let connection = self
            .pg
            .get()
            .await
            .map_err(|_| CacheError::GetPgConnectionError)?;

        match time::timeout(
            MAX_QUERY_SECONDS,
            connection.execute(TSCacheSql::SET_WITH_AS_OF, &[&key, &value_str, &as_of]),
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
        let connection = self
            .pg
            .get()
            .await
            .map_err(|_| CacheError::GetPgConnectionError)?;
        let query = TSCacheSql::DEL;

        match time::timeout(MAX_QUERY_SECONDS, connection.execute(query, &[&key_str])).await {
            Ok(query_result) => match query_result {
                Ok(_) => Ok(()),
                Err(e) => Err(CacheError::QueryError(e.to_string())),
            },
            Err(_) => Err(CacheError::QueryTimeout("del".to_string(), key_str)),
        }
    }

    #[inline]
    async fn get_mint_decimals(&self, mint: &str) -> Result<u64, CacheError> {
        self.execute_query(TSCacheSql::GET_MINT_DECIMALS, &[&mint], mint, |row| {
            let decimals: Option<i16> = row.get(0);
            let decimals = decimals.ok_or_else(|| CacheError::NoDecimalsFound(mint.to_string()))?;
            if decimals < 0 {
                return Err(CacheError::NegativeDecimals(decimals));
            }
            Ok(decimals as u64)
        })
        .await
    }

    #[inline]
    async fn get_supply(
        &self,
        mint: &str,
        reference_time: NaiveDateTime,
    ) -> Result<Option<u64>, CacheError> {
        self.execute_query(
            TSCacheSql::GET_SUPPLY,
            &[&mint.to_string(), &reference_time],
            mint,
            |row| {
                let supply: Decimal = row.get(0);
                supply
                    .to_u64()
                    .ok_or_else(|| {
                        CacheError::ValueExceedsU64Max("supply".to_string(), mint.to_string())
                    })
                    .map(Some)
            },
        )
        .await
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

        self.execute_query(
            TSCacheSql::GET_TOKEN_ACCOUNT_BALANCE,
            &[&token_account.to_string(), &reference_time],
            token_account,
            |row| {
                let balance: Decimal = row.get(0);
                balance
                    .to_u64()
                    .ok_or_else(|| {
                        CacheError::ValueExceedsU64Max(
                            "balance".to_string(),
                            token_account.to_string(),
                        )
                    })
                    .map(Some)
            },
        )
        .await
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
                    Err(e) => Err(e),
                }
            }
        }
    }
}
