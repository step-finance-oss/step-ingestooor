pub struct TSCacheSql;

impl TSCacheSql {
    pub fn get_with_max_age_and_ref_time(max_age: u64) -> String {
        format!(
            "SELECT cache_value FROM indexer.timeseries_cache
             WHERE cache_key = $1 AND $2 - as_of < interval '{} seconds'
             ORDER BY as_of DESC
             LIMIT 1;",
            max_age
        )
    }

    pub const GET_WITH_REF_TIME: &'static str = "SELECT cache_value FROM indexer.timeseries_cache
         WHERE cache_key = $1 and as_of <= $2
         ORDER BY as_of DESC
         LIMIT 1;";

    pub const GET_LATEST: &'static str = "SELECT cache_value FROM indexer.timeseries_cache
         WHERE cache_key = $1
         ORDER BY as_of DESC
         LIMIT 1;";

    pub const BASIC_SET: &'static str =
        "INSERT INTO indexer.timeseries_cache (cache_key, cache_value, as_of)
         VALUES ($1, $2, $3)
         ON CONFLICT (cache_key) 
         DO UPDATE SET 
           cache_value = EXCLUDED.cache_value, 
           as_of = EXCLUDED.as_of;";

    pub const SET_WITH_AS_OF: &'static str =
        "INSERT INTO indexer.timeseries_cache (cache_key, cache_value, as_of)
         VALUES ($1, $2, $3)
         ON CONFLICT (cache_key) 
         DO UPDATE SET 
           cache_value = EXCLUDED.cache_value,
           as_of = EXCLUDED.as_of
         WHERE timeseries_cache.as_of < EXCLUDED.as_of;";

    pub const DEL: &'static str = "DELETE FROM indexer.timeseries_cache WHERE cache_key = $1;";

    pub const GET_MINT_DECIMALS: &'static str =
        "SELECT decimals FROM indexer.lookup_mint WHERE mint = $1";

    pub const GET_SUPPLY: &'static str = "SELECT tsm.total_supply 
            FROM indexer.time_mint_supply tsm
            INNER JOIN indexer.lookup_mint lm ON tsm.mint_id = lm.mint_id
            WHERE lm.mint = $1 AND tsm.time <= $2 
            ORDER BY tsm.time DESC 
            LIMIT 1;";

    pub const GET_TOKEN_ACCOUNT_BALANCE: &'static str = "WITH old_count AS (
        SELECT ttb.balance 
        FROM indexer.time_token_balances ttb 
        INNER JOIN indexer.lookup_account la ON la.account_id = ttb.token_account_id
        WHERE la.account_pubkey = $1 and ttb.time <= $2
        ORDER BY ttb.time desc 
        LIMIT 1
    ), new_count AS (
        SELECT ttb.balance 
        FROM indexer.time_token_balances_bigint ttb 
        LEFT JOIN indexer.lookup_account la ON la.account_id = ttb.token_account_id AND la.account_pubkey = $1
        LEFT JOIN indexer.lookup_account_bigint lab ON lab.account_id = ttb.token_account_id AND lab.account_pubkey = $1
        WHERE ttb.time <= $2
        ORDER BY ttb.time desc 
        LIMIT 1
    )
    SELECT COALESCE(
        (SELECT * FROM new_count),
        (SELECT * FROM old_count)
    ) as balance";
}
