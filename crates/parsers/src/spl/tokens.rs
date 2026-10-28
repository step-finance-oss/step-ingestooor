use anyhow::{anyhow, Result};
use solana_sdk::pubkey::Pubkey;
use step_ingestooor_sdk::utils::step_account_reader::StepAccountReader;
use step_ingestooor_sdk::utils::token_metadata::{
    get_metadata_extension_data_from_tlv, MetadataExtensionData,
};

use std::collections::HashSet;
use std::sync::mpsc::SyncSender;

use async_trait::async_trait;
use rust_decimal::Decimal;
use solana_sdk::account::ReadableAccount;
use solana_transaction_status::{UiInstruction, UiParsedInstruction};

use crate::state_source::{AccountProvider, StateSource};
use crate::{TransactionParserFilter, TransactionParserTrait};
use step_ingestooor_sdk::dooot::MintInfoDooot;
use step_ingestooor_sdk::utils::spl_ixs::HasAuthority;
use step_ingestooor_sdk::utils::step_utils::StepUtils;
use step_ingestooor_sdk::utils::util_fns::{
    check_token_account_type, convert_blocktime_to_micros, TokenAccountType,
};
use step_ingestooor_sdk::{
    dooot::{
        Dooot, MintSupplyGlobalDooot, TokenBalanceUserDooot, TokenMissingOwnerGlobalDooot,
        TokenTransferEventDooot,
    },
    trait_defs::transaction::TransactionLike,
    utils::{
        constants::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID},
        spl_ixs::TokenParsedInstruction,
        util_fns::{get_program_id_of_ix, get_spl_version_from_program_id},
    },
};

pub struct TokenTransfer;

