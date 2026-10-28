use thiserror::Error;

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("No value found for query: {0}, key: {1}")]
    NoValueFound(String, String),
    #[error("Failed to serialize value for key: {0}")]
    SerializeError(String),
    #[error("Failed to get pg connection")]
    GetPgConnectionError,
    #[error("Query timeout for {0}: {1}")]
    QueryTimeout(String, String),
    #[error("Failed to deserialize value for key: {0}, error: {1}")]
    DeserializeError(String, String),
    #[error("Decimals cannot be negative: {0}")]
    NegativeDecimals(i16),
    #[error("No decimals found for mint: {0}")]
    NoDecimalsFound(String),
    #[error("Value exceeds u64 max: {0} for {1}")]
    ValueExceedsU64Max(String, String),
    #[error("Query error: {0}")]
    QueryError(String),
    #[error("Failed to cancel query: {0}")]
    CancelQueryError(String),
    #[error("Failed to get a value from get_value_callback")]
    GetValueCallBackError,
}
