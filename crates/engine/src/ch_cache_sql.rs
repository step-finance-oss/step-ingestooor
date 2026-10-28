pub struct ChCacheSql;

impl ChCacheSql {
    pub const GET_WITH_MAX_AGE_AND_REF_TIME: &'static str =
        "SELECT * from vw_cache_get_max_age_and_ref_time(cache_key = ?, reference_time = ?, max_age = ?)";

    pub const GET_WITH_REF_TIME: &'static str =
        "SELECT * from vw_cache_get_ref_time(cache_key = ?, reference_time = ?)";

    pub const GET_LATEST: &'static str = "SELECT * from vw_cache_get_latest(cache_key = ?)";

    pub const GET_MINT_DECIMALS: &'static str = "SELECT * from vw_get_mint_decimals(mint = ?)";

    pub const GET_SUPPLY: &'static str =
        "SELECT * from vw_global_time_mint_supply(mint_pubkey = ?, time = ?)";

    pub const GET_TOKEN_ACCOUNT_BALANCE: &'static str =
        "SELECT * from vw_global_time_token_balance(token_account_pubkey = ?, time = ?)";

    pub const BASIC_SET: &'static str =
        "INSERT INTO timeseries_cache (cache_key, cache_value, as_of) VALUES (?, ?, ?)";

    pub const SET_WITH_AS_OF: &'static str =
        "INSERT INTO timeseries_cache (cache_key, cache_value, as_of)
         VALUES (?, ?, ?);";

    pub const DEL: &'static str = "DELETE FROM timeseries_cache WHERE cache_key = ?;";
}
