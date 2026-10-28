use crate::trait_defs::{
    cache::{CacheClientTypes, TimeSeriesCacheLike},
    transaction::TransactionLike,
};
use anyhow::Result;
use async_trait::async_trait;
use chrono::NaiveDateTime;
use deadpool_postgres::tokio_postgres::types::ToSql;
use log::debug;
use parking_lot::RwLock;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{any::type_name, collections::HashMap};

use super::cache_error::CacheError;

/// Simple in-memory cache for testing
///
/// Note: This implementation does NOT respect time constraints and should only be used for testing
pub struct SimpleMemCache {
    cache: RwLock<HashMap<String, String>>,
}

// A representative implementation of the TimeSeriesCacheLike trait
#[async_trait]
impl TimeSeriesCacheLike for SimpleMemCache {
    async fn new(_: Option<CacheClientTypes>) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    async fn get<'a, V, K>(
        &self,
        key: K,
        _: Option<NaiveDateTime>,
        _: Option<u64>,
    ) -> Result<V, CacheError>
    where
        V: DeserializeOwned + Send,
        K: ToSql + Into<String> + Send + Sync,
    {
        let key = key.into();
        let cache = self.cache.read();
        match cache.get(&key) {
            Some(value) => serde_json::from_str(value)
                .map_err(|e| CacheError::DeserializeError(key, e.to_string())),
            None => {
                // Check if V is an Option type
                if type_name::<V>().starts_with("core::option::Option") {
                    // For Option types, deserialize null to None from V
                    Ok(V::deserialize(Value::Null)
                        .map_err(|e| CacheError::DeserializeError(key, e.to_string()))?)
                } else {
                    // For non-Option types, return an error
                    Err(CacheError::NoValueFound(key.to_string(), key.to_string()))
                }
            }
        }
    }

    async fn basic_set<V: Serialize + Send + Sync>(
        &self,
        key: String,
        value: V,
        _: NaiveDateTime,
    ) -> Result<(), CacheError> {
        let value = serde_json::to_string(&value)
            .map_err(|_| CacheError::SerializeError(key.to_string()))?;
        self.cache.write().insert(key, value);
        Ok(())
    }

    async fn set<V: Serialize + Send + Sync>(
        &self,
        key: &str,
        value: V,
        _as_of: NaiveDateTime,
        _max_age: Option<u64>,
    ) -> Result<(), CacheError> {
        let value = serde_json::to_string(&value)
            .map_err(|_| CacheError::SerializeError(key.to_string()))?;
        self.cache.write().insert(key.to_string(), value);
        Ok(())
    }

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
        Fut: std::future::Future<Output = Result<V>> + Send,
    {
        // Check cache first, but scope the lock guard
        let cached_value = {
            let cache = self.cache.read();
            cache
                .get(key)
                .and_then(|v| serde_json::from_str::<V>(v).ok())
        };

        if let Some(value) = cached_value {
            return Ok(value);
        }

        // If not found, get new value
        let value = get_value_callback()
            .await
            .map_err(|_| CacheError::GetValueCallBackError)?;
        self.set(key, &value, reference_time, max_age_seconds)
            .await?;
        Ok(value)
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
        Fut: std::future::Future<Output = Result<V>> + Send,
    {
        // Check cache first, but scope the lock guard
        let cached_value = {
            let cache = self.cache.read();
            cache
                .get(key)
                .and_then(|v| serde_json::from_str::<V>(v).ok())
        };

        if let Some(value) = cached_value {
            return Ok(value);
        }

        // If not found, get new value
        let value = get_value_callback()
            .await
            .map_err(|_| CacheError::GetValueCallBackError)?;
        self.basic_set(key.to_string(), &value, reference_time)
            .await?;
        Ok(value)
    }

    async fn del<K: Into<String> + Send + Sync>(&self, key: K) -> Result<(), CacheError> {
        self.cache.write().remove(&key.into());
        Ok(())
    }

    async fn get_mint_decimals(&self, _mint: &str) -> Result<u64, CacheError> {
        // For testing, return a default value
        Ok(9)
    }

    async fn get_supply(&self, _mint: &str, _: NaiveDateTime) -> Result<Option<u64>, CacheError> {
        Ok(Some(100_000))
    }

    async fn get_token_account_balance(
        &self,
        _mint: &str,
        _as_of: NaiveDateTime,
    ) -> Result<Option<u64>, CacheError> {
        Ok(Some(100_000))
    }

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
            None => Ok(Some(100_000)),
        }
    }
}

// Specific methods for SimpleMemCache to seed the cache with values for testing
impl SimpleMemCache {
    /// Seed mint decimals directly
    pub async fn seed_mint_decimals(&self, mint: &str, decimals: u64) -> Result<()> {
        let value = decimals.to_string();
        self.cache
            .write()
            .insert(format!("mint_decimals:{}", mint), value);
        Ok(())
    }

    /// Seed mint supply at a specific time
    pub async fn seed_mint_supply(
        &self,
        mint: &str,
        supply: u64,
        reference_time: NaiveDateTime,
    ) -> Result<()> {
        let value = supply.to_string();
        self.cache
            .write()
            .insert(format!("mint_supply:{}:{}", mint, reference_time), value);
        Ok(())
    }

    /// Seed token account balance at a specific time
    pub async fn seed_token_account_balance(
        &self,
        token_account: &str,
        balance: u64,
        reference_time: NaiveDateTime,
    ) -> Result<()> {
        let value = balance.to_string();
        self.cache.write().insert(
            format!("token_balance:{}:{}", token_account, reference_time),
            value,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_basic_operations() {
        let cache = SimpleMemCache::new(None).await;
        let now = Utc::now().naive_utc();

        // Test basic set and get
        cache
            .basic_set("test_key".to_string(), "test_value", now)
            .await
            .unwrap();
        let value: String = cache.get("test_key", None, None).await.unwrap();
        assert_eq!(value, "test_value");

        // Test deletion
        cache.del("test_key").await.unwrap();
        let result: Result<String, CacheError> = cache.get("test_key", None, None).await;
        assert!(result.is_err());
    }
}
