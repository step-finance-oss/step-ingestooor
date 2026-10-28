use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use borsh::BorshDeserialize;

use crate::{TransactionParserFilter, TransactionParserTrait};

use step_ingestooor_idl_gens::reward_options::reward_options::{
    accounts::CallOptionDistributor, utils::InstructionUnion,
};
use step_ingestooor_sdk::{
    dooot::{Dooot, RewardOptionUserDooot},
    trait_defs::transaction::TransactionLike,
    utils::{step_utils::StepUtils, util_fns::get_partially_decoded_variant},
};

pub struct RewardOptions;

pub const REWARD_OPTIONS_PROGRAM_ID: &str = "RopsJ74nrtcsk9iND8CFFNjn5mxf1tFzF3RhMPJ3CdV";

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct Proof {
    #[serde(flatten)]
    inner: HashMap<String, UserProof>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct UserProof {
    amount: String,
}

#[async_trait]
impl TransactionParserTrait for RewardOptions {
    fn get_name(&self) -> &'static str {
        "RewardOptions"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            REWARD_OPTIONS_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let block_time = txn.get_block_time();
        for ix in txn.get_flattened_ixs() {
            if ix.get_ix_program_id(txn) != REWARD_OPTIONS_PROGRAM_ID {
                continue;
            }
            let Some(part_dec) = get_partially_decoded_variant(ix.instruction) else {
                continue;
            };

            let ix_bytes = bs58::decode(&part_dec.data).into_vec()?;
            let decoded = InstructionUnion::try_from_slice(&ix_bytes)?;

            // Process new distributor instruction
            match decoded {
                InstructionUnion::NewDistributor(_ix_params) => {
                    let distributor_pubkey = &part_dec.accounts[4];
                    let dist_data = txn
                        .get_account_data_after(distributor_pubkey)?
                        .ok_or(anyhow!("Failed to get distributor data in NewDistributor"))?;
                    let dist = CallOptionDistributor::deserialize(&mut &dist_data.as_slice()[8..])
                        .context("Failed to deserialize distributor in NewDistributor")?;
                    let exp = chrono::DateTime::from_timestamp(dist.expiry as i64, 0)
                        .ok_or(anyhow!(
                            "Failed to convert expiry to datetime in NewDistributor"
                        ))?
                        .naive_utc();

                    //this is a creation and we need to lookup the proof from data_location
                    //and create user doots for all users
                    let loc = dist.data_location.clone();
                    //make web request to the data location
                    let resp = reqwest::get(&loc)
                        .await
                        .context("fetching reward option proof")?;
                    let proof_map: Proof =
                        resp.json().await.context("parsing reward option proof")?;

                    //create user doots for all users
                    for (pubkey, proof) in proof_map.inner.into_iter() {
                        let user_dooot = RewardOptionUserDooot {
                            time: block_time,
                            distributor: distributor_pubkey.clone(),
                            user: pubkey.clone(),
                            idx: dist.index,
                            writer: dist.writer.to_string(),
                            owner: dist.owner.to_string(),
                            strike_price: dist.strike_price,
                            expires_on: exp,
                            data_location: dist.data_location.clone(),
                            reward_mint: dist.reward_mint.to_string(),
                            price_mint: dist.price_mint.to_string(),
                            written_amount: proof
                                .amount
                                .parse()
                                .context("parsing written amount")?,
                            claimed_amount: 0,
                            paid_amount: 0,
                        };
                        dooots.push(Dooot::RewardOptionUser(user_dooot));
                    }
                }
                InstructionUnion::Exercise(ix_params) => {
                    // Distributor account should be at index 0
                    let distributor_pubkey = &part_dec.accounts[0];
                    let dist_data = txn
                        .get_account_data_after(distributor_pubkey)?
                        .ok_or(anyhow!("Failed to get distributor data in Exercise"))?;
                    let dist = CallOptionDistributor::deserialize(&mut &dist_data.as_slice()[8..])
                        .context("Failed to deserialize distributor in Exercise")?;

                    let exp = chrono::DateTime::from_timestamp(dist.expiry as i64, 0)
                        .ok_or(anyhow!("Failed to convert expiry to datetime in Exercise"))?
                        .naive_utc();
                    // Claimant account should be at index 7
                    let claimant_pubkey = &part_dec.accounts[7];

                    // Create user dooot to track remaining amount
                    let user_dooot = RewardOptionUserDooot {
                        time: block_time,
                        distributor: distributor_pubkey.clone(),
                        user: claimant_pubkey.clone(),
                        idx: dist.index,
                        writer: dist.writer.to_string(),
                        owner: dist.owner.to_string(),
                        strike_price: dist.strike_price,
                        expires_on: exp,
                        data_location: dist.data_location,
                        reward_mint: dist.reward_mint.to_string(),
                        price_mint: dist.price_mint.to_string(),
                        written_amount: ix_params.authorized_amount,
                        claimed_amount: ix_params.exercise_amount,
                        paid_amount: ix_params.exercise_amount * dist.strike_price,
                    };

                    dooots.push(Dooot::RewardOptionUser(user_dooot));
                }
                _ => {}
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use anyhow::Context;
    use step_ingestooor_sdk::{
        decoded_transaction::DecodedTransaction,
        dooot::Dooot,
        utils::{test_utils::init_test_extras_bare, util_fns::convert_blocktime_to_micros},
    };

    use crate::{RewardOptions, TransactionParserTrait as _};

    #[tokio::test]
    async fn test_reward_options_claim() -> anyhow::Result<()> {
        let txn_file = fs::read_to_string("tests/step/reward_option_claim.json").unwrap();
        let json = serde_json::from_str(&txn_file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();

        let (step_utils, _) = init_test_extras_bare().await;

        let reward_options = RewardOptions;
        let mut dooots = vec![];
        reward_options
            .invoke(&txn, &mut dooots, &step_utils)
            .await
            .context("Failed to invoke reward options")?;

        assert_eq!(dooots.len(), 1);
        let dooot = &dooots[0];
        assert!(matches!(dooot, Dooot::RewardOptionUser(_)));
        let Dooot::RewardOptionUser(dooot) = dooot else {
            panic!("Failed to match Dooot::RewardOptionUser")
        };
        assert_eq!(
            dooot.time,
            convert_blocktime_to_micros(1742511173, Some(2), Some(694)).unwrap()
        );
        assert_eq!(
            dooot.distributor,
            "4D1yby3quuE6suDvbwXb2cEawq6Std7UGc7ovNhs5k2r"
        );
        assert_eq!(dooot.user, "DanayVvgHg3dFjhJgjNECxU589vGjzmwRBBZ7nCMVzUg");
        assert_eq!(dooot.idx, 5253);
        assert_eq!(dooot.writer, "8A8WPc5mNE6xqypTrdRU6oLHa3wzM7o44BrFHjTGSGS4");
        assert_eq!(dooot.owner, "5Cebzty8iwgAUx9jyfZVAT2iMvXBECLwEVgT6T8KYmvS");
        assert_eq!(dooot.strike_price, 77283);
        assert_eq!(
            dooot.expires_on,
            convert_blocktime_to_micros(1743638400, Some(0), Some(0)).unwrap()
        );
        assert_eq!(
            dooot.data_location,
            "https://step-public.s3.eu-north-1.amazonaws.com/proofs/reward-options/validator-staking-2025-03-19.json"
        );
        assert_eq!(
            dooot.reward_mint,
            "StepAscQoEioFxxWGnh2sLBDFp9d8rvKz2Yp39iDpyT"
        );
        assert_eq!(
            dooot.price_mint,
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
        );
        assert_eq!(dooot.written_amount, 133055577175);
        assert_eq!(dooot.claimed_amount, 133055577175);
        assert_eq!(dooot.paid_amount, 10282934170815525);
        Ok(())
    }
}
