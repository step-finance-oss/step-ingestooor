use std::{
    collections::{HashMap, HashSet, VecDeque},
    str::FromStr,
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use borsh::BorshDeserialize;

use rust_decimal::Decimal;
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};

use solana_transaction_status::{UiInstruction, UiParsedInstruction};
use step_ingestooor_idl_gens::{
    meteora_amm_gen::{self, meteora_amm::events::Swap},
    meteora_vaults_gen::meteora_vaults::accounts::Vault,
};
use step_ingestooor_sdk::{
    dooot::{
        self, Dooot, LPInfoDooot, LPInfoUnderlyingMintVault, LiquidityChangeEventDooot,
        MintSupplyGlobalDooot, MintUnderlyingsGlobalDooot, SwapEventDooot, SwapFees,
    },
    trait_defs::transaction::{TransactionLike, TransferDirection},
    utils::{
        step_account_reader::StepAccountReader,
        step_utils::StepUtils,
        util_fns::{
            convert_blocktime_to_micros, get_partially_decoded_variant,
            get_spl_version_from_program_id,
        },
    },
};

use crate::{
    state_source::StateSource,
    vaults::meteora_vaults::{METEORA_VAULTS_PROGRAM_ID, VAULT_ACCOUNT_DISCRIMINATOR},
    MeteoraVaults, TransactionParserFilter, TransactionParserTrait,
};

pub const METEORA_AMM_PROGRAM_ID: &str = "Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB";
pub const POOL_ACCOUNT_DISCRIMINATOR: [u8; 8] = [0xf1, 0x9a, 0x6d, 0x04, 0x11, 0xb1, 0x6d, 0xbc];

pub struct MeteoraAMM;

