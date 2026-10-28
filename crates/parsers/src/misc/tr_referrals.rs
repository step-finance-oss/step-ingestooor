use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::mpsc::SyncSender,
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use borsh::BorshDeserialize;

use crate::{
    state_source::{AccountProvider, StateSource},
    TransactionParserFilter, TransactionParserTrait,
};
use log::{debug, info, warn};
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};
use spl_token::solana_program::program_pack::Pack;
use step_ingestooor_idl_gens::tr_referrals_gen::tr_referrals::{
    accounts::{project, referralAccount},
    utils::InstructionUnion,
};
use step_ingestooor_sdk::{
    dooot::{AccountInfoDooot, Dooot},
    trait_defs::transaction::TransactionLike,
    utils::{
        constants::TOKEN_PROGRAM_ID,
        step_utils::StepUtils,
        util_fns::{convert_blocktime_to_micros, get_partially_decoded_variant},
    },
};

pub struct TeamRaccoonsReferrals;

pub const REFERRAL_PROGRAM_ID: &str = "REFER4ZgmyYx9c6He5XfaTMiGfdLwRnkV4RPp9t9iF3";

#[async_trait]
impl TransactionParserTrait for TeamRaccoonsReferrals {
    fn get_name(&self) -> &'static str {
        "TeamRacoonsReferrals"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            REFERRAL_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        for ix in txn.get_flattened_ixs() {
            let Some(part_dec) = get_partially_decoded_variant(ix.instruction) else {
                continue;
            };

            if part_dec.program_id != REFERRAL_PROGRAM_ID {
                continue;
            }

            let ix_bytes = bs58::decode(&part_dec.data).into_vec()?;
            let decoded = match InstructionUnion::try_from_slice(&ix_bytes) {
                Ok(decoded) => decoded,
                Err(err) => {
                    log::info!(
                        "Failed to deserialize instruction for program {}: {}\n{:?}",
                        part_dec.program_id,
                        err,
                        ix_bytes
                    );
                    continue;
                }
            };

            let new_dooot = match decoded {
                InstructionUnion::InitializeProject(a) => {
                    Some(Dooot::AccountMetadata(AccountInfoDooot {
                        time,
                        account_pubkey: part_dec.accounts[3].clone(),
                        tr_project_name: Some(a.params.name),
                        ..Default::default()
                    }))
                }
                InstructionUnion::InitializeReferralAccount(_) => {
                    //deser the project to get the name on the metadata because why not
                    let p_key = &part_dec.accounts[2];
                    let p_data = txn
                        .get_account_data_after(p_key)?
                        .ok_or_else(|| anyhow!("project data not found"))?;
                    let p = project::deserialize(&mut &p_data[8..])?;
                    //the metadata dooot
                    Some(Dooot::AccountMetadata(AccountInfoDooot {
                        time,
                        account_pubkey: part_dec.accounts[3].clone(),
                        tr_project_key: Some(p_key.clone()),
                        tr_project_name: Some(p.name),
                        tr_referral_account_name: Some(String::new()),
                        ..Default::default()
                    }))
                }
                InstructionUnion::InitializeReferralAccountWithName(ix_params) => {
                    //deser the project to get the name on the metadata because why not
                    let p_key = part_dec.accounts[2].clone();
                    let p_data = txn
                        .get_account_data_after(&p_key)?
                        .ok_or_else(|| anyhow!("project data not found"))?;
                    let p = project::deserialize(&mut &p_data[8..])?;
                    //the metadata dooot
                    Some(Dooot::AccountMetadata(AccountInfoDooot {
                        time,
                        account_pubkey: part_dec.accounts[3].clone(),
                        tr_project_key: Some(part_dec.accounts[2].clone()),
                        tr_project_name: Some(p.name),
                        tr_referral_account_name: Some(ix_params.params.name),
                        ..Default::default()
                    }))
                }
                InstructionUnion::InitializeReferralTokenAccount(_) => {
                    //deser the project to get the name on the metadata because why not
                    let p_key = &part_dec.accounts[1];
                    let p_data = txn
                        .get_account_data_after(p_key)?
                        .ok_or_else(|| anyhow!("project data not found"))?;
                    let p = project::deserialize(&mut &p_data[8..])?;
                    //deser the referral account to get the name on the metadata because why not
                    let r_key = &part_dec.accounts[2];
                    let r_data = txn
                        .get_account_data_after(r_key)?
                        .ok_or_else(|| anyhow!("referral account data not found"))?;
                    let r = referralAccount::deserialize(&mut &r_data[8..])?;
                    // Publish a dooot for the token account
                    Some(Dooot::AccountMetadata(AccountInfoDooot {
                        time,
                        account_pubkey: part_dec.accounts[3].clone(),
                        tr_project_key: Some(p_key.clone()),
                        tr_project_name: Some(p.name),
                        tr_referral_account_key: Some(r_key.clone()),
                        tr_referral_account_name: Some(r.name.unwrap_or_default()),
                    }))
                }
                _ => None,
            };
            if let Some(new_dooot) = new_dooot {
                dooots.push(new_dooot);
            }
        }

        Ok(())
    }
}

impl StateSource for TeamRaccoonsReferrals {
    fn get_name(&self) -> &'static str {
        "TeamRaccoonsReferrals"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        info!("begin get state for team raccoons referrals");
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let disc_project = &solana_sdk::hash::hash("account:Project".as_bytes()).to_bytes()[0..8];
        debug!("project disc is {:?}", disc_project);
        let disc_referral =
            &solana_sdk::hash::hash("account:ReferralAccount".as_bytes()).to_bytes()[0..8];
        debug!("referral disc is {:?}", disc_referral);
        let program_id = Pubkey::from_str(REFERRAL_PROGRAM_ID)?;

