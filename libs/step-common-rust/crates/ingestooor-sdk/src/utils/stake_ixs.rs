use anyhow::Result;
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use solana_transaction_status::{UiInstruction, UiParsedInstruction};

use super::constants::STAKE_PROGRAM_ID;

#[enum_dispatch]
pub trait HasStakeAccount {
    fn get_stake_account(&self) -> Option<&String>;
}

#[enum_dispatch(HasStakeAccount)]
#[derive(Deserialize, Serialize)]
#[serde(tag = "type", content = "info")]
pub enum StakeProgramParsedInstruction {
    #[serde(alias = "initialize")]
    Initialize(InitializeIx),
    #[serde(alias = "authorize")]
    Authorize(AuthorizeIx),
    #[serde(alias = "delegate")]
    Delegate(DelegateIx),
    #[serde(alias = "split")]
    Split(SplitIx),
    #[serde(alias = "withdraw")]
    Withdraw(WithdrawIx),
    #[serde(alias = "deactivate")]
    Deactivate(DeactivateIx),
    #[serde(alias = "setLockup")]
    SetLockup(SetLockupIx),
    #[serde(alias = "merge")]
    Merge(MergeIx),
    #[serde(alias = "authorizeWithSeed")]
    AuthorizeWithSeed(AuthorizeWithSeedIx),
    #[serde(alias = "initializeChecked")]
    InitializeChecked(InitializeCheckedIx),
    #[serde(alias = "authorizeChecked")]
    AuthorizeChecked(AuthorizeCheckedIx),
    #[serde(alias = "authorizeCheckedWithSeed")]
    AuthorizeCheckedWithSeed(AuthorizeCheckedWithSeedIx),
    #[serde(alias = "setLockupChecked")]
    SetLockupChecked(SetLockupCheckedIx),
    #[serde(alias = "getMinimumDelegation")]
    GetMinimumDelegation(GetMinimumDelegationIx),
    #[serde(alias = "deactivateDelinquent")]
    DeactivateDelinquent(DeactivateDelinquentIx),
    #[serde(alias = "redelegate")]
    Redelegate(RedelegateIx),
    #[serde(alias = "moveLamports")]
    MoveLamports(MoveLamportsIx),
    #[serde(alias = "moveStake")]
    MoveStake(MoveStakeIx),
}

pub fn get_parsed_stake_program_instruction(
    ix: &UiInstruction,
) -> Result<Option<StakeProgramParsedInstruction>> {
    let UiInstruction::Parsed(UiParsedInstruction::Parsed(parsed)) = &ix else {
        return Ok(None);
    };

    if parsed.program_id != STAKE_PROGRAM_ID {
        return Ok(None);
    }

    Ok(Some(
        serde_json::from_value::<StakeProgramParsedInstruction>(parsed.parsed.clone())?,
    ))
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeIx {
    pub stake_account: String,
    pub rent_sysvar: String,
    pub authorized: Authorized,
    pub lockup: Lockup,
}

impl HasStakeAccount for InitializeIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Authorized {
    pub staker: String,
    pub withdrawer: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lockup {
    pub unix_timestamp: Option<u64>,
    pub epoch: Option<u64>,
    pub custodian: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeIx {
    pub stake_account: String,
    pub clock_sysvar: String,
    pub authority: String,
    pub new_authority: String,
    pub authority_type: String,
    pub custodian: Option<String>,
}

impl HasStakeAccount for AuthorizeIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegateIx {
    pub stake_account: String,
    pub vote_account: String,
    pub clock_sysvar: String,
    pub stake_history_sysvar: String,
    pub stake_config_account: String,
    pub stake_authority: String,
}

impl HasStakeAccount for DelegateIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitIx {
    pub stake_account: String,
    pub new_split_account: String,
    pub stake_authority: String,
    pub lamports: u64,
}

impl HasStakeAccount for SplitIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawIx {
    pub stake_account: String,
    pub destination: String,
    pub clock_sysvar: String,
    pub stake_history_sysvar: String,
    pub withdraw_authority: String,
    pub lamports: u64,
    pub custodian: Option<String>,
}

impl HasStakeAccount for WithdrawIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeactivateIx {
    pub stake_account: String,
    pub clock_sysvar: String,
    pub stake_authority: String,
}

impl HasStakeAccount for DeactivateIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLockupIx {
    pub stake_account: String,
    pub custodian: String,
    pub lockup: Lockup,
}

impl HasStakeAccount for SetLockupIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeIx {
    pub destination: String,
    pub source: String,
    pub clock_sysvar: String,
    pub stake_history_sysvar: String,
    pub stake_authority: String,
}

impl HasStakeAccount for MergeIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.source)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeWithSeedIx {
    pub stake_account: String,
    pub authority_base: String,
    pub new_authorized: String,
    pub authority_type: String,
    pub authority_seed: String,
    pub authority_owner: String,
    pub clock_sysvar: Option<String>,
    pub custodian: Option<String>,
}

impl HasStakeAccount for AuthorizeWithSeedIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeCheckedIx {
    pub stake_account: String,
    pub rent_sysvar: String,
    pub staker: String,
    pub withdrawer: String,
}

impl HasStakeAccount for InitializeCheckedIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeCheckedIx {
    pub stake_account: String,
    pub clock_sysvar: String,
    pub authority: String,
    pub new_authority: String,
    pub authority_type: String,
    pub custodian: Option<String>,
}

impl HasStakeAccount for AuthorizeCheckedIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeCheckedWithSeedIx {
    pub stake_account: String,
    pub authority_base: String,
    pub clock_sysvar: String,
    pub new_authorized: String,
    pub authority_type: String,
    pub authority_seed: String,
    pub authority_owner: String,
    pub custodian: Option<String>,
}

impl HasStakeAccount for AuthorizeCheckedWithSeedIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLockupCheckedIx {
    pub stake_account: String,
    pub custodian: String,
    pub lockup: Lockup,
}

impl HasStakeAccount for SetLockupCheckedIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMinimumDelegationIx {}

impl HasStakeAccount for GetMinimumDelegationIx {
    fn get_stake_account(&self) -> Option<&String> {
        None
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeactivateDelinquentIx {
    pub stake_account: String,
    pub vote_account: String,
    pub reference_vote_account: String,
}

impl HasStakeAccount for DeactivateDelinquentIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedelegateIx {
    pub stake_account: String,
    pub new_stake_account: String,
    pub vote_account: String,
    pub stake_config_account: String,
    pub stake_authority: String,
}

impl HasStakeAccount for RedelegateIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.stake_account)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveLamportsIx {
    pub destination: String,
    pub lamports: u64,
    pub source: String,
    pub stake_authority: String,
}

impl HasStakeAccount for MoveLamportsIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.source)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveStakeIx {
    pub destination: String,
    pub lamports: u64,
    pub source: String,
    pub stake_authority: String,
}

impl HasStakeAccount for MoveStakeIx {
    fn get_stake_account(&self) -> Option<&String> {
        Some(&self.source)
    }
}
