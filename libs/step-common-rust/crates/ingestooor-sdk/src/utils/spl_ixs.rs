use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use solana_account_decoder::{
    parse_token::{UiAccountState, UiTokenAmount},
    parse_token_extension::UiConfidentialTransferMint,
};
use solana_transaction_status::{
    parse_token::{UiAuthorityType, UiExtensionType},
    UiInstruction, UiParsedInstruction,
};

use super::constants::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID};

pub trait HasAuthority {
    fn get_authority(&self) -> Result<&String>;
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", content = "info")]
pub enum TokenParsedInstruction {
    #[serde(alias = "initializeMint", alias = "initializeMint2")]
    InitializeMint(TokenInitializeMintIx),
    #[serde(alias = "transfer")]
    TokenTransfer(TokenTransferIx),
    #[serde(alias = "transferChecked")]
    TokenTransferChecked(TokenTransferCheckedIx),
    #[serde(alias = "mintTo")]
    TokenMintTo(TokenMintToIx),
    #[serde(alias = "mintToChecked")]
    TokenMintToChecked(TokenMintToCheckedIx),
    #[serde(alias = "burn")]
    TokenBurn(TokenBurnIx),
    #[serde(alias = "burnChecked")]
    TokenBurnChecked(TokenBurnCheckedIx),
    #[serde(
        alias = "initializeAccount",
        alias = "initializeAccount2",
        alias = "initializeAccount3"
    )]
    TokenInitializeAccount(TokenInitializeAccountIx),
    #[serde(alias = "initializeMultisig", alias = "initializeMultisig2")]
    TokenInitializeMultiSig(TokenInitializeMultiSigIx),
    #[serde(alias = "approve")]
    TokenApprove(TokenApproveIx),
    #[serde(alias = "approveChecked")]
    TokenApproveChecked(TokenApproveCheckedIx),
    #[serde(alias = "revoke")]
    TokenRevoke(TokenRevokeIx),
    #[serde(alias = "setAuthority")]
    TokenSetAuthority(TokenSetAuthorityIx),
    #[serde(alias = "closeAccount")]
    TokenCloseAccount(TokenCloseAccountIx),
    #[serde(alias = "freezeAccount")]
    TokenFreezeAccount(TokenFreezeAccountIx),
    #[serde(alias = "thawAccount")]
    TokenThawAccount(TokenThawAccountIx),
    #[serde(alias = "syncNative")]
    TokenSyncNative(TokenSyncNativeIx),
    #[serde(alias = "getAccountDataSize")]
    TokenGetAccountDataSize(TokenGetAccountDataSizeIx),
    #[serde(alias = "initializeImmutableOwner")]
    TokenInitializeImmutableOwner(TokenInitializeImmutableOwnerIx),
    #[serde(alias = "amountToUiAmount")]
    TokenAmountToUiAmount(TokenAmountToUiAmountIx),
    #[serde(alias = "uiAmountToAmount")]
    TokenUiAmountToAmount(TokenUiAmountToAmountIx),
    #[serde(alias = "initializeMintCloseAuthority")]
    TokenInitializeMintCloseAuthority(TokenInitializeMintCloseAuthorityIx),
    #[serde(alias = "initializeTransferFeeConfig")]
    TokenInitializeTransferFeeConfig(TokenInitializeTransferFeeConfigIx),
    #[serde(alias = "transferCheckedWithFee")]
    TokenTransferCheckedWithFee(TokenTransferCheckedWithFeeIx),
    #[serde(alias = "withdrawWithheldTokensFromMint")]
    TokenWithdrawHeldTokensFromMint(TokenWithdrawHeldTokensFromMintIx),
    #[serde(alias = "withdrawWithheldTokensFromAccounts")]
    TokenWithdrawWithheldTokensFromAccounts(TokenWithdrawWithheldTokensFromAccountsIx),
    #[serde(alias = "harvestWithheldTokensToMint")]
    TokenHarvestWithheldTokensToMint(TokenHarvestWithheldTokensToMintIx),
    #[serde(alias = "setTransferFee")]
    TokenSetTransferFee(TokenSetTransferFeeIx),
    #[serde(alias = "initializeConfidentialTransferMint")]
    TokenInitializeConfidentialTransferMint(TokenInitializeConfidentialTransferMintIx),
    #[serde(alias = "updateConfidentialTransferMint")]
    TokenUpdateConfidentialTransferMint(TokenUpdateConfidentialTransferMintIx),
    #[serde(alias = "configureConfidentialTransferAccount")]
    TokenConfigureConfidentialTransferAccount(TokenConfigureConfidentialTransferAccountIx),
    #[serde(alias = "approveConfidentialTransferAccount")]
    TokenApproveConfidentialTransferAccount(TokenApproveConfidentialTransferAccountIx),
    #[serde(alias = "emptyConfidentialTransferAccount")]
    TokenEmptyConfidentialTransferAccount(TokenEmptyConfidentialTransferAccountIx),
    #[serde(alias = "depositConfidentialTransfer")]
    TokenDepositConfidentialTransfer(TokenDepositConfidentialTransferIx),
    #[serde(alias = "withdrawConfidentialTransfer")]
    TokenWithdrawConfidentialTransfer(TokenWithdrawConfidentialTransferIx),
    #[serde(alias = "confidentialTransfer")]
    TokenConfidentialTransfer(TokenConfidentialTransferIx),
    #[serde(alias = "applyPendingConfidentialTransferBalance")]
    TokenApplyPendingConfidentialTransferBalance(TokenApplyPendingConfidentialTransferBalanceIx),
    #[serde(
        alias = "enableConfidentialTransferConfidentialCredits",
        alias = "disableConfidentialTransferConfidentialCredits",
        alias = "enableConfidentialTransferNonConfidentialCredits",
        alias = "disableConfidentialTransferNonConfidentialCredits"
    )]
    TokenConfigureConfidentialTransferCredits(TokenConfigConfidentialTransferCreditsIx),
    #[serde(alias = "withdrawWithheldConfidentialTransferTokensFromMint")]
    TokenWithdrawWithheldConfidentialTransferTokensFromMint(
        TokenWithdrawWithheldConfidentialTransferTokensFromMintIx,
    ),
    #[serde(alias = "withdrawWithheldConfidentialTransferTokensFromAccounts")]
    TokenWithdrawWithheldConfidentialTransferTokensFromAccounts(
        TokenWithdrawWithheldConfidentialTransferTokensFromAccountsIx,
    ),
    #[serde(alias = "harvestWithheldConfidentialTransferTokensToMint")]
    TokenHarvestWithheldConfidentialTransferTokensToMint(
        TokenHarvestWithheldConfidentialTransferTokensToMintIx,
    ),
    #[serde(alias = "initializeDefaultAccountState")]
    TokenInitializeDefaultAccountState(TokenInitializeDefaultAccountStateIx),
    #[serde(alias = "updateDefaultAccountState")]
    TokenUpdateDefaultAccountState(TokenUpdateDefaultAccountStateIx),
    #[serde(alias = "reallocate")]
    TokenReallocate(TokenReallocateIx),
    #[serde(
        alias = "enableRequiredMemoTransfers",
        alias = "disableRequiredMemoTransfers"
    )]
    TokenConfigureRequiredMemoTransfers(TokenConfigureRequiredMemoTransfers),
    #[serde(alias = "createNativeMint")]
    TokenCreateNativeMint(TokenCreateNativeMintIx),
    #[serde(alias = "initializeNonTransferableMint")]
    TokenInitializeNonTransferableMint(TokenInitializeNonTransferableMintIx),
    #[serde(alias = "initializeInterestBearingConfig")]
    TokenInitializeInterestBearingConfig(TokenInitializeInterestBearingConfigIx),
    #[serde(alias = "updateInterestBearingConfigRate")]
    TokenUpdateInterestBearingConfigRate(TokenUpdateInterestBearingConfigRateIx),
    #[serde(alias = "enableCpiGuard", alias = "disableCpiGuard")]
    TokenConfigureCpiGuard(TokenConfigureCpiGuardIx),
    #[serde(alias = "initializePermanentDelegate")]
    TokenInitializePermanentDelegate(TokenInitializePermanentDelegateIx),
    #[serde(alias = "initializeTransferHook")]
    TokenInitializeTransferHook(TokenInitializeTransferHookIx),
    #[serde(alias = "updateTransferHook")]
    TokenUpdateTransferHook(TokenUpdateTransferHookIx),
    #[serde(
        alias = "enableConfidentialTransferFeeHarvestToMint",
        alias = "disableConfidentialTransferFeeHarvestToMint"
    )]
    TokenConfigureHarvestToMint(TokenConfigureConfidentialTransferFeeHarvestToMintIx),
    #[serde(alias = "withdrawExcessLamports")]
    TokenWithdrawExcessLamports(TokenWithdrawExcessLamportsIx),
    #[serde(alias = "initializeMetadataPointer")]
    TokenInitializeMetadataPointer(TokenInitializeMetadataPointerIx),
    #[serde(alias = "updateMetadataPointer")]
    TokenUpdateMetadataPointer(TokenUpdateMetadataPointerIx),
    #[serde(alias = "initializeGroupPointer")]
    TokenInitializeGroupPointer(TokenInitializeGroupPointerIx),
    #[serde(alias = "updateGroupPointer")]
    TokenUpdateGroupPointer(TokenUpdateGroupPointerIx),
    #[serde(alias = "initializeGroupMemberPointer")]
    TokenInitializeGroupMemberPointer(TokenInitializeGroupMemberPointerIx),
    #[serde(alias = "updateGroupMemberPointer")]
    TokenUpdateGroupMemberPointer(TokenUpdateGroupMemberPointerIx),
    // taken from https://github.com/step-finance/agave/blob/step-plugin-2.0.13-triton/transaction-status/src/parse_token/extension/token_metadata.rs#L26
    #[serde(alias = "initializeTokenMetadata")]
    TokenInitializeTokenMetadata(TokenInitializeTokenMetadataIx),
    #[serde(alias = "updateTokenMetadataField")]
    TokenUpdateField(TokenUpdateFieldIx),
    #[serde(alias = "removeTokenMetadataKey")]
    TokenRemoveKey(TokenRemoveKeyIx),
    #[serde(alias = "updateTokenMetadataAuthority")]
    TokenUpdateAuthority(TokenUpdateAuthorityIx),
    #[serde(alias = "emitTokenMetadata")]
    TokenEmit(TokenEmitIx),
    //https://github.com/step-finance/agave/blob/step-plugin-2.0.18-triton/transaction-status/src/parse_token/extension/token_group.rs#L58
    #[serde(alias = "initializeTokenGroupMember")]
    TokenInitializeTokenGroupMember(TokenInitializeTokenGroupMemberIx),
    #[serde(alias = "initializeScaledUiAmountConfig")]
    TokenInitializeScaledUiAmountConfig(TokenInitializeScaledUiAmountConfigIx),
    #[serde(alias = "initializeTokenGroup")]
    TokenInitializeTokenGroup(TokenInitializeTokenGroupIx),
    #[serde(alias = "initializePausableConfig")]
    TokenInitializePausableConfig(TokenInitializePausableConfigIx),
    #[serde(alias = "resume")]
    TokenResume(TokenResumeIx),
    #[serde(alias = "pause")]
    TokenPause(TokenPauseIx),
    #[serde(alias = "updateMultiplier")]
    TokenUpdateMultiplier(TokenUpdateMultiplierIx),
    #[serde(alias = "initializeConfidentialTransferFeeConfig")]
    TokenInitializeConfidentialTransferFeeConfig(TokenInitializeConfidentialTransferFeeConfigIx),
}