#[async_trait]
impl TransactionParserTrait for MeteoraAMM {
    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            METEORA_AMM_PROGRAM_ID.to_string(),
        )]
    }

    fn get_name(&self) -> &'static str {
        "MeteoraAMM"
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        doots: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let accts = txn
            .iter_accounts()
            .filter(|a| a.post_owner_program == Some(METEORA_AMM_PROGRAM_ID));

        let mut pools = HashMap::new();
        let mut vault_ata_infos = HashMap::new();

        let mut swap_events: VecDeque<_> = txn
            .get_program_events_from_logs(METEORA_AMM_PROGRAM_ID)
            .iter()
            .filter_map(|e| Swap::try_from_slice(e).ok())
            .collect();

        for acct in accts {
            let post_pool_data = acct
                .unpack_post_datum()?
                .context("Missing post datum for Meteora AMM")?;

            let discrim = post_pool_data
                .get(..8)
                .context("Datum not big enough for discrim")?;
            if discrim != POOL_ACCOUNT_DISCRIMINATOR {
                continue;
            }

            let mut acct_reader: StepAccountReader<'_> = StepAccountReader::new(&post_pool_data);
            acct_reader.advance_unclamped(8);
            let lp_mint_pk = acct_reader.read_next_pubkey()?;
            let token_a_mint_pk = acct_reader.read_next_pubkey()?;
            let token_b_mint_pk = acct_reader.read_next_pubkey()?;
            let vault_a_pk = acct_reader.read_next_pubkey()?;
            let vault_b_pk = acct_reader.read_next_pubkey()?;
            let vault_a_lp_pk = acct_reader.read_next_pubkey()?;
            let vault_b_lp_pk = acct_reader.read_next_pubkey()?;

            let lp_mint = lp_mint_pk.to_string();
            let token_a_mint = token_a_mint_pk.to_string();
            let token_b_mint = token_b_mint_pk.to_string();
            let vault_a = vault_a_pk.to_string();
            let vault_b = vault_b_pk.to_string();
            let vault_a_lp = vault_a_lp_pk.to_string();
            let vault_b_lp = vault_b_lp_pk.to_string();
            let was_created = acct.was_created;

            let curve_type = Self::get_pool_curve_type(&post_pool_data)?;

            let pool_info = MeteoraAMMPoolInfo {
                lp_mint: lp_mint.clone(),
                token_a_mint: token_a_mint.clone(),
                token_b_mint: token_b_mint.clone(),
                vault_a: vault_a.clone(),
                vault_b: vault_b.clone(),
                vault_a_lp: vault_a_lp.clone(),
                vault_b_lp: vault_b_lp.clone(),
                was_created,
                curve_type,
            };
            pools.insert(acct.pubkey, pool_info);
        }

        let txn_signers = txn
            .get_account_keys()
            .iter()
            .filter(|a| a.signer)
            .collect::<Vec<_>>();

        for (pk, pool) in pools.iter() {
            let vault_ata_a = &pool.vault_a_lp;
            let vault_ata_b = &pool.vault_b_lp;

            let Ok((pre_a, post_a)) = txn.get_pre_and_post_token_balance(vault_ata_a) else {
                continue;
            };
            let Ok((pre_b, post_b)) = txn.get_pre_and_post_token_balance(vault_ata_b) else {
                continue;
            };

            // Grab GState info before we check for mintunderlying info
            if pool.was_created {
                let vault_ata_underlying =
                    MeteoraVaults::derive_vault_ata_pk(&Pubkey::from_str(&pool.vault_a)?);
                let vault_ata_underlying_b =
                    MeteoraVaults::derive_vault_ata_pk(&Pubkey::from_str(&pool.vault_b)?);
                let lp_info_dooot = Dooot::LPInfo(LPInfoDooot {
                    time,
                    curve_type: pool.curve_type.clone(),
                    lp_mint: Some(pool.lp_mint.clone()),
                    pool: pk.to_string(),
                    underlyings: vec![
                        LPInfoUnderlyingMintVault {
                            mint: pool.token_a_mint.clone(),
                            vault: vault_ata_underlying.to_string(),
                        },
                        LPInfoUnderlyingMintVault {
                            mint: pool.token_b_mint.clone(),
                            vault: vault_ata_underlying_b.to_string(),
                        },
                    ],
                    program: METEORA_AMM_PROGRAM_ID.to_string(),
                });

                doots.push(lp_info_dooot);
            }

            // PartialEq check on UiTransactionTokenBalance
            let a_changed = post_a.is_some() && pre_a != post_a;
            let b_changed = post_b.is_some() && pre_b != post_b;

            if !a_changed && !b_changed {
                continue;
            }

            // Avoiding `unwrap`s even though we've checked for Some above
            let vault_a = post_a.context(
                "UNREACHABLE: Vault A post balance is None, but it should have been Some (checked above)",
            )?;
            let vault_b = post_b.context(
                "UNREACHABLE: Vault B post balance is None, but it should have been Some (checked above)",
            )?;

            let vault_a_mint = &vault_a.mint;
            let vault_b_mint = &vault_b.mint;
            vault_ata_infos.insert(vault_ata_a, vault_a_mint.clone());
            vault_ata_infos.insert(vault_ata_b, vault_b_mint.clone());

            let vault_a_amount = vault_a.ui_token_amount.amount.parse::<u64>()?;
            let vault_b_amount = vault_b.ui_token_amount.amount.parse::<u64>()?;

            // At least one of the vaults has changed, get lp_supply and emit a new MintUnderlying
            let Some(lp_supply) = txn.get_token_supply(&pool.lp_mint) else {
                continue;
            };

            let (amt_a_per_parent, amt_b_per_parent) = if lp_supply == 0 {
                (0.0, 0.0)
            } else {
                let amt_a_per_parent = (vault_a_amount as f64) / (lp_supply as f64);
                let amt_b_per_parent = (vault_b_amount as f64) / (lp_supply as f64);
                (amt_a_per_parent, amt_b_per_parent)
            };

            let mint_underlying_dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                mint_pubkey: pool.lp_mint.clone(),
                platform_program_pubkey: METEORA_AMM_PROGRAM_ID.to_string(),
                mints: vec![vault_a_mint.clone(), vault_b_mint.clone()],
                mints_qty_per_one_parent: vec![amt_a_per_parent, amt_b_per_parent],
                time,
                total_underlying_amounts: vec![vault_a_amount.into(), vault_b_amount.into()],
                discriminant_id: pk.to_string(),
            });

            doots.push(mint_underlying_dooot);
        }

        let tree = txn.get_ix_tree()?;
        let root = tree.root().context("No root node found in ix tree")?;
        let ix_iter = root.traverse_level_order().skip(1);
        let sig = txn.get_signature();

        for node in ix_iter {
            let ix = node.data().context("No ix found in node")?;
            let Some(ix) = get_partially_decoded_variant(ix) else {
                continue;
            };

            if ix.program_id != METEORA_AMM_PROGRAM_ID {
                continue;
            }

            let Some((pool_pk, relevant_pool)) = ix
                .accounts
                .iter()
                .find(|&a| pools.contains_key(a.as_str()))
                .and_then(|p_pk| Some((p_pk.clone(), pools.get(p_pk.as_str())?)))
            else {
                // Ix does not involve a Meteora AMM pool
                continue;
            };

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
                .and_then(|ix| match ix {
                    UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(p_ix)) => {
                        Some(p_ix.program_id.clone())
                    }
                    _ => None,
                });

            let lp_mint = &relevant_pool.lp_mint;
            let token_a_mint = &relevant_pool.token_a_mint;
            let token_b_mint = &relevant_pool.token_b_mint;
            let ixs_scan = node
                .traverse_level_order()
                .skip(1)
                .filter_map(|kid| *kid.data());
            log::trace!(
                "\nToken A: {}\nToken B: {}\nLP: {}\nUser: {}\nSig: {}",
                token_a_mint,
                token_b_mint,
                lp_mint,
                user,
                sig
            );

            // Putting all mints in both the `in_mints` and `out_mints` params give us the full picture of what happend
            // Allowing us to determine if something was a swap or liq change, without looking directly at instructions
            let txfer_amounts = txn.get_total_transfer_amounts_for_pubkey_ui_ixs(
                ixs_scan,
                &[user],
                &[lp_mint, token_a_mint, token_b_mint],
                &[lp_mint, token_a_mint, token_b_mint],
            )?;

            log::debug!("Transfer amounts: {:?}", txfer_amounts);

            let lp_mint_direction = txfer_amounts.get_direction(lp_mint, user);
            let token_a_direction = txfer_amounts.get_direction(token_a_mint, user);
            let token_b_direction = txfer_amounts.get_direction(token_b_mint, user);

            match (lp_mint_direction, token_a_direction, token_b_direction) {
                (_, TransferDirection::Zero, TransferDirection::Zero) => {
                    // No underlyings moved, no need to emit any events
                    // We don't care about LP Mint in this case
                    continue;
                }
                (TransferDirection::Receive | TransferDirection::Send, _, _) => {
                    // Exchange of lp tokens with underlyings, emit liquidity change event
                    // get_in_amount will be either positive or negative depending on the direction
                    // This is relative to the `user`, but can also be flipped to be relative to the pool
                    // Deposit => positive change, Withdraw => negative change
                    let change_amt_a = txfer_amounts.get_in_amount_i128(token_a_mint, user);
                    let change_amt_b = txfer_amounts.get_in_amount_i128(token_b_mint, user);
                    if change_amt_a == 0 && change_amt_b == 0 {
                        continue;
                    }

                    let liq_change_dooot = Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                        amounts: vec![change_amt_a.into(), change_amt_b.into()],
                        mints: vec![token_a_mint.clone(), token_b_mint.clone()],
                        owner: user.to_string(),
                        pool: pool_pk,
                        time,
                    });

                    doots.push(liq_change_dooot);
                }
                (TransferDirection::Zero, dir_a, dir_b) => {
                    // No LP tokens moved, but underlying tokens did, this is a Swap, or a Fee Claim
                    let mut in_mint = token_a_mint;
                    let mut out_mint = token_b_mint;

                    let mut in_amount = txfer_amounts.get_in_amount_i128(in_mint, user);
                    let mut out_amount = txfer_amounts.get_out_amount_i128(out_mint, user);

                    if dir_a == dir_b {
                        // Fee Claim, emit liq change for both mints
                        let abs_in = in_amount.abs();
                        let abs_out = out_amount.abs();
                        let sign = match dir_a {
                            TransferDirection::Receive => -1,
                            TransferDirection::Send => 1,
                            TransferDirection::Zero => 0,
                        };
                        let amt_a = abs_in * sign;
                        let amt_b = abs_out * sign;
                        let liq_change_dooot =
                            Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                                amounts: vec![amt_a.into(), amt_b.into()],
                                mints: vec![token_a_mint.clone(), token_b_mint.clone()],
                                owner: user.to_string(),
                                pool: pool_pk,
                                time,
                            });

                        doots.push(liq_change_dooot);
                    } else {
                        if in_amount == 0 || out_amount == 0 {
                            continue;
                        }

                        // Either `in` or `out` can be negative, but we only need to check one
                        // to see if the swap is in the opposite direction
                        if in_amount < 0 || out_amount < 0 {
                            // Swap is in the opposite direction, swap the mints and amounts, and flip signs
                            std::mem::swap(&mut in_mint, &mut out_mint);
                            std::mem::swap(&mut in_amount, &mut out_amount);
                            in_amount = -in_amount;
                            out_amount = -out_amount;
                        }

                        if in_amount < 0 || out_amount < 0 {
                            // An amount is still negative after doing the swap and abs()
                            continue;
                        }

                        let fees = if let Some(fee_data) = swap_events.pop_front() {
                            vec![SwapFees {
                                mint: in_mint.to_string(),
                                amount: Decimal::from(fee_data.trade_fee),
                            }]
                        } else {
                            vec![]
                        };

                        let swap_dooot = Dooot::SwapEvent(SwapEventDooot {
                            time,
                            in_amount: in_amount.into(),
                            in_mint_pubkey: in_mint.clone(),
                            out_amount: out_amount.into(),
                            out_mint_pubkey: out_mint.clone(),
                            program_pubkey: METEORA_AMM_PROGRAM_ID.to_string(),
                            swap_auth_pubkey: user.to_string(),
                            market_pubkey: Some(pool_pk.to_string()),
                            parent_amm_program_pubkey: parent_program_id,
                            fee_mint_is_in_mint: None,
                            referring_platform_account_pubkey: None,
                            platform_referral_fee: None,
                            fees,
                            slippage_bps: None,
                            tx_sig: txn.get_signature().to_string(),
                        });

                        doots.push(swap_dooot);
                    }
                }
            }
        }

        Ok(())
    }
}