        //get all projects; doing a collect because I know this isn't too many (< 5k)
        let all_accounts: Vec<_> = acct_provider.program_accounts(program_id, None).collect();

        debug!("there are {} accounts", all_accounts.len());

        let projects = all_accounts
            .iter()
            .filter_map(|a| {
                let (disc, mut data) = a.account.data().split_at(8);
                if disc != disc_project {
                    return None;
                }
                let t = project::deserialize(&mut data);
                if let Err(e) = t {
                    warn!("error parsing project: {} data size {}", e, data.len());
                    return None;
                }
                let project = t.ok()?;
                Some((a.pubkey, project))
            })
            .collect::<HashMap<_, _>>();
        debug!("there are {} projects", projects.len());

        //simple dooot for each project
        let project_dooots = projects.iter().map(|(key, project)| {
            Dooot::AccountMetadata(AccountInfoDooot {
                time,
                account_pubkey: key.to_string(),
                tr_project_name: Some(project.name.clone()),
                ..Default::default()
            })
        });
        //send the dooots
        for p in project_dooots {
            tx.send(p)?;
        }

        //now grab all referrers
        let referrers = all_accounts
            .iter()
            .filter_map(|a| {
                let (disc, mut data) = a.account.data().split_at(8);
                if disc != disc_referral {
                    debug!("looking for referrals and disc {:?} didnt match", disc);
                    return None;
                }
                let t = referralAccount::deserialize(&mut data);
                if let Err(e) = t {
                    warn!("error parsing referrer: {} data size {}", e, data.len());
                    return None;
                }
                let referrer = t.ok()?;
                //join to the project
                let Some(project) = projects.get(&referrer.project) else {
                    warn!(
                        "project {} is not found for referrer {:?}",
                        &referrer.project, &referrer
                    );
                    return None;
                };
                Some((a.pubkey, referrer, project))
            })
            .collect::<Vec<_>>();
        debug!("there are {} referrers", referrers.len());

        let referrer_dooots = referrers.iter().map(|(key, referrer, project)| {
            Dooot::AccountMetadata(AccountInfoDooot {
                account_pubkey: key.to_string(),
                tr_project_key: Some(referrer.project.to_string()),
                tr_project_name: Some(project.name.clone()),
                tr_referral_account_name: Some(referrer.name.clone().unwrap_or("".to_string())),
                ..Default::default()
            })
        });
        //send the referrer dooots
        for r in referrer_dooots {
            tx.send(r)?;
        }

        let project_keys: HashSet<_> = projects.keys().map(|a| a.to_bytes()).collect();
        debug!(
            "getting token accounts for {} project keys",
            project_keys.len()
        );
        let mut project_token_accounts = acct_provider
            .program_accounts(Pubkey::from_str(TOKEN_PROGRAM_ID)?, None)
            .filter_map(|a| {
                let data = a.account.data();
                if data.len() == 165 && project_keys.contains(&data[32..64]) {
                    let token_account = spl_token::state::Account::unpack(data).ok()?;
                    Some((a.pubkey, token_account))
                } else {
                    None
                }
            })
            .collect::<HashMap<_, _>>();
        debug!("there are {} token accounts", project_token_accounts.len());

        let mints = project_token_accounts
            .values()
            .map(|a| a.mint)
            .collect::<HashSet<_>>();
        debug!("there are {} distinct mints", mints.len());

        //&["referral_ata".as_bytes(), key.as_ref(), mint.as_ref()],
        //join every mint to every referrer
        let mint_referrer_product = mints.iter().flat_map(|a| referrers.iter().map(|b| (*a, b)));
        let referrer_token_accounts = mint_referrer_product.filter_map(|a| {
            let (derived, _bump) = Pubkey::find_program_address(
                &["referral_ata".as_bytes(), a.1 .0.as_ref(), a.0.as_ref()],
                &program_id,
            );
            let (token_account_key, _) = project_token_accounts.remove_entry(&derived)?;
            Some((token_account_key, a.1))
        });

        //create token account dooots
        let token_account_dooots =
            referrer_token_accounts.map(|(token_account_key, (key, referrer, project))| {
                Dooot::AccountMetadata(AccountInfoDooot {
                    time,
                    account_pubkey: token_account_key.to_string(),
                    tr_project_key: Some(referrer.project.to_string()),
                    tr_project_name: Some(project.name.clone()),
                    tr_referral_account_key: Some(key.to_string()),
                    tr_referral_account_name: Some(referrer.name.clone().unwrap_or("".to_string())),
                })
            });

        debug!("sending dooots");
        //send the token account dooots
        for t in token_account_dooots {
            tx.send(t)?;
        }

        debug!(
            "there are {} underived token accounts",
            project_token_accounts.len()
        );
        if !project_token_accounts.is_empty() {
            debug!(
                "details:\n{:?}",
                project_token_accounts
                    .drain()
                    .map(|a| a.0)
                    .collect::<Vec<_>>()
            );
        }
        debug!("done with state source!");

        Ok(())
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
    #[tokio::test]
    async fn failing_dlq_txn() {
        let message_json = fs::read_to_string("./tests/team_racoon_reff/failing.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();

        let step_utils = Rc::new(StepUtils::new(None));

        let mut dooot = vec![];
        let parser = TeamRaccoonsReferrals;

        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
        println!("dooot: {:#?}", dooot);
        assert_eq!(dooot.len(), 0);
    }
}