pub fn get_parsed_token_program_instruction(
    ix: &UiInstruction,
) -> Result<Option<TokenParsedInstruction>> {
    let UiInstruction::Parsed(UiParsedInstruction::Parsed(parsed)) = &ix else {
        return Ok(None);
    };

    if parsed.program_id != TOKEN_PROGRAM_ID && parsed.program_id != TOKEN_2022_PROGRAM_ID {
        return Ok(None);
    }

    Ok(Some(serde_json::from_value::<TokenParsedInstruction>(
        parsed.parsed.clone(),
    )?))
}
/// Encompasses both `initializeMint` and `initializeMint2`
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeMintIx {
    pub decimals: u8,
    pub mint: String,
    pub mint_authority: String,
    pub freeze_authority: Option<String>,
}

impl HasAuthority for TokenInitializeMintIx {
    fn get_authority(&self) -> Result<&String> {
        Ok(&self.mint_authority)
    }
}

#[serde_as]
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenTransferIx {
    pub source: String,
    pub destination: String,
    pub authority: Option<String>,
    #[serde_as(as = "DisplayFromStr")]
    pub amount: u64,
    pub multisig_authority: Option<String>,
}

impl HasAuthority for TokenTransferIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenTransferIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenTransferCheckedIx {
    pub source: String,
    pub destination: String,
    pub authority: Option<String>,
    pub token_amount: UiTokenAmount,
    pub multisig_authority: Option<String>,
}