impl MeteoraAMM {
    pub fn get_pool_curve_type(data: &[u8]) -> Result<dooot::CurveType> {
        if data.len() < 875 {
            return Err(anyhow::anyhow!(
                "meteora amm curve_type reading: data is too short"
            ));
        }
        let curve_type_byte = &data[874];
        let curve_type = meteora_amm_gen::CurveType::try_from(*curve_type_byte)?;
        Ok(curve_type.into())
    }
}

impl StateSource for MeteoraAMM {
    fn get_name(&self) -> &'static str {
        "MeteoraAMM"
    }

    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl crate::state_source::AccountProvider,
        tx: std::sync::mpsc::SyncSender<Dooot>,
    ) -> Result<()> {
        #[derive(Debug)]
        struct MeteoraAMMPoolInfoPK {
            pub lp_mint: Pubkey,
            pub token_a_mint: Pubkey,
            pub token_b_mint: Pubkey,
            pub vault_a: Pubkey,
            pub vault_b: Pubkey,
            pub vault_a_lp: Pubkey,
            pub vault_b_lp: Pubkey,
            pub curve_type: dooot::CurveType,
        }

        let blocktime = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let mut pools = HashMap::new();
        let mut vault_mints = HashMap::new();
        let mut lp_supplies_mints = HashSet::new();
        let mut lp_supplies = HashMap::new();
        let mut token_account_balances = HashMap::new();

        let meteora_accounts = acct_provider.all_accounts();

        for acct in meteora_accounts {
            let owner = acct.account.owner().to_string();
            if owner == METEORA_AMM_PROGRAM_ID {
                // Parse Meteora AMM pool, only extracting what we need
                let data = acct.account.data();
                let Some(disc) = data.get(..8) else {
                    continue;
                };
                if disc != POOL_ACCOUNT_DISCRIMINATOR {
                    continue;
                }
                let mut reader = StepAccountReader::new(data);
                reader.advance_unclamped(8);
                let lp_mint_pk = reader.read_next_pubkey()?;
                let token_a_mint_pk = reader.read_next_pubkey()?;
                let token_b_mint_pk = reader.read_next_pubkey()?;
                let vault_a_pk = reader.read_next_pubkey()?;
                let vault_b_pk = reader.read_next_pubkey()?;
                let vault_a_lp_pk = reader.read_next_pubkey()?;
                let vault_b_lp_pk = reader.read_next_pubkey()?;

                let curve_type = Self::get_pool_curve_type(data)?;

                let info = MeteoraAMMPoolInfoPK {
                    lp_mint: *lp_mint_pk,
                    token_a_mint: *token_a_mint_pk,
                    token_b_mint: *token_b_mint_pk,
                    vault_a: *vault_a_pk,
                    vault_b: *vault_b_pk,
                    vault_a_lp: *vault_a_lp_pk,
                    vault_b_lp: *vault_b_lp_pk,
                    curve_type,
                };

                pools.insert(acct.pubkey, info);
                lp_supplies_mints.insert(*lp_mint_pk);
                token_account_balances.insert(*vault_a_lp_pk, 0u64);
                token_account_balances.insert(*vault_b_lp_pk, 0u64);
            } else if owner == METEORA_VAULTS_PROGRAM_ID {
                // Parse Meteora Vaults, only extracting what we need
                let data = acct.account.data();
                let Some(disc) = data.get(..8) else {
                    continue;
                };
                if disc != VAULT_ACCOUNT_DISCRIMINATOR {
                    continue;
                }
                let vault = Vault::deserialize(&mut &data[8..])?;

                vault_mints.insert(acct.pubkey, vault.lp_mint);
            }
        }

        // Now iterate through all token accounts and mints, pulling mint supplies and balances
        let token_program_accounts = acct_provider.all_accounts();

        for acct in token_program_accounts {
            let pk = acct.pubkey;
            if let std::collections::hash_map::Entry::Occupied(mut e) =
                token_account_balances.entry(pk)
            {
                let data = acct.account.data();
                let balance_bytes = &data[64..72].try_into()?;
                let balance = u64::from_le_bytes(*balance_bytes);
                e.insert(balance);
            } else if lp_supplies_mints.contains(&pk) {
                let data = acct.account.data();
                let supply_bytes = &data[36..44].try_into()?;
                let owner = acct.account.owner().to_string();
                let supply = u64::from_le_bytes(*supply_bytes);
                lp_supplies.insert(pk, (owner, supply));
            }
        }

        // just do a supply for each lp mint
        for (lp_mint_pk, (owner, supply)) in &lp_supplies {
            let spl_version = get_spl_version_from_program_id(owner);
            let mint_supply_dooot = Dooot::MintSupplyGlobal(MintSupplyGlobalDooot {
                block_time: blocktime,
                mint: lp_mint_pk.to_string(),
                total_supply: (*supply).into(),
                spl_version,
            });
            tx.send(mint_supply_dooot)?;
        }

        // Get all dooots from pools
        for (pk, pool) in pools {
            // GState first
            let lp_info_dooot = Dooot::LPInfo(LPInfoDooot {
                time: blocktime,
                curve_type: pool.curve_type,
                lp_mint: Some(pool.lp_mint.to_string()),
                pool: pk.to_string(),
                program: METEORA_AMM_PROGRAM_ID.to_string(),
                underlyings: vec![
                    LPInfoUnderlyingMintVault {
                        mint: pool.token_a_mint.to_string(),
                        vault: pool.vault_a.to_string(),
                    },
                    LPInfoUnderlyingMintVault {
                        mint: pool.token_b_mint.to_string(),
                        vault: pool.vault_b.to_string(),
                    },
                ],
            });

            tx.send(lp_info_dooot)?;

            // MintUnderlyings
            let (_, lp_supply) = lp_supplies
                .get(&pool.lp_mint)
                .context("Missing lp supply")?;
            let vault_a_balance = token_account_balances
                .get(&pool.vault_a_lp)
                .context("Missing vault a balance")?;
            let vault_b_balance = token_account_balances
                .get(&pool.vault_b_lp)
                .context("Missing vault b balance")?;

            let (amt_a_per_parent, amt_b_per_parent) = if *lp_supply == 0 {
                (0.0, 0.0)
            } else {
                let amt_a_per_parent = (*vault_a_balance as f64) / (*lp_supply as f64);
                let amt_b_per_parent = (*vault_b_balance as f64) / (*lp_supply as f64);
                (amt_a_per_parent, amt_b_per_parent)
            };

            let vault_vtoken_mint_a = vault_mints
                .get(&pool.vault_a)
                .context("Missing vToken vault mint a")?
                .to_string();
            let vault_vtoken_mint_b = vault_mints
                .get(&pool.vault_b)
                .context("Missing vToken vault mint b")?
                .to_string();

            let mint_underlying_dooot = Dooot::MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot {
                mint_pubkey: pool.lp_mint.to_string(),
                platform_program_pubkey: METEORA_AMM_PROGRAM_ID.to_string(),
                mints: vec![vault_vtoken_mint_a, vault_vtoken_mint_b],
                mints_qty_per_one_parent: vec![amt_a_per_parent, amt_b_per_parent],
                time: blocktime,
                total_underlying_amounts: vec![
                    (*vault_a_balance).into(),
                    (*vault_b_balance).into(),
                ],
                discriminant_id: pk.to_string(),
            });

            tx.send(mint_underlying_dooot)?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct MeteoraAMMPoolInfo {
    pub lp_mint: String,
    pub token_a_mint: String,
    pub token_b_mint: String,
    pub vault_a: String,
    pub vault_b: String,
    pub vault_a_lp: String,
    pub vault_b_lp: String,
    pub was_created: bool,
    pub curve_type: dooot::CurveType,
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rust_decimal::Decimal;
    use step_ingestooor_sdk::{
        decoded_transaction::DecodedTransaction,
        dooot::{CurveType, Dooot, LiquidityChangeEventDooot},
        trait_defs::transaction::TransactionLike,
        utils::test_utils::init_test_extras_bare,
    };

    use crate::{MeteoraAMM, TransactionParserTrait};

    #[test]
    fn stable_curve_type_found() {
        let file = fs::read_to_string("./tests/meteora_amm/swap_over_stable.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let data = txn
            .get_account_data_after("32D4zRxNc1EssbJieVHfPhZM3rH6CzfUPrWUuWxD9prG")
            .unwrap()
            .unwrap();
        let curve_type = MeteoraAMM::get_pool_curve_type(&data).unwrap();
        assert_eq!(curve_type, CurveType::Stable);
    }

    #[test]
    fn get_swap_amounts() {
        let txn_file = fs::read_to_string("./tests/meteora_amm/meteora_amm.json").unwrap();
        let txn = serde_json::from_str::<DecodedTransaction>(&txn_file).unwrap();

        let swap_ix_begin = 18;
        let swap_ix_end = 26;

        let meteoa_swap_ixs =
            &txn.get_flattened_ixs().collect::<Vec<_>>()[swap_ix_begin..swap_ix_end];

        let ixs_iter = meteoa_swap_ixs.iter();
        let in_mint = "So11111111111111111111111111111111111111112";
        let out_mint = "5xpJLFHGPz7kpqYkifVNhLxTFb4L7N8rm7hvu4stAewi";
        let lp_mint = "HBmBSARs1hTM1278263xN5CQ8NMB1i55GS34t9FKR3ia";
        let user = "CZwSozPveXDEajkX1vyVF1yUTrJEMMBZcyXMXrsgFqke";

        let amts = txn
            .get_total_transfer_amounts_for_pubkey_flattened_ixs(
                ixs_iter,
                &[user],
                &[lp_mint, in_mint, out_mint],
                &[lp_mint, in_mint, out_mint],
            )
            .unwrap();

        let amt_in_pos = amts.get_total_in_amount_i128(in_mint) > 0;
        let amt_out_pos = amts.get_total_out_amount_i128(out_mint) > 0;
        let amt_lp_zero = amts.get_total_in_amount_i128(lp_mint) == 0
            && amts.get_total_out_amount_i128(lp_mint) == 0;

        assert!(amt_in_pos);
        assert!(amt_out_pos);
        assert!(amt_lp_zero);
    }

    #[tokio::test]
    async fn curve_type_byte() {
        let file = fs::read_to_string("./tests/meteora_amm/bad_curve_bytes.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let parser = MeteoraAMM {};
        let (step_utils, _) = init_test_extras_bare().await;

        parser.invoke(&txn, &mut vec![], &step_utils).await.unwrap();
    }

    #[tokio::test]
    async fn swap_neg() {
        let file = fs::read_to_string("./tests/meteora_amm/meteora_swap_neg.json").unwrap();
        let txn = serde_json::from_str::<DecodedTransaction>(&file).unwrap();
        let parser = MeteoraAMM {};
        let (step_utils, _) = init_test_extras_bare().await;
        let mut dooots = vec![];

        parser.invoke(&txn, &mut dooots, &step_utils).await.unwrap();

        let dooot = dooots.pop().unwrap();

        if let Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot { amounts, .. }) = dooot {
            let amt_a = amounts[0];
            let amt_b = amounts[1];
            assert!(amt_a < Decimal::ZERO);
            assert!(amt_b < Decimal::ZERO);
        } else {
            panic!("dooot is not a LiquidityChangeEventDooot");
        }
    }

    #[tokio::test]
    async fn swap_mint_underlyings() {
        let file = fs::read_to_string("./tests/meteora_amm/swap_mu.json").unwrap();
        let json = serde_json::from_str(&file).unwrap();
        let txn = DecodedTransaction::from_rabbit_message_json(json).unwrap();
        let parser = MeteoraAMM {};
        let (step_utils, _) = init_test_extras_bare().await;
        let mut d = vec![];

        parser.invoke(&txn, &mut d, &step_utils).await.unwrap();

        assert!(!d.is_empty());
    }
}
