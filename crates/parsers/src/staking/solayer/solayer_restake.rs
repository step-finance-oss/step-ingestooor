use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use borsh::BorshDeserialize;
use solana_sdk::pubkey::Pubkey;
use spl_token::solana_program::program_pack::Pack;
use std::collections::HashMap;
use std::str::FromStr;
use step_ingestooor_idl_gens::solayer_restake_gen::solayer_restake::accounts::RestakingPool;
use step_ingestooor_sdk::dooot::{Dooot, MintUnderlyingsGlobalDooot, SwapEventDooot};
use step_ingestooor_sdk::trait_defs::transaction::{TransactionLike, TransferDirection};
use step_ingestooor_sdk::utils::step_utils::StepUtils;
use step_ingestooor_sdk::utils::util_fns::{
    get_associated_token_address, get_partially_decoded_variant, get_program_id_of_ix,
};

pub struct SolayerRestake;

pub const SOLAYER_RESTAKE_PROGRAM_ID: &str = "sSo1iU21jBrU9VaJ8PJib1MtorefUV4fzC9GURa2KNn";
pub const SOLAYER_VAULT_ACCOUNT_DISCRIMINATOR: [u8; 8] =
    [0x0c, 0x05, 0x64, 0x8f, 0x7d, 0x5e, 0x1a, 0xd6];
pub const ASSOCIATED_TOKEN_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