#[async_trait]
impl TransactionParserTrait for TokenTransfer {
    fn get_name(&self) -> &'static str {
        "Token transfer"
    }
    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![
            TransactionParserFilter::ProgramId(TOKEN_PROGRAM_ID.to_string()),
            TransactionParserFilter::ProgramId(TOKEN_2022_PROGRAM_ID.to_string()),
        ]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let account_keys = txn.get_account_keys();
        //put account keys in a map to check for missing owners
        let all_accts_in_tx: HashSet<String> =
            account_keys.iter().map(|a| a.pubkey.clone()).collect();

        let acct_iter = txn.iter_accounts().filter_map(|a| {
            if !(a.post_owner_program == Some(TOKEN_PROGRAM_ID)
                || a.post_owner_program == Some(TOKEN_2022_PROGRAM_ID)
                || a.pre_owner_program == Some(TOKEN_PROGRAM_ID)
                || a.pre_owner_program == Some(TOKEN_2022_PROGRAM_ID))
            {
                return None;
            }

            Some(a)
        });

        for acct in acct_iter {
            let post_info = acct.unpack_post_datum()?.and_then(|bytes| {
                let owner_program = acct.post_owner_program;
                let acct_type = check_token_account_type(owner_program.unwrap_or(""), &bytes)?;
                Some((acct_type, bytes, owner_program))
            });
            let pre_info = acct.unpack_pre_datum()?.and_then(|bytes| {
                let owner_program = acct.pre_owner_program;
                let acct_type = check_token_account_type(owner_program.unwrap_or(""), &bytes)?;
                Some((acct_type, bytes, owner_program))
            });

            let closed = acct.was_closed;
            let (bytes, acct_type, owner_program) = match (&pre_info, &post_info) {
                (_, Some((acct_type, bytes, owner_program))) => (bytes, acct_type, owner_program),
                (Some((acct_type, bytes, owner_program)), None) => {
                    (bytes, acct_type, owner_program)
                }
                _ => continue,
            };

            if !acct.datum_changed && !closed {
                continue;
            }

            match acct_type {
                TokenAccountType::TokenKegMint
                | TokenAccountType::TokenT22MintWithExtensions
                | TokenAccountType::TokenT22Mint => {
                    if closed {
                        // TODO: Handle closed mint accounts in mint_info tables/dooot
                        continue;
                    }

                    let mint = acct.pubkey;
                    let extracted_metadata = bytes.get(166..).and_then(|tlv_data| {
                        get_metadata_extension_data_from_tlv(tlv_data)?.metadata
                    });

                    if let Some(metadata) = extracted_metadata {
                        // Emit a metadata dooot if data present
                        let mi_dooot = Dooot::MintInfo(MintInfoDooot {
                            time,
                            name: Some(metadata.name),
                            symbol: Some(metadata.symbol),
                            metadata_uri: Some(metadata.uri),
                            mint: mint.to_string(),
                            ..Default::default()
                        });

                        dooot.push(mi_dooot);
                    }

                    let supply = u64::from_le_bytes(bytes[36..44].try_into()?);
                    let Some(owner_program) = owner_program else {
                        log::warn!(
                            "UNREACHABLE - owner_program is None for mint that has acct_type set"
                        );
                        continue;
                    };
                    let spl_version = get_spl_version_from_program_id(owner_program);
                    let supply_dooot = Dooot::MintSupplyGlobal(MintSupplyGlobalDooot {
                        block_time: time,
                        mint: mint.to_string(),
                        total_supply: supply.into(),
                        spl_version,
                    });

                    dooot.push(supply_dooot);
                }
                TokenAccountType::TokenKegTokenAccount
                | TokenAccountType::TokenT22TokenAccountWithExtensions
                | TokenAccountType::TokenT22TokenAccount => {
                    let mut reader = StepAccountReader::new(bytes);
                    let mint = reader.read_next_pubkey()?;
                    let authority = reader.read_next_pubkey()?;
                    let balance = if closed { 0 } else { reader.read_le_u64()? };
                    reader.advance_clamped(8);

                    let balance_dooot = Dooot::TokenBalanceUser(TokenBalanceUserDooot {
                        time,
                        mint_pubkey: mint.to_string(),
                        owner_pubkey: authority.to_string(),
                        token_account_pubkey: acct.pubkey.to_string(),
                        balance: balance.into(),
                        closed,
                    });

                    dooot.push(balance_dooot);

                    // Check if owner changed
                    let old_owner = if let (Some((_, pre_b, _)), Some(_)) = (&pre_info, &post_info)
                    {
                        let mut r_pre = StepAccountReader::new(pre_b);
                        r_pre.read_next_pubkey()?;
                        let pre_auth = r_pre.read_next_pubkey()?;

                        if pre_auth != authority {
                            Some(pre_auth)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    // Owner was changed in this tx
                    if let Some(old_owner) = old_owner {
                        // Emit a closed dooot for this owner
                        let balance_dooot = Dooot::TokenBalanceUser(TokenBalanceUserDooot {
                            time,
                            mint_pubkey: mint.to_string(),
                            owner_pubkey: old_owner.to_string(),
                            token_account_pubkey: acct.pubkey.to_string(),
                            balance: Decimal::ZERO,
                            closed: true,
                        });

                        dooot.push(balance_dooot);
                    }
                }
                _ => continue,
            }
        }

        for ix in txn.get_flattened_ixs() {
            if let UiInstruction::Parsed(UiParsedInstruction::Parsed(parsed)) = ix.instruction {
                if parsed.program_id != TOKEN_PROGRAM_ID
                    && parsed.program_id != TOKEN_2022_PROGRAM_ID
                {
                    continue;
                }

                let des_ix =
                    serde_json::from_value::<TokenParsedInstruction>(parsed.parsed.clone())?;

                // Transfer values
                let amount: Decimal;
                let source: Option<String>;
                let destination: Option<String>;
                let mint: String;
                let parent_program: Option<String>;

                // One off check for new mints, to upsert decimals in db
                if let TokenParsedInstruction::InitializeMint(init_ix) = &des_ix {
                    let new_mint = init_ix.mint.clone();
                    let decimals: i16 = init_ix.decimals.into();
                    let mint_authority = init_ix.mint_authority.clone();
                    let spl_version = get_spl_version_from_program_id(&parsed.program_id);

                    let metadata_row = Dooot::MintInfo(MintInfoDooot {
                        time,
                        mint: new_mint.clone(),
                        symbol: None,
                        name: None,
                        description: None,
                        logouri: None,
                        token_standard: None,
                        decimals: Some(decimals),
                        spl_version: Some(spl_version),
                        mint_authority: Some(mint_authority),
                        metadata_uri: None,
                        metadata_fetch_success: None,
                        metadata_fetch_tries: None,
                        nft_collection_key: None,
                    });

                    //push these to the real dooot vec so they are always first
                    dooot.push(metadata_row);
                }
                if let TokenParsedInstruction::TokenInitializeTokenMetadata(init_ix) = &des_ix {
                    let new_mint = init_ix.mint.clone();
                    let symbol = init_ix.symbol.clone();
                    let name = init_ix.name.clone();
                    let uri = init_ix.uri.clone();
                    let mint_authority = init_ix.mint_authority.clone();
                    let spl_version = get_spl_version_from_program_id(&parsed.program_id);
                    let metadata_row = Dooot::MintInfo(MintInfoDooot {
                        time,
                        mint: new_mint.clone(),
                        symbol: Some(symbol.replace('\0', "")),
                        name: Some(name.replace('\0', "")),
                        description: None,
                        logouri: None,
                        token_standard: None,
                        decimals: None,
                        spl_version: Some(spl_version),
                        mint_authority,
                        metadata_uri: Some(uri.replace('\0', "")),
                        metadata_fetch_success: None,
                        metadata_fetch_tries: None,
                        nft_collection_key: None,
                    });
                    dooot.push(metadata_row);
                }
                if let TokenParsedInstruction::TokenUpdateField(update_ix) = &des_ix {
                    let field = update_ix.field.clone();
                    let mut description = None;
                    let mut name = None;
                    let mut symbol = None;
                    let mut uri = None;
                    // confirm mint is the same as the one in the metadata https://github.com/solana-labs/solana-program-library/blob/master/token/program-2022/src/extension/token_metadata/processor.rs#L61
                    let mint = update_ix.metadata.clone();
                    // can be more keys but for now we only care about these
                    match field.as_str() {
                        "description" => {
                            description = Some(update_ix.value.clone().replace('\0', ""));
                        }
                        "name" => {
                            name = Some(update_ix.value.clone().replace('\0', ""));
                        }
                        "uri" => {
                            uri = Some(update_ix.value.clone().replace('\0', ""));
                        }
                        "symbol" => {
                            symbol = Some(update_ix.value.clone().replace('\0', ""));
                        }
                        _ => {}
                    };

                    let metadata_row = Dooot::MintInfo(MintInfoDooot {
                        time,
                        mint,
                        symbol,
                        name,
                        description,
                        logouri: None,
                        token_standard: None,
                        decimals: None,
                        spl_version: None,
                        mint_authority: None,
                        metadata_uri: uri,
                        metadata_fetch_success: None,
                        metadata_fetch_tries: None,
                        nft_collection_key: None,
                    });
                    dooot.push(metadata_row);
                }

                match &des_ix {
                    TokenParsedInstruction::TokenTransfer(transfer_ix) => {
                        let destination_acct = &transfer_ix.destination;
                        let token_account_data =
                            txn.get_token_account_data_by_key(destination_acct)?;
                        let destination_auth = token_account_data
                            .owner
                            .ok_or_else(|| anyhow!("Token account owner not found"))?;
                        destination = Some(destination_auth);
                        source = Some(transfer_ix.get_authority()?.to_string());
                        amount = transfer_ix.amount.into();
                        mint = token_account_data.mint;
                    }
                    TokenParsedInstruction::TokenTransferChecked(transfer_ix) => {
                        let destination_acct = &transfer_ix.destination;
                        let token_account_data =
                            txn.get_token_account_data_by_key(destination_acct)?;
                        let destination_auth = token_account_data
                            .owner
                            .ok_or_else(|| anyhow!("Token account owner not found"))?;
                        destination = Some(destination_auth);
                        source = Some(transfer_ix.get_authority()?.to_string());
                        amount = transfer_ix.token_amount.amount.parse()?;
                        mint = token_account_data.mint;
                    }
                    TokenParsedInstruction::TokenTransferCheckedWithFee(transfer_ix) => {
                        let destination_acct = &transfer_ix.destination;
                        let token_account_data =
                            txn.get_token_account_data_by_key(destination_acct)?;
                        let destination_auth = token_account_data
                            .owner
                            .ok_or_else(|| anyhow!("Token account owner not found"))?;
                        destination = Some(destination_auth);
                        source = Some(transfer_ix.get_authority()?.to_string());
                        amount = transfer_ix.token_amount.amount.parse()?;
                        mint = token_account_data.mint;
                    }
                    TokenParsedInstruction::TokenMintTo(mint_ix) => {
                        let destination_acct = &mint_ix.account;
                        let token_account_data =
                            txn.get_token_account_data_by_key(destination_acct)?;
                        let destination_auth = token_account_data
                            .owner
                            .ok_or_else(|| anyhow!("Token account owner not found"))?;
                        destination = Some(destination_auth);
                        amount = mint_ix.amount.into();
                        source = None;
                        mint = mint_ix.mint.clone();
                    }
                    TokenParsedInstruction::TokenMintToChecked(mint_checked) => {
                        let destination_acct = &mint_checked.account;
                        let token_account_data =
                            txn.get_token_account_data_by_key(destination_acct)?;
                        let destination_auth = token_account_data
                            .owner
                            .ok_or_else(|| anyhow!("Token account owner not found"))?;
                        destination = Some(destination_auth);
                        amount = mint_checked.token_amount.amount.parse()?;
                        source = None;
                        mint = mint_checked.mint.clone();
                    }
                    TokenParsedInstruction::TokenBurn(burn_ix) => {
                        let source_acct = &burn_ix.account;
                        let token_account_data = txn.get_token_account_data_by_key(source_acct)?;
                        let source_auth = token_account_data
                            .owner
                            .ok_or_else(|| anyhow!("Token account owner not found"))?;
                        source = Some(source_auth);
                        destination = None;
                        amount = burn_ix.amount.into();
                        mint = burn_ix.mint.clone();
                    }
                    TokenParsedInstruction::TokenBurnChecked(burn_ix) => {
                        let source_acct = &burn_ix.account;
                        let token_account_data = txn.get_token_account_data_by_key(source_acct)?;
                        let source_auth = token_account_data
                            .owner
                            .ok_or_else(|| anyhow!("Token account owner not found"))?;
                        source = Some(source_auth);
                        destination = None;
                        amount = burn_ix.token_amount.amount.parse()?;
                        mint = burn_ix.mint.clone();
                    }
                    _ => {
                        continue;
                    }
                }

                // Assign `parent_program` if inner
                if ix.is_inner {
                    let parent_ix = txn.get_outer_instructions().get(ix.index as usize);
                    if let Some(p_ix) = parent_ix {
                        parent_program = get_program_id_of_ix(p_ix);
                    } else {
                        parent_program = None;
                    }
                } else {
                    parent_program = None;
                }

                //if token is sent to a wallet but that wallet is not in the accounts for
                //this transaction, add a dooot row for it in a place that tx history looks
                if let Some(dest_auth) = destination.clone() {
                    if !all_accts_in_tx.contains(&dest_auth) {
                        dooot.push(Dooot::TokenMissingOwner(TokenMissingOwnerGlobalDooot {
                            time,
                            owner_pubkey: dest_auth,
                            signature: txn.get_signature().to_owned(),
                        }))
                    }
                }

                let spl_version = get_spl_version_from_program_id(&parsed.program_id);
                let transfer_dooot_rows = Dooot::TokenTransferEvent(TokenTransferEventDooot {
                    amount,
                    source_pubkey: source.clone(),
                    destination_pubkey: destination.clone(),
                    mint_pubkey: mint.clone(),
                    parent_program,
                    spl_version,
                    tx_id: txn.get_signature().to_owned(),
                    time,
                });

                dooot.push(transfer_dooot_rows);
            }
        }

        Ok(())
    }
}