impl HasAuthority for TokenTransferCheckedIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenTransferCheckedIx instruction!"))
    }
}

#[serde_as]
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenMintToIx {
    pub account: String,
    #[serde_as(as = "DisplayFromStr")]
    pub amount: u64,
    pub mint: String,
    pub mint_authority: Option<String>,
    pub multisig_authority: Option<String>,
}

impl HasAuthority for TokenMintToIx {
    fn get_authority(&self) -> Result<&String> {
        self.mint_authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenMintToIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenMintToCheckedIx {
    pub account: String,
    pub mint: String,
    pub mint_authority: Option<String>,
    pub token_amount: UiTokenAmount,
    pub multisig_authority: Option<String>,
}

impl HasAuthority for TokenMintToCheckedIx {
    fn get_authority(&self) -> Result<&String> {
        self.mint_authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenMintToCheckedIx instruction!"))
    }
}

#[serde_as]
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBurnIx {
    pub account: String,
    #[serde_as(as = "DisplayFromStr")]
    pub amount: u64,
    pub authority: Option<String>,
    pub mint: String,
    pub multisig_authority: Option<String>,
}

impl HasAuthority for TokenBurnIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenBurnIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBurnCheckedIx {
    pub mint: String,
    pub account: String,
    pub authority: Option<String>,
    pub token_amount: UiTokenAmount,
    pub multisig_authority: Option<String>,
}

impl HasAuthority for TokenBurnCheckedIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenBurnCheckedIx instruction!"))
    }
}

