use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;

use borsh::{maybestd::io::Error as BorshError, BorshDeserialize};
use itertools::Itertools;
use mpl_token_metadata::{
    accounts::Metadata,
    types::{Collection, CollectionDetails, Data, Key, ProgrammableConfig, TokenStandard, Uses},
};
use reqwest::{Client, Url};
use rust_decimal::Decimal;
use solana_sdk::account::ReadableAccount;
use spl_token::solana_program::pubkey::Pubkey;
use step_ingestooor_sdk::{
    dooot::{Dooot, MintInfoDooot, MintSupplyGlobalDooot},
    trait_defs::transaction::TransactionLike,
    utils::{
        constants::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID},
        step_utils::StepUtils,
        token_metadata::{trim_padding_bytes, TokenJSONMetadata},
        util_fns::{
            check_token_account_type, convert_blocktime_to_micros, get_spl_version_from_program_id,
            unpack_mint_any_size, TokenAccountType,
        },
    },
};

use crate::{state_source::StateSource, TransactionParserFilter, TransactionParserTrait};

pub struct MetaplexTokenMetadata;

const MPL_TOKEN_METADATA_PROGRAM: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";
const TOKEN_2022_METADATA_PROGRAM: &str = "META4s4fSmpkTbZoUsgC1oBnWB31vQcmnN8giPw51Zu";
const METADATA_FETCH_TIMEOUT_MS: u64 = 15000;

#[async_trait]
impl TransactionParserTrait for MetaplexTokenMetadata {
    fn get_name(&self) -> &'static str {
        "Metaplex Token Metadata"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![
            TransactionParserFilter::ProgramId(MPL_TOKEN_METADATA_PROGRAM.to_string()),
            TransactionParserFilter::ProgramId(TOKEN_2022_METADATA_PROGRAM.to_string()),
        ]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let changed_accounts = txn
            .iter_accounts()
            .filter(|a| {
                a.datum_changed
                    && (a.post_owner_program == Some(MPL_TOKEN_METADATA_PROGRAM)
                        || a.post_owner_program == Some(TOKEN_2022_METADATA_PROGRAM))
            })
            .collect_vec();

        let http_client = &step_utils.json_metadata_http_client;
        for acct in changed_accounts {
            let Some(post_data) = acct.unpack_post_datum()? else {
                continue;
            };

            let Some(key_byte) = post_data.first() else {
                // Data is empty and therefore uninitialized, skip
                continue;
            };

            if *key_byte != mpl_token_metadata::types::Key::MetadataV1 as u8 {
                // Not a valid metadata account, skip
                continue;
            }

            let Ok(metadata_account) =
                MetaplexTokenMetadata::meta_deser_unchecked(&mut &post_data[..])
            else {
                // Corrupted metadata, skip
                log::warn!("Possibly corrupted metadata at addr {}", acct.pubkey);
                continue;
            };

            let mint = metadata_account.mint.to_string();
            let on_chain_name_trimmed = trim_padding_bytes(&metadata_account.name);
            let on_chain_name = Self::none_if_empty(Some(on_chain_name_trimmed));
            let on_chain_symbol_trimmed = trim_padding_bytes(&metadata_account.symbol);
            let on_chain_symbol = Self::none_if_empty(Some(on_chain_symbol_trimmed));
            let token_standard = metadata_account.token_standard.map(|s| s as i16);
            let uri = trim_padding_bytes(&metadata_account.uri);

            let fetch_result = Self::fetch_json_metadata(&uri, http_client, step_utils).await;
            let fetch_success = fetch_result.is_some();
            let json_metadata = fetch_result.unwrap_or_default().as_trimmed();

            let logouri = json_metadata.image;
            let description = json_metadata.description;
            let off_chain_name = Self::none_if_empty(json_metadata.name);
            let off_chain_symbol = Self::none_if_empty(json_metadata.symbol);

            let name = off_chain_name.or(on_chain_name);
            let symbol = off_chain_symbol.or(on_chain_symbol);

            // Get info from mint account
            let mut decimals = None;
            let mut spl_version = None;
            let mut mint_authority = None;
            if let Ok(Some(mint_data)) = txn.get_account_data_after(&mint) {
                let mint_acct = unpack_mint_any_size(&mint_data)?;
                decimals = Some(mint_acct.decimals as i16);
                mint_authority = mint_acct.mint_authority.map(|a| a.to_string()).into();
                if let Some(owner) = txn.get_account_owner(&mint) {
                    spl_version = get_spl_version_from_program_id(owner).into();
                }
            }

            let metadata_fetch_success = if fetch_success { Some(time) } else { None };

            let nft_collection_key = Self::extract_collection_key(&metadata_account);

            let metadata_dooot = Dooot::MintInfo(MintInfoDooot {
                time,
                mint,
                name,
                symbol,
                description,
                logouri,
                token_standard,
                decimals,
                spl_version,
                mint_authority,
                metadata_uri: Some(uri),
                metadata_fetch_success,
                metadata_fetch_tries: None,
                nft_collection_key,
            });

            dooot.push(metadata_dooot);
        }