#[async_trait]
impl TransactionParserTrait for SolayerRestake {
    fn get_name(&self) -> &'static str {
        "SolayerRestake"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        /*
        Instead of filtering by programId maybe we should filter
        dynamically by token vault accounts? In case token is sent
        directly to the vault instead of instructions? Similarly
        to xStep
         */
        vec![TransactionParserFilter::ProgramId(
            SOLAYER_RESTAKE_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> anyhow::Result<()> {
        let accts = txn
            .iter_accounts()
            .filter(|a| a.post_owner_program == Some(SOLAYER_RESTAKE_PROGRAM_ID));

        let mut restaking_pools = HashMap::new();

        for acct in accts {
            let post_acct_data = acct.unpack_post_datum().context("No post data")?;

            let post_acct_data = match post_acct_data {
                Some(data) => data,
                None => return Err(anyhow!("post_data is None")),
            };

            if post_acct_data[0..8] != SOLAYER_VAULT_ACCOUNT_DISCRIMINATOR {
                continue;
            }

            let restake_pool = RestakingPool::deserialize(&mut &post_acct_data[8..])
                .context("Failed to deserialize restaking pool")?;

            let lst_mint = &restake_pool.lst_mint;
            let rst_mint = &restake_pool.rst_mint;

            let token_program = txn
                .get_account_owner(&lst_mint.to_string())
                .context("No token program")?;

            let (vault_address, _) = get_associated_token_address(
                &Pubkey::from_str(acct.pubkey)?,
                lst_mint,
                &ASSOCIATED_TOKEN_PROGRAM_ID,
                &Pubkey::from_str(token_program)?,
            );

            let vault_balance = txn
                .get_post_token_account_balance(&vault_address.to_string())
                .ok()
                .flatten()
                .context("No vault balance found")?;

            let rst_mint_supply = txn
                .get_account_data_after(&rst_mint.to_string())
                .ok()
                .flatten()
                .map(|bytes| spl_token::state::Mint::unpack(&bytes))
                .transpose()?
                .map(|mint| mint.supply)
                .context("No LST mint supply found")?;

            let amt_per_parent = if rst_mint_supply > 0 {
                vault_balance as f64 / rst_mint_supply as f64
            } else {
                0.0
            };

            let mint_underlying = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                mint_pubkey: rst_mint.to_string(),
                mints: vec![lst_mint.to_string()],
                mints_qty_per_one_parent: vec![amt_per_parent],
                platform_program_pubkey: SOLAYER_RESTAKE_PROGRAM_ID.to_string(),
                time: txn.get_block_time(),
                total_underlying_amounts: vec![vault_balance.into()],
                discriminant_id: acct.pubkey.to_string(),
            });

            dooots.push(mint_underlying);

            let pool_account = StakePoolInfo {
                lst_mint: lst_mint.to_string(),
                rst_mint: rst_mint.to_string(),
            };

            restaking_pools.insert(acct.pubkey.to_string(), pool_account);
        }

        let txn_signers = txn
            .get_account_keys()
            .iter()
            .filter(|a| a.signer)
            .collect::<Vec<_>>();

        let tree = txn.get_ix_tree()?;
        let root = tree.root().context("No root node found in ix tree")?;
        let ix_iter = root.traverse_level_order().skip(1);

        for node in ix_iter {
            let ix = node.data().context("No instruction data")?;
            let Some(ix) = get_partially_decoded_variant(ix) else {
                continue;
            };

            // Check if instruction is from Solayer program
            if ix.program_id != SOLAYER_RESTAKE_PROGRAM_ID {
                continue;
            }

            // Check if instruction contains pool account, if not skip
            let Some((pool_pubkey, pool_data)) = ix
                .accounts
                .iter()
                .find(|&a| restaking_pools.contains_key(a.as_str()))
                .and_then(|pubkey| Some((pubkey.clone(), restaking_pools.get(pubkey.as_str())?)))
            else {
                continue;
            };

            // Get the signer that is also in the ixs
            let Some(user) = ix
                .accounts
                .iter()
                .find(|&a| txn_signers.iter().any(|s| s.pubkey == *a))
            else {
                // No signer that is also in ixs here?
                continue;
            };

            let parent_program_id = node
                .parent()
                .and_then(|n| *n.data())
                .and_then(get_program_id_of_ix);

            let ixs_scan = node
                .traverse_level_order()
                .skip(1)
                .filter_map(|kid| *kid.data());

            let txfer_amounts = txn.get_total_transfer_amounts_for_pubkey_ui_ixs(
                ixs_scan,
                &[user],
                &[&pool_data.lst_mint, &pool_data.rst_mint],
                &[&pool_data.lst_mint, &pool_data.rst_mint],
            )?;

            let lst_token_direction = txfer_amounts.get_direction(&pool_data.lst_mint, user);
            let rst_token_direction = txfer_amounts.get_direction(&pool_data.rst_mint, user);

            match (lst_token_direction, rst_token_direction) {
                (TransferDirection::Zero, TransferDirection::Zero) => {
                    // Don't do anything
                    continue;
                }
                (TransferDirection::Receive, TransferDirection::Send) => {
                    let amount_in = txfer_amounts.get_out_amount_i128(&pool_data.lst_mint, user);
                    let amount_out = txfer_amounts.get_in_amount_i128(&pool_data.rst_mint, user);

                    let swap_doot = Dooot::SwapEvent(SwapEventDooot {
                        in_amount: amount_in.into(),
                        out_amount: amount_out.into(),
                        in_mint_pubkey: pool_data.lst_mint.to_string(),
                        out_mint_pubkey: pool_data.rst_mint.to_string(),
                        parent_amm_program_pubkey: parent_program_id,
                        program_pubkey: SOLAYER_RESTAKE_PROGRAM_ID.to_string(),
                        fee_mint_is_in_mint: None,
                        platform_referral_fee: None,
                        referring_platform_account_pubkey: None,
                        swap_auth_pubkey: user.to_string(),
                        time: txn.get_block_time(),
                        market_pubkey: Some(pool_pubkey.to_string()),
                        fees: vec![],
                        slippage_bps: None,
                        tx_sig: txn.get_signature().to_string(),
                    });

                    dooots.push(swap_doot);
                }
                (TransferDirection::Send, TransferDirection::Receive) => {
                    let amount_in = txfer_amounts.get_out_amount_i128(&pool_data.rst_mint, user);
                    let amount_out = txfer_amounts.get_in_amount_i128(&pool_data.lst_mint, user);

                    let swap_doot = Dooot::SwapEvent(SwapEventDooot {
                        in_amount: amount_in.into(),
                        out_amount: amount_out.into(),
                        in_mint_pubkey: pool_data.rst_mint.to_string(),
                        out_mint_pubkey: pool_data.lst_mint.to_string(),
                        parent_amm_program_pubkey: parent_program_id,
                        program_pubkey: SOLAYER_RESTAKE_PROGRAM_ID.to_string(),
                        fee_mint_is_in_mint: None,
                        platform_referral_fee: None,
                        referring_platform_account_pubkey: None,
                        swap_auth_pubkey: user.to_string(),
                        time: txn.get_block_time(),
                        market_pubkey: Some(pool_pubkey.to_string()),
                        fees: vec![],
                        slippage_bps: None,
                        tx_sig: txn.get_signature().to_string(),
                    });

                    dooots.push(swap_doot);
                }
                _ => {
                    // Invalid transfer direction
                    continue;
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct StakePoolInfo {
    pub lst_mint: String,
    pub rst_mint: String,
}

#[cfg(test)]
mod tests {
    use crate::{SolayerRestake, TransactionParserTrait};
    use std::fs;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    use step_ingestooor_sdk::utils::test_utils::init_test_extras_bare;

    #[tokio::test]
    async fn test_solayer_restake() {
        let file = fs::read_to_string("./tests/solayer/solayer_restake.json")
            .expect("Unable to read file");
        let txn: DecodedTransaction = serde_json::from_str(&file).expect("Unable to parse json");
        let solayer_restake = SolayerRestake;
        let mut dooots = vec![];
        let (step_utils, _) = init_test_extras_bare().await;

        solayer_restake
            .invoke(&txn, &mut dooots, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooots.len(), 2); // mint underlying and swap dooots
    }

    #[tokio::test]
    async fn test_solayer_unrestake() {
        let file = fs::read_to_string("./tests/solayer/solayer_unrestake.json")
            .expect("Unable to read file");
        let txn: DecodedTransaction = serde_json::from_str(&file).expect("Unable to parse json");
        let solayer_restake = SolayerRestake;
        let mut dooots = vec![];
        let (step_utils, _) = init_test_extras_bare().await;

        solayer_restake
            .invoke(&txn, &mut dooots, &step_utils)
            .await
            .unwrap();

        assert_eq!(dooots.len(), 2); // mint underlying and swap dooots
    }
}