impl StateSource for TokenTransfer {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        let time = acct_provider.get_block_time();
        let time_stamp = convert_blocktime_to_micros(time, Some(0), None)?;

        // let mut ptr_accounts = HashMap::new();

        let accs_iter = acct_provider.all_accounts().filter_map(|acc| {
            let acc_data = acc.account.data();
            let owner_id = acc.account.owner().to_string();
            if let Some(token_account_type) = check_token_account_type(&owner_id, acc_data) {
                if matches!(
                    token_account_type,
                    TokenAccountType::TokenKegTokenAccount
                        | TokenAccountType::TokenT22TokenAccount
                        | TokenAccountType::TokenT22TokenAccountWithExtensions
                        | TokenAccountType::TokenKegMint
                        | TokenAccountType::TokenT22Mint
                        | TokenAccountType::TokenT22MintWithExtensions
                ) {
                    return Some((acc, token_account_type));
                }
            }
            None
        });

        // process accounts in parallel
        for (a, acct_type) in accs_iter {
            let account_pubkey = a.pubkey.to_string();
            let acc_data = a.account.data();
            let spl_version = get_spl_version_from_program_id(&a.account.owner().to_string());

            match acct_type {
                TokenAccountType::TokenKegTokenAccount
                | TokenAccountType::TokenT22TokenAccount
                | TokenAccountType::TokenT22TokenAccountWithExtensions => {
                    let balance: u64 = u64::from_le_bytes(acc_data[64..72].try_into()?);
                    if balance == 0 {
                        continue;
                    }

                    let balance_dooot_rows = Dooot::TokenBalanceUser(TokenBalanceUserDooot {
                        balance: Decimal::from(balance),
                        time: time_stamp,
                        mint_pubkey: Pubkey::try_from(&acc_data[0..32])?.to_string(),
                        owner_pubkey: Pubkey::try_from(&acc_data[32..64])?.to_string(),
                        token_account_pubkey: account_pubkey.clone(),
                        closed: false,
                    });
                    tx.send(balance_dooot_rows)?;
                }
                TokenAccountType::TokenKegMint
                | TokenAccountType::TokenT22Mint
                | TokenAccountType::TokenT22MintWithExtensions => {
                    if matches!(acct_type, TokenAccountType::TokenT22MintWithExtensions) {
                        // Get extensions data from tlv data
                        if let Some(tlv_data) = acc_data.get(166..) {
                            if let Some(MetadataExtensionData {
                                pointer_address,
                                metadata,
                            }) = get_metadata_extension_data_from_tlv(tlv_data)
                            {
                                match (metadata, pointer_address) {
                                    (Some(metadata), _) => {
                                        // Just use metadata
                                        let mi_dooot = Dooot::MintInfo(MintInfoDooot {
                                            time: time_stamp,
                                            mint: account_pubkey.clone(),
                                            metadata_uri: Some(metadata.uri),
                                            name: Some(metadata.name),
                                            symbol: Some(metadata.symbol),
                                            ..Default::default()
                                        });

                                        tx.send(mi_dooot)?;
                                    }
                                    (None, Some(_ptr_addr)) => {
                                        // For now do nothing
                                        // if ptr_addr == a.pubkey {
                                        //     // We should already handle mints that point to themselves above.
                                        //     continue;
                                        // }

                                        // ptr_accounts.insert(ptr_addr, a.pubkey);
                                    }
                                    (None, None) => {
                                        // Do nothing
                                    }
                                }
                            }
                        }
                    }
                    // Emit a dooot for decimals
                    let decimals = acc_data[0x2c];

                    let mi_dooot = Dooot::MintInfo(MintInfoDooot {
                        time: time_stamp,
                        mint: account_pubkey.clone(),
                        decimals: Some(decimals as i16),
                        ..Default::default()
                    });

                    tx.send(mi_dooot)?;

                    // Emit a dooot for supply
                    let supply = u64::from_le_bytes(acc_data[36..44].try_into()?);

                    let supply_row = Dooot::MintSupplyGlobal(MintSupplyGlobalDooot {
                        block_time: time_stamp,
                        mint: account_pubkey.clone(),
                        spl_version,
                        total_supply: supply.into(),
                    });

                    tx.send(supply_row)?;
                }
                _ => continue,
            }
        }