        Ok(())
    }
}

impl MetaplexTokenMetadata {
    #[inline]
    pub fn extract_collection_key(meta: &Metadata) -> Option<String> {
        meta.collection.as_ref().and_then(|c| {
            if c.verified {
                Some(c.key.to_string())
            } else {
                None
            }
        })
    }

    #[inline]
    pub fn none_if_empty(string: Option<String>) -> Option<String> {
        match string {
            Some(s) if s.trim().is_empty() => None,
            other => other,
        }
    }

    /// See [the MPL Token Metadata Program](https://github.com/metaplex-foundation/mpl-token-metadata/blob/main/programs/token-metadata/program/src/utils/mod.rs#L243-L251)
    /// and [the client library](https://github.com/metaplex-foundation/mpl-token-metadata/blob/e86de64101fc386dd4cc97b6f107da3de258833a/clients/rust/src/traits.rs#L81)
    #[inline]
    pub fn metadata_is_empty_or_uninitialized(data: &[u8]) -> bool {
        data.is_empty() || data[0] == mpl_token_metadata::types::Key::Uninitialized as u8
    }

    #[inline]
    pub async fn fetch_json_metadata(
        uri: &str,
        client: &Client,
        step_utils: &StepUtils,
    ) -> Option<TokenJSONMetadata> {
        let transformed_uri = step_utils.transform_ipfs_url(uri);
        // Use the transformed URI if it exists, else use the original URI
        let uri_to_use = transformed_uri.unwrap_or(uri.to_string());

        let Ok(parsed_uri) = Url::parse(&uri_to_use) else {
            let counter = metrics::counter!("MetaplexMetadataFetchError", "Domain" => "InvalidUri", "ErrorType" => "ParseError");
            counter.increment(1);
            return None;
        };
        let base_uri = format!("{}://{}", parsed_uri.scheme(), parsed_uri.host_str()?);

        let resp = match client
            .get(&uri_to_use)
            .timeout(Duration::from_millis(METADATA_FETCH_TIMEOUT_MS))
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                // log::error!(
                //     "Error fetching metadata from uri {}: {:?} original uri: {}",
                //     uri_to_use,
                //     e,
                //     uri
                // );
                // Something went wrong with the request itself
                let error_type = if e.is_request() {
                    "RequestError"
                } else if e.is_connect() {
                    "ConnectError"
                } else if e.is_timeout() {
                    "TimeoutError"
                } else {
                    "OtherError"
                };
                let counter = metrics::counter!("MetaplexMetadataFetchError", "Domain" => base_uri, "ErrorType" => error_type);
                counter.increment(1);
                return None;
            }
        };

        match resp.json::<TokenJSONMetadata>().await {
            Ok(metadata) => Some(metadata),
            Err(e) => {
                // log::error!(
                //     "Error fetching metadata from uri {}: {:?} original uri: {}",
                //     uri_to_use,
                //     e,
                //     uri
                // );
                // Something else went wrong
                let error_type = if e.is_decode() {
                    "DecodeError"
                } else if e.is_status() {
                    "StatusError"
                } else {
                    "OtherError"
                };
                let counter = metrics::counter!("MetaplexMetadataFetchError", "Domain" => base_uri, "ErrorType" => error_type);
                counter.increment(1);
                None
            }
        }
    }

    // ripped from https://github.com/metaplex-foundation/mpl-token-metadata/blob/main/programs/token-metadata/program/src/utils/metadata.rs#L205

    // Custom deserialization function to handle NFTs with corrupted data.
    // This function is used in a custom deserialization implementation for the
    // `Metadata` struct, so should never have `msg` macros used in it as it may be used client side
    // either in tests or client code.
    //
    // It does not check `Key` type or account length and should only be used through the custom functions
    // `from_account_info` and `deserialize` implemented on the Metadata struct.
    pub fn meta_deser_unchecked(buf: &mut &[u8]) -> Result<Metadata, BorshError> {
        // Metadata corruption shouldn't appear until after edition_nonce.
        let key: Key = BorshDeserialize::deserialize(buf)?;
        let update_authority: Pubkey = BorshDeserialize::deserialize(buf)?;
        let mint: Pubkey = BorshDeserialize::deserialize(buf)?;
        let data: Data = BorshDeserialize::deserialize(buf)?;
        let primary_sale_happened: bool = BorshDeserialize::deserialize(buf)?;
        let is_mutable: bool = BorshDeserialize::deserialize(buf)?;
        let edition_nonce: Option<u8> = BorshDeserialize::deserialize(buf)?;

        // V1.2
        let token_standard_res: Result<Option<TokenStandard>, BorshError> =
            BorshDeserialize::deserialize(buf);
        let collection_res: Result<Option<Collection>, BorshError> =
            BorshDeserialize::deserialize(buf);
        let uses_res: Result<Option<Uses>, BorshError> = BorshDeserialize::deserialize(buf);

        // V1.3
        let collection_details_res: Result<Option<CollectionDetails>, BorshError> =
            BorshDeserialize::deserialize(buf);

        // pNFT - Programmable Config
        let programmable_config_res: Result<Option<ProgrammableConfig>, BorshError> =
            BorshDeserialize::deserialize(buf);

        // We can have accidentally valid, but corrupted data, particularly on the Collection struct,
        // so to increase probability of catching errors. If any of these deserializations fail, set
        // all values to None.
        let (token_standard, collection, uses) =
            match (token_standard_res, collection_res, uses_res) {
                (Ok(token_standard_res), Ok(collection_res), Ok(uses_res)) => {
                    (token_standard_res, collection_res, uses_res)
                }
                _ => (None, None, None),
            };

        // V1.3
        let collection_details = collection_details_res.unwrap_or_default();

        // Programmable Config
        let programmable_config = programmable_config_res.unwrap_or(None);

        let metadata = Metadata {
            key,
            update_authority,
            mint,
            name: data.name,
            symbol: data.symbol,
            uri: data.uri,
            seller_fee_basis_points: data.seller_fee_basis_points,
            creators: data.creators,
            primary_sale_happened,
            is_mutable,
            edition_nonce,
            token_standard,
            collection,
            uses,
            collection_details,
            programmable_config,
        };

        Ok(metadata)
    }
}

