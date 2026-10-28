use anyhow::Result;
use serde::{Deserialize, Serialize};
use solana_transaction_status::{UiInstruction, UiParsedInstruction};

use super::constants::SYSTEM_PROGRAM_ID;

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", content = "info")]
pub enum SystemProgramParsedInstruction {
    #[serde(alias = "transfer")]
    SystemTransfer(SystemTransferIx),
    #[serde(alias = "createAccount")]
    SystemCreateAccount(SystemCreateAccountIx),
    #[serde(alias = "assign")]
    SystemAssign(SystemAssignIx),
    #[serde(alias = "createAccountWithSeed")]
    SystemCreateAccountWithSeed(SystemCreateAccountWithSeedIx),
    #[serde(alias = "advanceNonce")]
    SystemAdvanceNonceAccount(SystemAdvanceNonceAccountIx),
    #[serde(alias = "withdrawFromNonce")]
    SystemWithdrawNonceAccount(SystemWithdrawNonceAccountIx),
    #[serde(alias = "initializeNonce")]
    SystemInitializeNonceAccount(SystemInitializeNonceAccountIx),
    #[serde(alias = "authorizeNonce")]
    SystemAuthorizeNonceAccount(SystemAuthorizeNonceAccountIx),
    #[serde(alias = "upgradeNonce")]
    SystemUpgradeNonceAccount(SystemUpgradeNonceAccountIx),
    #[serde(alias = "allocate")]
    SystemAllocate(SystemAllocateIx),
    #[serde(alias = "allocateWithSeed")]
    SystemAllocateWithSeed(SystemAllocateWithSeedIx),
    #[serde(alias = "assignWithSeed")]
    SystemAssignWithSeed(SystemAssignWithSeedIx),
    #[serde(alias = "transferWithSeed")]
    SystemTransferWithSeed(SystemTransferWithSeedIx),
}

pub fn get_parsed_system_program_instruction(
    ix: &UiInstruction,
) -> Result<Option<SystemProgramParsedInstruction>> {
    let UiInstruction::Parsed(UiParsedInstruction::Parsed(parsed)) = &ix else {
        return Ok(None);
    };

    if parsed.program_id != SYSTEM_PROGRAM_ID {
        return Ok(None);
    }

    Ok(Some(serde_json::from_value::<
        SystemProgramParsedInstruction,
    >(parsed.parsed.clone())?))
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemTransferIx {
    pub destination: String,
    pub lamports: u64,
    pub source: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemCreateAccountIx {
    pub source: String,
    pub new_account: String,
    pub lamports: u64,
    pub space: u64,
    pub owner: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAssignIx {
    pub account: String,
    pub owner: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemCreateAccountWithSeedIx {
    pub source: String,
    pub new_account: String,
    pub base: String,
    pub seed: String,
    pub lamports: u64,
    pub space: u64,
    pub owner: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAdvanceNonceAccountIx {
    pub nonce_account: String,
    pub nonce_authority: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemWithdrawNonceAccountIx {
    pub nonce_account: String,
    pub destination: String,
    pub nonce_authority: String,
    pub lamports: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInitializeNonceAccountIx {
    pub nonce_account: String,
    pub nonce_authority: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAuthorizeNonceAccountIx {
    pub nonce_account: String,
    pub nonce_authority: String,
    pub new_authorized: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemUpgradeNonceAccountIx {
    pub nonce_account: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAllocateIx {
    pub account: String,
    pub space: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAllocateWithSeedIx {
    pub account: String,
    pub base: String,
    pub seed: String,
    pub space: u64,
    pub owner: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAssignWithSeedIx {
    pub account: String,
    pub base: String,
    pub seed: String,
    pub owner: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemTransferWithSeedIx {
    pub source: String,
    pub source_base: String,
    pub destination: String,
    pub lamports: u64,
    pub source_seed: String,
    pub source_owner: String,
}