/// Encompasses all 3 account initialization types:
/// - initializeAccount
/// - initializeAccount2
/// - initializeAccount3
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeAccountIx {
    pub account: String,
    pub mint: String,
    pub owner: String,
}

impl HasAuthority for TokenInitializeAccountIx {
    fn get_authority(&self) -> Result<&String> {
        Ok(&self.owner)
    }
}

/// `initializeMultisig` and `initializeMultisig2`
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeMultiSigIx {
    pub multisig: String,
    pub signers: Vec<String>,
    pub m: u8,
}

/// `approve`
#[serde_as]
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenApproveIx {
    pub source: String,
    pub delegate: String,
    #[serde_as(as = "DisplayFromStr")]
    pub amount: u64,
    pub owner: Option<String>,
    pub multisig_owner: Option<String>,
}

impl HasAuthority for TokenApproveIx {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenApproveIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenApproveCheckedIx {
    pub source: String,
    pub mint: String,
    pub delegate: String,
    pub token_amount: UiTokenAmount,
    pub owner: Option<String>,
    pub multisig_owner: Option<String>,
}

impl HasAuthority for TokenApproveCheckedIx {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenApproveCheckedIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenRevokeIx {
    pub source: String,
    pub owner: Option<String>,
    pub multisig_owner: Option<String>,
}

impl HasAuthority for TokenRevokeIx {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenRevokeIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenSetAuthorityIx {
    pub mint: Option<String>,
    pub account: Option<String>,
    pub authority_type: UiAuthorityType,
    pub new_authority: Option<String>,
    pub authority: Option<String>,
    pub multisig_authority: Option<String>,
}

impl HasAuthority for TokenSetAuthorityIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenSetAuthorityIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenCloseAccountIx {
    pub account: String,
    pub destination: String,
    pub owner: Option<String>,
    pub multisig_owner: Option<String>,
}

impl HasAuthority for TokenCloseAccountIx {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenCloseAccountIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenFreezeAccountIx {
    pub account: String,
    pub mint: String,
    pub freeze_authority: Option<String>,
    pub multisig_freeze_authority: Option<String>,
}

impl HasAuthority for TokenFreezeAccountIx {
    fn get_authority(&self) -> Result<&String> {
        self.freeze_authority
            .as_ref()
            .or(self.multisig_freeze_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenFreezeAccountIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenThawAccountIx {
    pub account: String,
    pub mint: String,
    pub freeze_authority: Option<String>,
    pub multisig_freeze_authority: Option<String>,
}

impl HasAuthority for TokenThawAccountIx {
    fn get_authority(&self) -> Result<&String> {
        self.freeze_authority
            .as_ref()
            .or(self.multisig_freeze_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenThawAccountIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenSyncNativeIx {
    pub account: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenGetAccountDataSizeIx {
    pub mint: String,
    pub extension_types: Option<Vec<UiExtensionType>>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeImmutableOwnerIx {
    pub account: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenAmountToUiAmountIx {
    pub mint: String,
    pub amount: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUiAmountToAmountIx {
    pub mint: String,
    pub ui_amount: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeMintCloseAuthorityIx {
    pub mint: String,
    pub new_authority: Option<String>,
}

impl HasAuthority for TokenInitializeMintCloseAuthorityIx {
    fn get_authority(&self) -> Result<&String> {
        // If we need to use this IX in the future
        // suggest we may need to change get_authority to return a Result<Option<String>>
        self.new_authority.as_ref().ok_or_else(|| {
            anyhow!("Cannot find authority in TokenInitializeMintCloseAuthorityIx instruction!")
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeTransferFeeConfigIx {
    pub mint: String,
    pub transfer_fee_basis_points: u16,
    pub maximum_fee: u64,
    pub transfer_fee_config_authority: Option<String>,
    pub withdraw_withheld_authority: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenTransferCheckedWithFeeIx {
    pub source: String,
    pub mint: String,
    pub destination: String,
    pub token_amount: UiTokenAmount,
    pub fee_amount: UiTokenAmount,
    pub authority: Option<String>,
    pub multisig_authority: Option<String>,
}

impl HasAuthority for TokenTransferCheckedWithFeeIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenTransferCheckedWithFeeIx instruction!")
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenWithdrawHeldTokensFromMintIx {
    pub mint: String,
    pub fee_recipient: String,
    pub withdraw_withheld_authority: Option<String>,
    pub multisig_withdraw_withheld_authority: Option<String>,
}

impl HasAuthority for TokenWithdrawHeldTokensFromMintIx {
    fn get_authority(&self) -> Result<&String> {
        self.withdraw_withheld_authority
            .as_ref()
            .or(self.multisig_withdraw_withheld_authority.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenWithdrawHeldTokensFromMintIx instruction!")
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenWithdrawWithheldTokensFromAccountsIx {
    pub mint: String,
    pub fee_recipient: String,
    pub source_accounts: Vec<String>,
    pub withdraw_withheld_authority: Option<String>,
    pub multisig_withdraw_withheld_authority: Option<String>,
}

impl HasAuthority for TokenWithdrawWithheldTokensFromAccountsIx {
    fn get_authority(&self) -> Result<&String> {
        self.withdraw_withheld_authority
            .as_ref()
            .or(self.multisig_withdraw_withheld_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenWithdrawWithheldTokensFromAccountsIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenHarvestWithheldTokensToMintIx {
    pub mint: String,
    pub source_accounts: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenSetTransferFeeIx {
    pub mint: String,
    pub transfer_fee_basis_points: u16,
    pub maximum_fee: u64,
    pub transfer_fee_config_authority: Option<String>,
    /// camelCasing typo in Solana code
    pub multisigtransfer_fee_config_authority: Option<String>,
}

impl HasAuthority for TokenSetTransferFeeIx {
    fn get_authority(&self) -> Result<&String> {
        self.transfer_fee_config_authority
            .as_ref()
            .or(self.multisigtransfer_fee_config_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenSetTransferFeeIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeConfidentialTransferMintIx {
    pub mint: String,
    #[serde(flatten)]
    pub confidential_mint_info: UiConfidentialTransferMint,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUpdateConfidentialTransferMintIx {
    pub mint: String,
    pub confidential_transfer_mint_authority: String,
    pub new_confidential_transfer_mint_authority: String,
    #[serde(flatten)]
    pub confidential_mint_info: UiConfidentialTransferMint,
}

impl HasAuthority for TokenUpdateConfidentialTransferMintIx {
    /// Will provide old authority for revocation
    fn get_authority(&self) -> Result<&String> {
        Ok(&self.confidential_transfer_mint_authority)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenConfigureConfidentialTransferAccountIx {
    pub account: String,
    pub mint: String,
    pub decryptable_zero_balance: String,
    pub maximum_pending_balance_credit_counter: u64,
    pub owner: Option<String>,
    pub multisig_owner: Option<String>,
}

impl HasAuthority for TokenConfigureConfidentialTransferAccountIx {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenConfigureConfidentialTransferAccountIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenApproveConfidentialTransferAccountIx {
    pub account: String,
    pub mint: String,
    pub confidential_transfer_auditor_authority: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenEmptyConfidentialTransferAccountIx {
    pub account: String,
    pub proof_instruction_offset: i8,
    pub owner: Option<String>,
    pub multisig_owner: Option<String>,
}

impl HasAuthority for TokenEmptyConfidentialTransferAccountIx {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| {
                anyhow!(
                    "Cannot find authority in TokenEmptyConfidentialTransferAccountIx instruction!"
                )
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenDepositConfidentialTransferIx {
    pub source: String,
    pub destination: String,
    pub mint: String,
    pub amount: u64,
    pub decimals: u8,
    pub owner: Option<String>,
    pub multisig_owner: Option<String>,
}

impl HasAuthority for TokenDepositConfidentialTransferIx {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenDepositConfidentialTransferIx instruction!")
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenWithdrawConfidentialTransferIx {
    pub amount: u64,
    pub decimals: u8,
    pub destination: String,
    pub mint: String,
    pub owner: String,
    pub source: String,
    pub equality_proof_context_state_account: String,
    pub equality_proof_instruction_offset: u64,
    pub range_proof_instruction_offset: u64,
    pub new_decryptable_available_balance: String,
}

impl HasAuthority for TokenWithdrawConfidentialTransferIx {
    fn get_authority(&self) -> Result<&String> {
        Ok(&self.owner)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenConfidentialTransferIx {
    pub ciphertext_validity_proof_context_state_account: String,
    pub ciphertext_validity_proof_instruction_offset: u64,
    pub destination: String,
    pub equality_proof_context_state_account: String,
    pub equality_proof_instruction_offset: u64,
    pub mint: String,
    pub multisig_owner: Option<String>,
    pub new_source_decryptable_available_balance: String,
    pub owner: Option<String>,
    pub range_proof_instruction_offset: u64,
    pub signers: Option<Vec<String>>,
    pub source: String,
}

impl HasAuthority for TokenConfidentialTransferIx {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenConfidentialTransferIx instruction!")
            })
    }
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenApplyPendingConfidentialTransferBalanceIx {
    pub account: String,
    pub new_decryptable_available_balance: String,
    pub expected_pending_balance_credit_counter: u64,
    pub owner: Option<String>,
    pub multisig_owner: Option<String>,
}

impl HasAuthority for TokenApplyPendingConfidentialTransferBalanceIx {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenApplyPendingConfidentialTransferBalanceIx instruction!"))
    }
}

/// `enableConfidentialTransferConfidentialCredits`
/// `disableConfidentialTransferConfidentialCredits`
/// `enableConfidentialTransferNonConfidentialCredits`
/// `disableConfidentialTransferNonConfidentialCredits`
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenConfigConfidentialTransferCreditsIx {
    pub account: String,
    pub owner: Option<String>,
    pub multisig_owner: Option<String>,
}

impl HasAuthority for TokenConfigConfidentialTransferCreditsIx {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenConfigConfidentialTransferCreditsIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenWithdrawWithheldConfidentialTransferTokensFromMintIx {
    pub mint: String,
    pub fee_recipient: String,
    pub proof_instruction_offset: i8,
    pub withdraw_withheld_authority: Option<String>,
    pub multisig_withdraw_withheld_authority: Option<String>,
}

impl HasAuthority for TokenWithdrawWithheldConfidentialTransferTokensFromMintIx {
    fn get_authority(&self) -> Result<&String> {
        self.withdraw_withheld_authority
            .as_ref()
            .or(self.multisig_withdraw_withheld_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenWithdrawWithheldConfidentialTransferTokensFromMintIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenWithdrawWithheldConfidentialTransferTokensFromAccountsIx {
    pub mint: String,
    pub fee_recipient: String,
    pub source_accounts: Vec<String>,
    pub proof_instruction_offset: i8,
    pub withdraw_withheld_authority: Option<String>,
    pub multisig_withdraw_withheld_authority: Option<String>,
}

impl HasAuthority for TokenWithdrawWithheldConfidentialTransferTokensFromAccountsIx {
    fn get_authority(&self) -> Result<&String> {
        self.withdraw_withheld_authority
            .as_ref()
            .or(self.multisig_withdraw_withheld_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenWithdrawWithheldConfidentialTransferTokensFromAccountsIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenHarvestWithheldConfidentialTransferTokensToMintIx {
    pub mint: String,
    pub source_accounts: Vec<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeDefaultAccountStateIx {
    pub mint: String,
    pub account_state: UiAccountState,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUpdateDefaultAccountStateIx {
    pub mint: String,
    pub account_state: UiAccountState,
    pub freeze_authority: Option<String>,
    pub multisig_freeze_authority: Option<String>,
}

impl HasAuthority for TokenUpdateDefaultAccountStateIx {
    fn get_authority(&self) -> Result<&String> {
        self.freeze_authority
            .as_ref()
            .or(self.multisig_freeze_authority.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenUpdateDefaultAccountStateIx instruction!")
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenReallocateIx {
    pub account: String,
    pub payer: String,
    pub extension_types: Vec<UiExtensionType>,
    pub owner: Option<String>,
    pub multisig_owner: Option<String>,
}

impl HasAuthority for TokenReallocateIx {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenReallocateIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenConfigureRequiredMemoTransfers {
    pub account: String,
    pub owner: Option<String>,
    pub multisig_owner: Option<String>,
}

impl HasAuthority for TokenConfigureRequiredMemoTransfers {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenConfigureRequiredMemoTransfers instruction!")
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenCreateNativeMintIx {
    pub payer: String,
    pub native_mint: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeNonTransferableMintIx {
    pub mint: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeInterestBearingConfigIx {
    pub mint: String,
    pub rate_authority: Option<String>,
    pub rate: i16,
}

impl HasAuthority for TokenInitializeInterestBearingConfigIx {
    fn get_authority(&self) -> Result<&String> {
        self.rate_authority.as_ref().ok_or_else(|| {
            anyhow!("Cannot find authority in TokenInitializeInterestBearingConfigIx instruction!")
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUpdateInterestBearingConfigRateIx {
    pub mint: String,
    pub new_rate: i16,
    pub rate_authority: Option<String>,
    pub multisig_rate_authority: Option<String>,
}

impl HasAuthority for TokenUpdateInterestBearingConfigRateIx {
    fn get_authority(&self) -> Result<&String> {
        self.rate_authority
            .as_ref()
            .or(self.multisig_rate_authority.as_ref())
            .ok_or_else(|| {
                anyhow!(
                    "Cannot find authority in TokenUpdateInterestBearingConfigRateIx instruction!"
                )
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenConfigureCpiGuardIx {
    pub account: String,
    pub owner: Option<String>,
    pub multisig_owner: Option<String>,
}

impl HasAuthority for TokenConfigureCpiGuardIx {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenConfigureCpiGuardIx instruction!")
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializePermanentDelegateIx {
    pub mint: String,
    pub delegate: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeTransferHookIx {
    pub mint: String,
    pub authority: Option<String>,
    pub program_id: Option<String>,
}

impl HasAuthority for TokenInitializeTransferHookIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority.as_ref().ok_or_else(|| {
            anyhow!("Cannot find authority in TokenInitializeTransferHookIx instruction!")
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUpdateTransferHookIx {
    pub mint: String,
    pub program_id: Option<String>,
    pub authority: Option<String>,
    pub multisig_authority: Option<String>,
}

impl HasAuthority for TokenUpdateTransferHookIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenUpdateTransferHookIx instruction!")
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenConfigureConfidentialTransferFeeHarvestToMintIx {
    pub account: String,
    pub owner: Option<String>,
    pub multisig_owner: Option<String>,
}

impl HasAuthority for TokenConfigureConfidentialTransferFeeHarvestToMintIx {
    fn get_authority(&self) -> Result<&String> {
        self.owner
            .as_ref()
            .or(self.multisig_owner.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenEnableHarvestToMintIx instruction!")
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenWithdrawExcessLamportsIx {
    pub source: String,
    pub destination: String,
    pub authority: Option<String>,
    pub multisig_authority: Option<String>,
}

impl HasAuthority for TokenWithdrawExcessLamportsIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenWithdrawExcessLamportsIx instruction!")
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeMetadataPointerIx {
    pub mint: String,
    pub authority: Option<String>,
    pub metadata_address: Option<String>,
}

impl HasAuthority for TokenInitializeMetadataPointerIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority.as_ref().ok_or_else(|| {
            anyhow!("Cannot find authority in TokenInitializeMetadataPointerIx instruction!")
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUpdateMetadataPointerIx {
    pub mint: String,
    pub metadata_address: Option<String>,
    pub authority: Option<String>,
    pub multisig_authority: Option<String>,
}

impl HasAuthority for TokenUpdateMetadataPointerIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenUpdateMetadataPointerIx instruction!")
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeGroupPointerIx {
    pub mint: String,
    pub authority: Option<String>,
    pub group_address: Option<String>,
}

impl HasAuthority for TokenInitializeGroupPointerIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority.as_ref().ok_or_else(|| {
            anyhow!("Cannot find authority in TokenInitializeGroupPointerIx instruction!")
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUpdateGroupPointerIx {
    pub mint: String,
    pub group_address: Option<String>,
    pub authority: Option<String>,
    pub multisig_authority: Option<String>,
}

impl HasAuthority for TokenUpdateGroupPointerIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenUpdateGroupPointerIx instruction!")
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeGroupMemberPointerIx {
    pub mint: String,
    pub authority: Option<String>,
    pub member_address: Option<String>,
}

impl HasAuthority for TokenInitializeGroupMemberPointerIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority.as_ref().ok_or_else(|| {
            anyhow!("Cannot find authority in TokenInitializeGroupMemberPointerIx instruction!")
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUpdateGroupMemberPointerIx {
    pub mint: String,
    pub member_address: Option<String>,
    pub authority: Option<String>,
    pub multisig_authority: Option<String>,
}

impl HasAuthority for TokenUpdateGroupMemberPointerIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenUpdateGroupMemberPointerIx instruction!")
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUpdateFieldIx {
    pub field: String,
    pub metadata: String,
    pub update_authority: Option<String>,
    pub value: String,
}

impl HasAuthority for TokenUpdateFieldIx {
    fn get_authority(&self) -> Result<&String> {
        self.update_authority
            .as_ref()
            .or(self.update_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenUpdateFieldIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeTokenMetadataIx {
    pub metadata: String,
    pub mint: String,
    pub mint_authority: Option<String>,
    pub name: String,
    pub symbol: String,
    pub update_authority: Option<String>,
    pub uri: String,
}

impl HasAuthority for TokenInitializeTokenMetadataIx {
    fn get_authority(&self) -> Result<&String> {
        self.update_authority
            .as_ref()
            .or(self.update_authority.as_ref())
            .ok_or_else(|| {
                anyhow!("Cannot find authority in TokenInitializeTokenMetadataIx instruction!")
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenRemoveKeyIx {
    pub metadata: String,
    pub update_authority: Option<String>,
    pub key: String,
    pub idempotent: bool,
}

impl HasAuthority for TokenRemoveKeyIx {
    fn get_authority(&self) -> Result<&String> {
        self.update_authority
            .as_ref()
            .or(self.update_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenRemoveKeyIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUpdateAuthorityIx {
    pub metadata: String,
    pub update_authority: Option<String>,
    pub new_authority: Option<String>,
}

impl HasAuthority for TokenUpdateAuthorityIx {
    fn get_authority(&self) -> Result<&String> {
        self.new_authority
            .as_ref()
            .or(self.new_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenUpdateAuthorityIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenEmitIx {
    pub metadata: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeTokenGroupMemberIx {
    pub member: String,
    pub member_mint: Option<String>,
    pub member_mint_authority: Option<String>,
    pub group: String,
    pub group_update_authority: Option<String>,
}

impl HasAuthority for TokenInitializeTokenGroupMemberIx {
    fn get_authority(&self) -> Result<&String> {
        self.member_mint_authority.as_ref().ok_or_else(|| {
            anyhow!("Cannot find authority in TokenInitializeTokenGroupMemberIx instruction!")
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeScaledUiAmountConfigIx {
    pub authority: String,
    pub mint: String,
    pub multiplier: String,
}

impl HasAuthority for TokenInitializeScaledUiAmountConfigIx {
    fn get_authority(&self) -> Result<&String> {
        Ok(&self.authority)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeTokenGroupIx {
    pub group: String,
    pub max_size: u64,
    pub mint: String,
    pub mint_authority: String,
    pub update_authority: Option<String>,
}

impl HasAuthority for TokenInitializeTokenGroupIx {
    fn get_authority(&self) -> Result<&String> {
        Ok(&self.mint_authority)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializePausableConfigIx {
    pub authority: String,
    pub mint: String,
}

impl HasAuthority for TokenInitializePausableConfigIx {
    fn get_authority(&self) -> Result<&String> {
        Ok(&self.authority)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenResumeIx {
    pub authority: String,
    pub mint: String,
}

impl HasAuthority for TokenResumeIx {
    fn get_authority(&self) -> Result<&String> {
        Ok(&self.authority)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenPauseIx {
    pub authority: String,
    pub mint: String,
}

impl HasAuthority for TokenPauseIx {
    fn get_authority(&self) -> Result<&String> {
        Ok(&self.authority)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUpdateMultiplierIx {
    pub mint: String,
    pub authority: Option<String>,
    pub multisig_authority: Option<String>,
    pub new_multiplier: String,
    pub new_multiplier_timestamp: i64,
}

impl HasAuthority for TokenUpdateMultiplierIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority
            .as_ref()
            .or(self.multisig_authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenUpdateMultiplierIx instruction!"))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInitializeConfidentialTransferFeeConfigIx {
    pub mint: String,
    pub authority: Option<String>,
    pub withdraw_withheld_authority_elgamal_pubkey: Option<String>,
}

impl HasAuthority for TokenInitializeConfidentialTransferFeeConfigIx {
    fn get_authority(&self) -> Result<&String> {
        self.authority
            .as_ref()
            .or(self.authority.as_ref())
            .ok_or_else(|| anyhow!("Cannot find authority in TokenInitializeConfidentialTransferFeeConfigIx instruction!"))
    }
}