impl StateSource for MetaplexTokenMetadata {
    fn get_name(&self) -> &'static str {
        "Metaplex Token Metadata"
    }
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        let block_time = acct_provider.get_block_time();
        let converted_bt_time = convert_blocktime_to_micros(block_time, Some(0), None)?;

        for acc in acct_provider.all_accounts() {
            let owner = acc.account.owner().to_string();

            if owner != TOKEN_PROGRAM_ID
                && owner != TOKEN_2022_PROGRAM_ID
                && owner != MPL_TOKEN_METADATA_PROGRAM
                && owner != TOKEN_2022_METADATA_PROGRAM
            {
                continue;
            }

            if owner == TOKEN_PROGRAM_ID || owner == TOKEN_2022_PROGRAM_ID {
                let data = acc.account.data();
                let token_account = check_token_account_type(&owner, data);
                let Some(token_account) = token_account else {
                    continue;
                };
                if token_account == TokenAccountType::TokenKegMint
                    || token_account == TokenAccountType::TokenT22Mint
                    || token_account == TokenAccountType::TokenT22MintWithExtensions
                {
                    let mint = acc.pubkey.to_string();
                    let supply = u64::from_le_bytes(data[36..44].try_into()?);
                    let mint_authority =
                        Pubkey::new_from_array(data[4..36].try_into()?).to_string();
                    let decimals = data[44] as i16;
                    let spl_version = get_spl_version_from_program_id(&owner);

                    let dec_supply: Decimal = supply.into();
                    let supply = MintSupplyGlobalDooot {
                        block_time: converted_bt_time,
                        mint: mint.clone(),
                        total_supply: dec_supply,
                        spl_version,
                    };
                    // Submit a dooot here just for decimals?
                    // Will have another dooot for same mint if it has metadata
                    let mint_metadata = MintInfoDooot {
                        time: converted_bt_time,
                        mint,
                        decimals: Some(decimals),
                        spl_version: Some(spl_version),
                        mint_authority: Some(mint_authority),
                        ..Default::default()
                    };

                    tx.send(Dooot::MintSupplyGlobal(supply))?;
                    tx.send(Dooot::MintInfo(mint_metadata))?;
                }
            } else if owner == MPL_TOKEN_METADATA_PROGRAM || owner == TOKEN_2022_METADATA_PROGRAM {
                let mut data = acc.account.data();
                //04 is MetadataV1
                if data.first().cloned() == Some(Key::MetadataV1 as u8) {
                    let mint = Pubkey::new_from_array(data[33..65].try_into()?).to_string();
                    let Ok(data) = MetaplexTokenMetadata::meta_deser_unchecked(&mut data) else {
                        // Corrupted metadata. Even fails meta_deser_unchecked. so skipping this
                        log::warn!("Corrupted metadata for mint: {}", mint);
                        continue;
                    };
                    let name = Some(data.name.replace('\0', ""));
                    let symbol = Some(data.symbol.replace('\0', ""));
                    let token_standard = data.token_standard.map(|s| s as i16);
                    let metadata_uri = data.uri.replace('\0', "");
                    let nft_collection_key = MetaplexTokenMetadata::extract_collection_key(&data);

                    let metadata = MintInfoDooot {
                        time: converted_bt_time,
                        mint,
                        name,
                        symbol,
                        decimals: None,
                        description: None,
                        logouri: None,
                        token_standard,
                        mint_authority: None,
                        spl_version: Some(get_spl_version_from_program_id(&owner)),
                        metadata_uri: Some(metadata_uri),
                        metadata_fetch_success: None,
                        metadata_fetch_tries: None,
                        nft_collection_key,
                    };

                    tx.send(Dooot::MintInfo(metadata))?;
                }
            }
        }

        Ok(())
    }
}