        // TODO: Do iteration over ptr accounts,
        // But we may not need for now
        // for acct in acct_provider.all_accounts() {
        //     let pk = acct.pubkey;
        //     if !ptr_accounts.contains_key(&pk) {
        //         continue;
        //     }
        //     let data = acct.account.data();
        // }

        Ok(())
    }

    fn get_name(&self) -> &'static str {
        "TokenTransfer"
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use std::{fs, rc::Rc};
    use step_ingestooor_sdk::{
        decoded_transaction::DecodedTransaction, utils::step_utils::StepUtils,
    };

    use super::*;
    use crate::TokenTransfer;
    #[tokio::test]
    async fn update_field() {
        let message_json = fs::read_to_string("./tests/spl/update-field.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));

        let mut dooot = vec![];
        let parser = TokenTransfer;

        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        // 4 total dooots
        // 3 mint metadata rows, 1 initalize mint for dec, 1 for metadata, 1 for update description
        // 1 supply change for mint (init from None to Some(0))
        assert_eq!(dooot.len(), 5);
    }

    #[tokio::test]
    async fn invoke_withdraw_confidential_transfer_deser() {
        let message_json = fs::read_to_string("./tests/spl/withdraw-conf.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));

        let mut dooot = vec![];
        let parser = TokenTransfer;

        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_confidential_transfer_deser() {
        let message_json = fs::read_to_string("./tests/spl/confidentialTransfer.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));

        let mut dooot = vec![];
        let parser = TokenTransfer;

        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn init_confidential_fee_config_deser() {
        let message_json =
            fs::read_to_string("./tests/spl/init-confidential-fee-config.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));

        let mut dooot = vec![];
        let parser = TokenTransfer;

        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn close_token_account() {
        let message_json = fs::read_to_string("./tests/spl/close-token-account.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));

        let mut dooot = vec![];
        let parser = TokenTransfer;

        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        let dooot = dooot.first().unwrap();

        let Dooot::TokenBalanceUser(dooot) = dooot else {
            panic!("dooot is not TokenBalanceUser");
        };

        assert_eq!(
            dooot.token_account_pubkey,
            "J8eNuZb3AYouz3piXhZfvt42yCNotUxyP5UaMsAx49J6"
        );
        assert_eq!(
            dooot.owner_pubkey,
            "qUeL7JzC52V1DvvPkqnMd74QjThWtSJY5G1PkKv1ur7"
        );
        assert!(dooot.closed);
    }

    #[tokio::test]
    async fn new_authority() {
        let message_json = fs::read_to_string("./tests/spl/new_authority.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));

        let mut dooot = vec![];
        let parser = TokenTransfer;

        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        let mut dooot_iter = dooot.iter();
        let dooot = dooot_iter.next().unwrap();

        let Dooot::TokenBalanceUser(dooot) = dooot else {
            panic!("dooot is not TokenBalanceUser");
        };

        assert_eq!(
            dooot.token_account_pubkey,
            "8wVgcpt221DSqE6pybjEc35hwUZoGR1dULr9FsDWNKBg"
        );
        assert_eq!(
            dooot.owner_pubkey,
            "qUeL7JzC52V1DvvPkqnMd74QjThWtSJY5G1PkKv1ur7"
        );
        assert!(!dooot.closed);

        let closed_owner_dooot = dooot_iter.next().unwrap();
        let Dooot::TokenBalanceUser(closed_owner_dooot) = closed_owner_dooot else {
            panic!("dooot is not TokenBalanceUser");
        };

        assert_eq!(
            closed_owner_dooot.token_account_pubkey,
            dooot.token_account_pubkey
        );
        assert_eq!(
            closed_owner_dooot.owner_pubkey,
            "ASXSjnpNE4cTGnmJRZkJBwTsRRRgUEbr1vWnPT8iCVsU"
        );
        assert!(closed_owner_dooot.closed);
    }

    #[tokio::test]
    async fn test_empty_datum() {
        let message_json = fs::read_to_string("./tests/spl/token-transfer-failed.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));

        let mut dooot = vec![];
        let parser = TokenTransfer;

        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
    }
}
