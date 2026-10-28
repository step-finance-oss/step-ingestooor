use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_common_rust::evalexpr::ValueWrapper;
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(Serialize, Deserialize, Debug, Clone, Default, TS)]
pub struct VaultInfoUnderlyingMintVault {
    pub mint: String,
    pub vault: String,
}

impl From<VaultInfoUnderlyingMintVault> for ValueWrapper {
    fn from(value: VaultInfoUnderlyingMintVault) -> Self {
        format!("{}:{}", value.mint, value.vault).into()
    }
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "VaultInfo")]
#[clickhouse(
    table = "incoming_vault_info_dooot",
    queue = "vault_info_dooot",
    flush_ms = 5000
)]
pub struct VaultInfoDooot {
    #[serde(
        serialize_with = "crate::serde::ch_naive_date_time::serialize_ch_naive_date_time",
        deserialize_with = "crate::serde::ch_naive_date_time::deserialize_ch_naive_date_time"
    )]
    pub time: NaiveDateTime,
    pub vault_pubkey: String,
    pub program_pubkey: String,
    /// Some vaults give you a token to represent your deposit. (e.g. Kamino)
    #[ts(optional)]
    pub vault_mint: Option<String>,
    pub deposit_tokens: Vec<VaultInfoUnderlyingMintVault>,
    pub reward_tokens: Vec<VaultInfoUnderlyingMintVault>,
    pub name: Option<String>,
}