//test deserialization
#[cfg(test)]
mod tests {
    use base64::Engine;

    use super::*;

    #[test]
    fn test_deserialize() {
        let data = "KLUv/WCnAUULAAQVBNPOEbAXvBmYTRWHBKfHJEgFy9MiuR7QuaDPbDRUbr+saIr7fVrigUksOOpmXWJIEYXOacqnS/yD3AsPO/PnQd0gAAAARmFuY3kgRnJlbmNoaWVzICM4NTQ5AArIAAAAaHR0cHM6Ly9hcndlYXZlLm5ldC9qUEszN1VpOXJucXo2VGJOUDV2eEFnYkhCXzRMY0NJMGNNUkQ1UkNBSkZz6AMBAwAAAAwv5WLhv0DtDHKUkuX4HGIu0pE3Wqh6OYpx+7rrcYSSAQC15Jc1bMMF0kV2IszBkRzKesRvn2VRaD/5yyy1Km+I9AAyfDX8ATB6/6vFweBiZgtsqi59LcZxuQARffYamStnlM0AMgEBAf0AAACPWd1NatUjSGEWw8ARQhemWuzfD1V8KV2iABZ0HhIe/zy45PYFFnksWYhAAQcg2JiirKIIJFA1ENrXSgALBwAtoCB2hEx4nmLwAjSh1I6ybNtAFA==";
        let data = base64::engine::general_purpose::STANDARD
            .decode(data)
            .unwrap();
        let data = zstd::decode_all(&*data).unwrap();
        MetaplexTokenMetadata::meta_deser_unchecked(&mut &*data).unwrap();
    }

    #[tokio::test]
    async fn remora_metadata() {
        let url = "https://remora-public.s3.us-east-2.amazonaws.com/solana/tokens/nvdar.json";
        let client = reqwest::Client::new();
        let utils = StepUtils::default();
        let res = MetaplexTokenMetadata::fetch_json_metadata(url, &client, &utils)
            .await
            .unwrap();
        assert!(res.image.is_some());
    }
}
