use std::fs;

use anyhow::{anyhow, Result};

use chrono::NaiveDateTime;
use indexer_rabbitmq::geyser::{Message, TransactionNotify};
use serde::{Deserialize, Deserializer, Serialize};

use serde_json::Value;
use solana_transaction_status::{
    option_serializer::OptionSerializer, parse_accounts::ParsedAccount,
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction,
    EncodedTransactionWithStatusMeta, UiInnerInstructions, UiInstruction, UiMessage,
    UiParsedInstruction,
};

use crate::{
    trait_defs::transaction::TransactionLike, utils::util_fns::convert_blocktime_to_micros,
};

pub type InnerMeta = solana_transaction_status::UiTransactionStatusMeta;

#[derive(Serialize, Deserialize, Debug)]
pub struct DecodedTransaction {
    #[serde(
        deserialize_with = "deserialize_naive_datetime_from_i64",
        serialize_with = "serialize_naive_datetime_to_i64"
    )]
    block_time: NaiveDateTime,
    slot: u64,
    meta: InnerMeta,
    instructions: Vec<UiInstruction>,
    accounts: Vec<ParsedAccount>,
    signature: String,
}

pub trait ProgramAndAccountIdsProvider {
    fn get_program_ids(&self) -> Vec<&str>;
    fn get_account_ids(&self) -> Vec<&str>;
}

impl ProgramAndAccountIdsProvider for DecodedTransaction {
    fn get_program_ids(&self) -> Vec<&str> {
        self.get_flattened_ixs()
            .map(|a| match a.instruction {
                UiInstruction::Compiled(ix) => {
                    self.accounts[ix.program_id_index as usize].pubkey.as_str()
                }
                UiInstruction::Parsed(ix) => match ix {
                    UiParsedInstruction::Parsed(ix) => ix.program_id.as_str(),
                    UiParsedInstruction::PartiallyDecoded(ix) => ix.program_id.as_str(),
                },
            })
            .collect()
    }

    fn get_account_ids(&self) -> Vec<&str> {
        self.accounts.iter().map(|a| a.pubkey.as_str()).collect()
    }
}

impl TransactionLike for DecodedTransaction {
    #[inline]
    fn get_block_time(&self) -> NaiveDateTime {
        self.block_time
    }

    #[inline]
    fn get_slot(&self) -> u64 {
        self.slot
    }

    #[inline]
    fn get_signature(&self) -> &str {
        &self.signature
    }

    #[inline]
    fn get_outer_instructions(&self) -> &Vec<UiInstruction> {
        &self.instructions
    }

    #[inline]
    fn get_inner_instructions(&self) -> &OptionSerializer<Vec<UiInnerInstructions>> {
        &self.meta.inner_instructions
    }

    #[inline]
    fn get_account_keys(&self) -> &Vec<ParsedAccount> {
        &self.accounts
    }

    #[inline]
    fn get_pre_token_balances(
        &self,
    ) -> &OptionSerializer<Vec<solana_transaction_status::UiTransactionTokenBalance>> {
        &self.meta.pre_token_balances
    }

    #[inline]
    fn get_post_token_balances(
        &self,
    ) -> &OptionSerializer<Vec<solana_transaction_status::UiTransactionTokenBalance>> {
        &self.meta.post_token_balances
    }

    #[inline]
    fn get_post_balances(&self) -> &Vec<u64> {
        &self.meta.post_balances
    }

    #[inline]
    fn get_pre_balances(&self) -> &Vec<u64> {
        &self.meta.pre_balances
    }

    #[inline]
    fn get_pre_datums(&self) -> &Option<Vec<Option<String>>> {
        &self.meta.pre_datum
    }

    #[inline]
    fn get_post_datums(&self) -> &Option<Vec<Option<String>>> {
        &self.meta.post_datum
    }

    #[inline]
    fn get_logs(&self) -> &OptionSerializer<Vec<String>> {
        &self.meta.log_messages
    }

    #[inline]
    fn get_pre_owners(&self) -> &Option<Vec<Option<String>>> {
        &self.meta.pre_owners
    }

    #[inline]
    fn get_post_owners(&self) -> &Option<Vec<Option<String>>> {
        &self.meta.post_owners
    }
}

impl DecodedTransaction {
    pub fn load_rabbit_message_from_file(path: &str) -> Result<Self> {
        let file_b = fs::read(path)?;
        let json = serde_json::from_slice(&file_b)?;
        Self::from_rabbit_message_json(json)
    }

    pub fn from_rabbit_message_json(json: Value) -> Result<Self> {
        let Message::TransactionNotify(msg) = serde_json::from_value::<Message>(json.clone())?
        else {
            return Err(anyhow!("Invalid message type"));
        };

        let TransactionNotify {
            transaction:
                EncodedConfirmedTransactionWithStatusMeta {
                    slot,
                    block_time,
                    transaction:
                        EncodedTransactionWithStatusMeta {
                            version: _,
                            transaction,
                            meta,
                        },
                    index_in_block,
                    slot_second_idx,
                },
        } = *msg;

        let block_time = block_time.ok_or(anyhow!("tx has no block time"))?;
        let converted_bt_micros =
            convert_blocktime_to_micros(block_time, slot_second_idx, Some(index_in_block))?;
        let meta = meta.ok_or(anyhow!("tx has no meta"))?;
        let decoded_transaction =
            standardized_decoded_transaction(converted_bt_micros, transaction, meta, slot)?;
        Ok(decoded_transaction)
    }
}

pub fn standardized_decoded_transaction(
    block_time: NaiveDateTime,
    transaction: EncodedTransaction,
    meta: InnerMeta,
    slot: u64,
) -> Result<DecodedTransaction> {
    let decoded_transaction: DecodedTransaction = match transaction {
        EncodedTransaction::Json(ui_tx) => match ui_tx.message {
            UiMessage::Parsed(parsed) => {
                // Get all accounts
                let accounts = parsed.account_keys;

                let signature = ui_tx
                    .signatures
                    .first()
                    .ok_or_else(|| anyhow!("No signature found for txn"))?
                    .to_string();

                DecodedTransaction {
                    meta,
                    slot,
                    instructions: parsed.instructions,
                    accounts,
                    signature,
                    block_time,
                }
            }

            UiMessage::Raw(_raw) => {
                // log::warn!("ignoring UiMessage::Raw");
                return Err(anyhow!("ignoring UiMessage::Raw"));
            }
        },

        tx_type => {
            // log::warn!("Ignoring non-JSON transaction type: {:?}", tx_type);
            return Err(anyhow!("Ignoring non-JSON transaction type: {:?}", tx_type));
        }
    };
    Ok(decoded_transaction)
}

// Custom deserializer for NaiveDateTime from i64. Used during tests
fn deserialize_naive_datetime_from_i64<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let block_time = i64::deserialize(deserializer)?;
    let convert_bt_micros = convert_blocktime_to_micros(block_time, Some(0), None)
        .map_err(|e| serde::de::Error::custom(format!("Failed to convert block time: {}", e)))?;
    Ok(convert_bt_micros)
}

fn serialize_naive_datetime_to_i64<S>(
    block_time: &NaiveDateTime,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let bt_micros = block_time.and_utc().timestamp();
    serializer.serialize_i64(bt_micros)
}
