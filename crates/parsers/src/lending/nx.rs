use crate::state_source::{AccountProvider, StateSource};
use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::Result;
use async_trait::async_trait;
use borsh::BorshDeserialize;
use solana_sdk::account::ReadableAccount;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::mpsc::SyncSender;
use step_ingestooor_idl_gens::nx_lending_gen::nx_lending::accounts::{
    fragmetricPool, fragmetricPosition, fragmetricUser, lendingAccount, lendingPool, position,
    solayerPool, vSolPool, vSolPosition,
};
use step_ingestooor_idl_gens::nx_leverage_gen::nx_leverage::accounts::{
    jupiterAccount, marginAccount, marginPool,
};
use step_ingestooor_sdk::dooot::{PsNxLendFragmetricPoolDooot, PsNxLendFragmetricUserDooot};
use step_ingestooor_sdk::utils::util_fns::convert_blocktime_to_micros;
use step_ingestooor_sdk::{
    dooot::{
        nx::{
            CollateralDetail, InterestRateData, LoanDetail, PositionType,
            PsNxLendLendingAccountDooot, PsNxLendLendingPoolDooot, PsNxLendPositionDooot,
            PsNxLendSolayerPoolDooot, PsNxLendVSolPoolDooot, PsNxLendVSolPositionDooot,
            PsNxLeverageJupiterAccountDooot, PsNxLeverageMarginAccountDooot,
            PsNxLeverageMarginPoolDooot, SwapTemp, VSolPositionDetail,
        },
        Dooot,
    },
    trait_defs::transaction::TransactionLike,
    utils::step_utils::StepUtils,
};

const NX_LEVERAGE_PROGRAM: &str = "EHBN9YKtMmrZhj8JZqyBQRGqyyeHw5xUB1Q5eAHszuMt";
const NX_LENDING_PROGRAM: &str = "NXFiKimQN3QSL3CDhCXddyVmLfrai8HK36bHKaAzK7g";
const DISC_BYTE: usize = 8;
const MARGIN_ACC_DISC: &[u8] = &[0x85, 0xdc, 0xad, 0xd5, 0xb3, 0xd3, 0x2b, 0xee];
const JUPITER_ACC_DISC: &[u8] = &[0xb6, 0x3b, 0x7d, 0x09, 0x7f, 0xd1, 0xe8, 0x03];
const LENDING_ACC_DISC: &[u8] = &[0xe4, 0x5e, 0x82, 0x51, 0xbb, 0xb6, 0xe6, 0x55];
const MARGIN_POOL_DISC: &[u8] = &[0x8e, 0xff, 0x1c, 0x20, 0xc4, 0xa8, 0xaa, 0xaf];
const LENDING_POOL_DISC: &[u8] = &[0xd0, 0x28, 0xf2, 0x52, 0xba, 0x12, 0x4b, 0x24];
const SOLAYER_POOL_DISC: &[u8] = &[0x65, 0xc0, 0x27, 0xbf, 0x34, 0xa5, 0x46, 0xf6];
const VSOL_POOL_DISC: &[u8] = &[0x4a, 0x4c, 0xfc, 0x0d, 0xcf, 0x80, 0x41, 0xda];
const FRAG_POOL_DISC: &[u8] = &[0x4b, 0xb2, 0x7f, 0x2c, 0x80, 0x21, 0x14, 0x9a];
const FRAG_POSITION_ACC_DISC: &[u8] = &[0xd6, 0xf2, 0x8f, 0xea, 0x92, 0x1b, 0x15, 0x01];
const FRAG_USER_ACC_DISC: &[u8] = &[0xde, 0x55, 0x3e, 0xdb, 0x33, 0xdc, 0xae, 0x78];
const SOLAYER_POSITION_ACC_DISC: &[u8] = &[0xaa, 0xbc, 0x8f, 0xe4, 0x7a, 0x40, 0xf7, 0xd0];
const VSOL_POSITION_ACC_DISC: &[u8] = &[0x73, 0xc8, 0x9c, 0x26, 0xc2, 0xa1, 0xc0, 0x35];

pub struct Nx;

#[async_trait]
impl TransactionParserTrait for Nx {
    fn get_name(&self) -> &'static str {
        "Nx"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![
            TransactionParserFilter::ProgramId(NX_LENDING_PROGRAM.to_string()),
            TransactionParserFilter::ProgramId(NX_LEVERAGE_PROGRAM.to_string()),
        ]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        let time = txn.get_block_time();
        let accts_iter = txn.iter_accounts().filter(|a| matches!(a.post_owner_program, Some(p) if p == NX_LENDING_PROGRAM || p == NX_LEVERAGE_PROGRAM)
        );

        for acct in accts_iter {
            let Some(data) = acct.unpack_post_datum()? else {
                log::warn!(
                    "No post datum for MarginFi acct {}, may be closed",
                    acct.pubkey
                );
                continue;
            };

            let disc = &data[..DISC_BYTE];
            match disc {
                FRAG_USER_ACC_DISC => {
                    let info = fragmetricUser::deserialize(&mut &data[DISC_BYTE..])?;

                    let ps_nx_frag_user_dooot = PsNxLendFragmetricUserDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        owner: info.owner.to_string(),
                        receipt_token: info.receipt_token.to_string(),
                        amount: info.amount,
                        nx_fragmetric_points: info.nx_fragmetric_points,
                        last_update_time: info.last_update_time,
                    };
                    dooot.push(Dooot::PsNxLendFragmetricUser(ps_nx_frag_user_dooot));
                }
                FRAG_POOL_DISC => {
                    let info = fragmetricPool::deserialize(&mut &data[DISC_BYTE..])?;

                    let ps_nx_frag_pool_dooot = PsNxLendFragmetricPoolDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        receipt_token: info.receipt_token.to_string(),
                        amount: info.amount,
                        total_nx_fragmetric_points: info.total_nx_fragmetric_points,
                        last_update_time: info.last_update_time,
                        padding: info.padding,
                    };
                    dooot.push(Dooot::PsNxLendFragmetricPool(ps_nx_frag_pool_dooot));
                }
                FRAG_POSITION_ACC_DISC => {
                    let info = fragmetricPosition::deserialize(&mut &data[DISC_BYTE..])?;

                    let positions = info.positions;
                    for pos in positions {
                        let ps_nx_position_dooot = PsNxLendPositionDooot {
                            time,
                            account_pubkey: acct.pubkey.to_string(),
                            collateral_mint: pos.collateral_mint.to_string(),
                            borrow_mint: pos.borrow_mint.to_string(),
                            leverage_mint: info.nx_market.to_string(),
                            collateral_tokens: pos.collateral_tokens,
                            borrow_tokens: pos.borrow_tokens,
                            collateral_note: pos.collateral_note,
                            borrow_note: pos.borrow_note,
                            leverage_note: pos.leverage_note,
                            leverage_tokens: pos.leverage_tokens,
                            liquidation_flag: pos.liquidation_flag,
                            leverage_multiples: pos.leverage_multiples,
                            position_type: PositionType::Fragmetric.to_string(),
                        };
                        dooot.push(Dooot::PsNxLendPosition(ps_nx_position_dooot));
                    }
                }
                SOLAYER_POSITION_ACC_DISC => {
                    let info = position::deserialize(&mut &data[DISC_BYTE..])?;

                    let positions = info.positions;
                    for pos in positions {
                        let ps_nx_position_dooot = PsNxLendPositionDooot {
                            time,
                            account_pubkey: acct.pubkey.to_string(),
                            collateral_mint: pos.collateral_mint.to_string(),
                            borrow_mint: pos.borrow_mint.to_string(),
                            leverage_mint: info.nx_market.to_string(),
                            collateral_tokens: pos.collateral_tokens,
                            borrow_tokens: pos.borrow_tokens,
                            collateral_note: pos.collateral_note,
                            borrow_note: pos.borrow_note,
                            leverage_note: pos.leverage_note,
                            leverage_tokens: pos.leverage_tokens,
                            liquidation_flag: pos.liquidation_flag,
                            leverage_multiples: pos.leverage_multiples,
                            position_type: PositionType::Solayer.to_string(),
                        };
                        dooot.push(Dooot::PsNxLendPosition(ps_nx_position_dooot));
                    }
                }
                VSOL_POSITION_ACC_DISC => {
                    let info = vSolPosition::deserialize(&mut &data[DISC_BYTE..])?;

                    let positions = info.positions;
                    let mut v_sol_positions = Vec::new();
                    for pos in positions {
                        v_sol_positions.push(VSolPositionDetail {
                            collateral_mint: pos.collateral_mint.to_string(),
                            borrow_mint: pos.borrow_mint.to_string(),
                            leverage_mint: pos.leverage_mint.to_string(),
                            collateral_note: pos.collateral_note,
                            collateral_tokens: pos.collateral_tokens,
                            borrow_note: pos.borrow_note,
                            borrow_tokens: pos.borrow_tokens,
                            leverage_note: pos.leverage_note,
                            leverage_tokens: pos.leverage_tokens,
                            leverage_multiples: pos.leverage_multiples,
                            last_points_and_leverage_notes_rate: pos
                                .last_points_and_leverage_notes_rate,
                            point_reward: pos.point_reward,
                        });
                    }
                    let swap_temp_detail = SwapTemp {
                        before_amount: info.swap_temp.before_amount,
                        expected: info.swap_temp.expected,
                    };

                    let ps_nx_vsol_position_dooot = PsNxLendVSolPositionDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        owner: info.owner.to_string(),
                        positions: v_sol_positions,
                        swap_temp: swap_temp_detail,
                    };
                    dooot.push(Dooot::PsNxLendVSolPosition(ps_nx_vsol_position_dooot));
                }
                MARGIN_ACC_DISC => {
                    let info = marginAccount::deserialize(&mut &data[DISC_BYTE..])?;

                    let mut deposits_vec = Vec::new();
                    for deposit in &info.deposits {
                        let collateral_detail = CollateralDetail {
                            token_mint: deposit.token_mint.to_string(),
                            deposit_note: deposit.deposit_note,
                            deposit_token: deposit.deposit_token,
                            market_value: deposit.market_value,
                        };
                        deposits_vec.push(collateral_detail);
                    }

                    let mut loans_vec = Vec::new();
                    for loan in &info.loans {
                        let loan_detail = LoanDetail {
                            token_mint: loan.token_mint.to_string(),
                            loan_note: loan.loan_note,
                            loan_token: loan.loan_token,
                        };
                        loans_vec.push(loan_detail);
                    }
                    let swap_temp_detail = SwapTemp {
                        before_amount: info.swap_temp.before_amount,
                        expected: info.swap_temp.expected,
                    };

                    let ps_nx_margin_account_dooot = PsNxLeverageMarginAccountDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        leveragefi: info.leveragefi.to_string(),
                        owner: info.owner.to_string(),
                        deposits: deposits_vec,
                        loans: loans_vec,
                        leverage: info.leverage,
                        jlp_notes: info.jlp_notes,
                        active_loan: info.active_loan.to_string(),
                        active_collateral: info.active_collateral.to_string(),
                        swap_temp: swap_temp_detail,
                        account_type: info.account_type,
                        liquidation_flag: info.liquidation_flag,
                    };
                    dooot.push(Dooot::PsNxLeverageMarginAccount(ps_nx_margin_account_dooot));
                }
                MARGIN_POOL_DISC => {
                    let info = marginPool::deserialize(&mut &data[DISC_BYTE..])?;
                    let ps_nx_margin_pool_dooot = PsNxLeverageMarginPoolDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        leveragefi: info.leveragefi.to_string(),
                        vault: info.vault.to_string(),
                        fee_destination: info.fee_destination.to_string(),
                        pool_authority: info.pool_authority.to_string(),
                        token_mint: info.token_mint.to_string(),
                        token_price_oracle: info.token_price_oracle.to_string(),
                        borrowed_tokens: info.borrowed_tokens,
                        deposit_tokens: info.deposit_tokens,
                        deposit_notes: info.deposit_notes,
                        loan_notes: info.loan_notes,
                        deposit_interest: info.deposit_interest,
                        loan_interest: info.loan_interest,
                        protocol_fee: info.protocol_fee,
                        accrued_until: info.accrued_until,
                        utilization_flag: info.utilization_flag,
                    };
                    dooot.push(Dooot::PsNxLeverageMarginPool(ps_nx_margin_pool_dooot));
                }
                JUPITER_ACC_DISC => {
                    // "The jupiter account to receive the leveraged loan tokens" -global
                    let info = jupiterAccount::deserialize(&mut &data[DISC_BYTE..])?;
                    let ps_nx_jupiter_account_dooot = PsNxLeverageJupiterAccountDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        leveragefi: info.leveragefi.to_string(),
                        admin: info.admin.to_string(),
                        jlp_mint: info.jlp_mint.to_string(),
                        jlp_account: info.jlp_account.to_string(),
                        jupiter_notes: info.jupiter_notes,
                        jlp_position: info.jlp_position,
                    };
                    dooot.push(Dooot::PsNxLeverageJupiterAccount(
                        ps_nx_jupiter_account_dooot,
                    ));
                }
                LENDING_POOL_DISC => {
                    let info = lendingPool::deserialize(&mut &data[DISC_BYTE..])?;
                    let mut rates = Vec::new();
                    for rate in info.interest_rate_configs {
                        rates.push(InterestRateData {
                            utilization_rate: rate.utilization_rate,
                            k_value: rate.k_value,
                            b_value: rate.b_value,
                        });
                    }
                    let ps_nx_lending_pool_dooot = PsNxLendLendingPoolDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        token_mint: info.token_mint.to_string(),
                        borrow_tokens: info.borrow_tokens,
                        borrow_notes: info.borrow_notes,
                        deposit_tokens: info.deposit_tokens,
                        deposit_notes: info.deposit_notes,
                        deposit_interest: info.deposit_interest,
                        borrow_interest: info.borrow_interest,
                        protocol_fee: info.protocol_fee,
                        accrued_until: info.accrued_until,
                        utilization_flag: info.utilization_flag,
                        interest_rate_configs: rates,
                    };
                    dooot.push(Dooot::PsNxLendLendingPool(ps_nx_lending_pool_dooot));
                }
                LENDING_ACC_DISC => {
                    let info = lendingAccount::deserialize(&mut &data[DISC_BYTE..])?;
                    let ps_nx_lending_account_dooot = PsNxLendLendingAccountDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        owner: info.owner.to_string(),
                        deposit_notes: info.deposit_notes,
                        deposit_tokens: info.deposit_tokens,
                        last_note_rate: info.last_note_rate,
                        total_reward: info.total_reward,
                    };
                    dooot.push(Dooot::PsNxLendLendingAccount(ps_nx_lending_account_dooot));
                }
                VSOL_POOL_DISC => {
                    let info = vSolPool::deserialize(&mut &data[DISC_BYTE..])?;
                    let ps_nx_vsol_pool_dooot = PsNxLendVSolPoolDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        vsol_leverage_tokens: info.vsol_leverage_tokens,
                        vsol_leverage_notes: info.vsol_leverage_notes,
                        vsol_points: info.vsol_points,
                        last_vsol_point_per_leverage_note: info.last_vsol_point_per_leverage_note,
                        vsol_points_fee_claimable: info.vsol_points_fee_claimable,
                        last_vsol_point_update_time: info.last_vsol_point_update_time,
                    };
                    dooot.push(Dooot::PsNxLendVSolPool(ps_nx_vsol_pool_dooot));
                }
                SOLAYER_POOL_DISC => {
                    let info = solayerPool::deserialize(&mut &data[DISC_BYTE..])?;
                    let ps_nx_solayer_pool_dooot = PsNxLendSolayerPoolDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        lrt_mint: info.lrt_mint.to_string(),
                        amount: info.amount,
                        total_nx_solayer_points: info.total_nx_solayer_points,
                        last_update_time: info.last_update_time,
                    };
                    dooot.push(Dooot::PsNxLendSolayerPool(ps_nx_solayer_pool_dooot));
                }
                _ => continue,
            }
        }

        Ok(())
    }
}

impl StateSource for Nx {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)?;
        let lending_iter =
            acct_provider.program_accounts(Pubkey::from_str(NX_LENDING_PROGRAM)?, None);

        let mut all_doots: Vec<_> = vec![];

        for acct in lending_iter {
            let data = acct.account.data();
            let disc = &data[..DISC_BYTE];

            match disc {
                FRAG_USER_ACC_DISC => {
                    let info = fragmetricUser::deserialize(&mut &data[DISC_BYTE..])?;
                    let ps_nx_frag_user_dooot = PsNxLendFragmetricUserDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        owner: info.owner.to_string(),
                        receipt_token: info.receipt_token.to_string(),
                        amount: info.amount,
                        nx_fragmetric_points: info.nx_fragmetric_points,
                        last_update_time: info.last_update_time,
                    };
                    all_doots.push(Dooot::PsNxLendFragmetricUser(ps_nx_frag_user_dooot));
                }
                FRAG_POOL_DISC => {
                    let info = fragmetricPool::deserialize(&mut &data[DISC_BYTE..])?;
                    let ps_nx_frag_pool_dooot = PsNxLendFragmetricPoolDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        receipt_token: info.receipt_token.to_string(),
                        amount: info.amount,
                        total_nx_fragmetric_points: info.total_nx_fragmetric_points,
                        last_update_time: info.last_update_time,
                        padding: info.padding,
                    };
                    all_doots.push(Dooot::PsNxLendFragmetricPool(ps_nx_frag_pool_dooot));
                }
                FRAG_POSITION_ACC_DISC => {
                    let info = fragmetricPosition::deserialize(&mut &data[DISC_BYTE..])?;
                    let positions = info.positions;
                    for pos in positions {
                        let ps_nx_position_dooot = PsNxLendPositionDooot {
                            time,
                            account_pubkey: acct.pubkey.to_string(),
                            collateral_mint: pos.collateral_mint.to_string(),
                            borrow_mint: pos.borrow_mint.to_string(),
                            leverage_mint: info.nx_market.to_string(),
                            collateral_tokens: pos.collateral_tokens,
                            borrow_tokens: pos.borrow_tokens,
                            collateral_note: pos.collateral_note,
                            borrow_note: pos.borrow_note,
                            leverage_note: pos.leverage_note,
                            leverage_tokens: pos.leverage_tokens,
                            liquidation_flag: pos.liquidation_flag,
                            leverage_multiples: pos.leverage_multiples,
                            position_type: PositionType::Fragmetric.to_string(),
                        };
                        all_doots.push(Dooot::PsNxLendPosition(ps_nx_position_dooot));
                    }
                }
                SOLAYER_POSITION_ACC_DISC => {
                    let info = position::deserialize(&mut &data[DISC_BYTE..])?;
                    let positions = info.positions;
                    for pos in positions {
                        let ps_nx_position_dooot = PsNxLendPositionDooot {
                            time,
                            account_pubkey: acct.pubkey.to_string(),
                            collateral_mint: pos.collateral_mint.to_string(),
                            borrow_mint: pos.borrow_mint.to_string(),
                            leverage_mint: info.nx_market.to_string(),
                            collateral_tokens: pos.collateral_tokens,
                            borrow_tokens: pos.borrow_tokens,
                            collateral_note: pos.collateral_note,
                            borrow_note: pos.borrow_note,
                            leverage_note: pos.leverage_note,
                            leverage_tokens: pos.leverage_tokens,
                            liquidation_flag: pos.liquidation_flag,
                            leverage_multiples: pos.leverage_multiples,
                            position_type: PositionType::Solayer.to_string(),
                        };
                        all_doots.push(Dooot::PsNxLendPosition(ps_nx_position_dooot));
                    }
                }
                VSOL_POSITION_ACC_DISC => {
                    let info = vSolPosition::deserialize(&mut &data[DISC_BYTE..])?;
                    let positions = info.positions;
                    let mut v_sol_positions = Vec::new();
                    for pos in positions {
                        v_sol_positions.push(VSolPositionDetail {
                            collateral_mint: pos.collateral_mint.to_string(),
                            borrow_mint: pos.borrow_mint.to_string(),
                            leverage_mint: pos.leverage_mint.to_string(),
                            collateral_note: pos.collateral_note,
                            collateral_tokens: pos.collateral_tokens,
                            borrow_note: pos.borrow_note,
                            borrow_tokens: pos.borrow_tokens,
                            leverage_note: pos.leverage_note,
                            leverage_tokens: pos.leverage_tokens,
                            leverage_multiples: pos.leverage_multiples,
                            last_points_and_leverage_notes_rate: pos
                                .last_points_and_leverage_notes_rate,
                            point_reward: pos.point_reward,
                        });
                    }
                    let swap_temp_detail = SwapTemp {
                        before_amount: info.swap_temp.before_amount,
                        expected: info.swap_temp.expected,
                    };

                    let ps_nx_vsol_position_dooot = PsNxLendVSolPositionDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        owner: info.owner.to_string(),
                        positions: v_sol_positions,
                        swap_temp: swap_temp_detail,
                    };
                    all_doots.push(Dooot::PsNxLendVSolPosition(ps_nx_vsol_position_dooot));
                }
                LENDING_POOL_DISC => {
                    let info = lendingPool::deserialize(&mut &data[DISC_BYTE..])?;
                    let mut rates = Vec::new();
                    for rate in info.interest_rate_configs {
                        rates.push(InterestRateData {
                            utilization_rate: rate.utilization_rate,
                            k_value: rate.k_value,
                            b_value: rate.b_value,
                        });
                    }
                    let ps_nx_lending_pool_dooot = PsNxLendLendingPoolDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        token_mint: info.token_mint.to_string(),
                        borrow_tokens: info.borrow_tokens,
                        borrow_notes: info.borrow_notes,
                        deposit_tokens: info.deposit_tokens,
                        deposit_notes: info.deposit_notes,
                        deposit_interest: info.deposit_interest,
                        borrow_interest: info.borrow_interest,
                        protocol_fee: info.protocol_fee,
                        accrued_until: info.accrued_until,
                        utilization_flag: info.utilization_flag,
                        interest_rate_configs: rates,
                    };
                    all_doots.push(Dooot::PsNxLendLendingPool(ps_nx_lending_pool_dooot));
                }
                LENDING_ACC_DISC => {
                    let info = lendingAccount::deserialize(&mut &data[DISC_BYTE..])?;
                    let ps_nx_lending_account_dooot = PsNxLendLendingAccountDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        owner: info.owner.to_string(),
                        deposit_notes: info.deposit_notes,
                        deposit_tokens: info.deposit_tokens,
                        last_note_rate: info.last_note_rate,
                        total_reward: info.total_reward,
                    };
                    all_doots.push(Dooot::PsNxLendLendingAccount(ps_nx_lending_account_dooot));
                }
                VSOL_POOL_DISC => {
                    let info = vSolPool::deserialize(&mut &data[DISC_BYTE..])?;
                    let ps_nx_vsol_pool_dooot = PsNxLendVSolPoolDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        vsol_leverage_tokens: info.vsol_leverage_tokens,
                        vsol_leverage_notes: info.vsol_leverage_notes,
                        vsol_points: info.vsol_points,
                        last_vsol_point_per_leverage_note: info.last_vsol_point_per_leverage_note,
                        vsol_points_fee_claimable: info.vsol_points_fee_claimable,
                        last_vsol_point_update_time: info.last_vsol_point_update_time,
                    };
                    all_doots.push(Dooot::PsNxLendVSolPool(ps_nx_vsol_pool_dooot));
                }
                SOLAYER_POOL_DISC => {
                    let info = solayerPool::deserialize(&mut &data[DISC_BYTE..])?;
                    let ps_nx_solayer_pool_dooot = PsNxLendSolayerPoolDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        nx_market: info.nx_market.to_string(),
                        lrt_mint: info.lrt_mint.to_string(),
                        amount: info.amount,
                        total_nx_solayer_points: info.total_nx_solayer_points,
                        last_update_time: info.last_update_time,
                    };
                    all_doots.push(Dooot::PsNxLendSolayerPool(ps_nx_solayer_pool_dooot));
                }
                _ => continue,
            }
        }
        let leverage_iter =
            acct_provider.program_accounts(Pubkey::from_str(NX_LEVERAGE_PROGRAM)?, None);
        for acct in leverage_iter {
            let data = acct.account.data();
            let disc = &data[..DISC_BYTE];

            match disc {
                MARGIN_ACC_DISC => {
                    let info = marginAccount::deserialize(&mut &data[DISC_BYTE..])?;

                    let mut deposits_vec = Vec::new();
                    for deposit in &info.deposits {
                        let collateral_detail = CollateralDetail {
                            token_mint: deposit.token_mint.to_string(),
                            deposit_note: deposit.deposit_note,
                            deposit_token: deposit.deposit_token,
                            market_value: deposit.market_value,
                        };
                        deposits_vec.push(collateral_detail);
                    }

                    let mut loans_vec = Vec::new();
                    for loan in &info.loans {
                        let loan_detail = LoanDetail {
                            token_mint: loan.token_mint.to_string(),
                            loan_note: loan.loan_note,
                            loan_token: loan.loan_token,
                        };
                        loans_vec.push(loan_detail);
                    }
                    let swap_temp_detail = SwapTemp {
                        before_amount: info.swap_temp.before_amount,
                        expected: info.swap_temp.expected,
                    };

                    let ps_nx_margin_account_dooot = PsNxLeverageMarginAccountDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        leveragefi: info.leveragefi.to_string(),
                        owner: info.owner.to_string(),
                        deposits: deposits_vec,
                        loans: loans_vec,
                        leverage: info.leverage,
                        jlp_notes: info.jlp_notes,
                        active_loan: info.active_loan.to_string(),
                        active_collateral: info.active_collateral.to_string(),
                        swap_temp: swap_temp_detail,
                        account_type: info.account_type,
                        liquidation_flag: info.liquidation_flag,
                    };
                    all_doots.push(Dooot::PsNxLeverageMarginAccount(ps_nx_margin_account_dooot));
                }
                MARGIN_POOL_DISC => {
                    let info = marginPool::deserialize(&mut &data[DISC_BYTE..])?;
                    let ps_nx_margin_pool_dooot = PsNxLeverageMarginPoolDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        leveragefi: info.leveragefi.to_string(),
                        vault: info.vault.to_string(),
                        fee_destination: info.fee_destination.to_string(),
                        pool_authority: info.pool_authority.to_string(),
                        token_mint: info.token_mint.to_string(),
                        token_price_oracle: info.token_price_oracle.to_string(),
                        borrowed_tokens: info.borrowed_tokens,
                        deposit_tokens: info.deposit_tokens,
                        deposit_notes: info.deposit_notes,
                        loan_notes: info.loan_notes,
                        deposit_interest: info.deposit_interest,
                        loan_interest: info.loan_interest,
                        protocol_fee: info.protocol_fee,
                        accrued_until: info.accrued_until,
                        utilization_flag: info.utilization_flag,
                    };
                    all_doots.push(Dooot::PsNxLeverageMarginPool(ps_nx_margin_pool_dooot));
                }
                JUPITER_ACC_DISC => {
                    // "The jupiter account to receive the leveraged loan tokens" -global
                    let info = jupiterAccount::deserialize(&mut &data[DISC_BYTE..])?;
                    let ps_nx_jupiter_account_dooot = PsNxLeverageJupiterAccountDooot {
                        time,
                        account_pubkey: acct.pubkey.to_string(),
                        leveragefi: info.leveragefi.to_string(),
                        admin: info.admin.to_string(),
                        jlp_mint: info.jlp_mint.to_string(),
                        jlp_account: info.jlp_account.to_string(),
                        jupiter_notes: info.jupiter_notes,
                        jlp_position: info.jlp_position,
                    };
                    all_doots.push(Dooot::PsNxLeverageJupiterAccount(
                        ps_nx_jupiter_account_dooot,
                    ));
                }
                _ => continue,
            }
        }
        for doot in all_doots {
            tx.send(doot)?;
        }
        Ok(())
    }
    fn get_name(&self) -> &'static str {
        "Nx"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use step_ingestooor_sdk::{
        decoded_transaction::DecodedTransaction, utils::test_utils::init_test_extras_bare,
    };

    #[tokio::test]
    async fn test_deposit_sol() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_f = fs::read_to_string("tests/nx/frag.json").unwrap();
        let decoded = serde_json::from_str(&txn_f).unwrap();
        let decoded = DecodedTransaction::from_rabbit_message_json(decoded).unwrap();

        let mut dooot = vec![];
        let parser = Nx;

        parser.invoke(&decoded, &mut dooot, &utils).await.unwrap();
        assert_eq!(dooot.len(), 4);
    }
    #[tokio::test]
    async fn leverage_ssol() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_f = fs::read_to_string("tests/nx/leverage_ssol.json").unwrap();
        let decoded = serde_json::from_str(&txn_f).unwrap();
        let decoded = DecodedTransaction::from_rabbit_message_json(decoded).unwrap();

        let mut dooot = vec![];
        let parser = Nx;

        parser.invoke(&decoded, &mut dooot, &utils).await.unwrap();
        assert_eq!(dooot.len(), 4);
    }
    #[tokio::test]
    async fn lend_deposit_sol() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_f = fs::read_to_string("tests/nx/lend_deposit_sol.json").unwrap();
        let decoded = serde_json::from_str(&txn_f).unwrap();
        let decoded = DecodedTransaction::from_rabbit_message_json(decoded).unwrap();

        let mut dooot = vec![];
        let parser = Nx;

        parser.invoke(&decoded, &mut dooot, &utils).await.unwrap();
        assert_eq!(dooot.len(), 2);
    }
    #[tokio::test]
    async fn test_leverage_add() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_f = fs::read_to_string("tests/nx/leverage_add.json").unwrap();
        let decoded = serde_json::from_str(&txn_f).unwrap();
        let decoded = DecodedTransaction::from_rabbit_message_json(decoded).unwrap();

        let mut dooot = vec![];
        let parser = Nx;

        parser.invoke(&decoded, &mut dooot, &utils).await.unwrap();
        assert_eq!(dooot.len(), 4);
    }
    #[tokio::test]
    async fn test_leverage_withdraw1() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_f = fs::read_to_string("tests/nx/leverage_withdraw1.json").unwrap();
        let decoded = serde_json::from_str(&txn_f).unwrap();
        let decoded = DecodedTransaction::from_rabbit_message_json(decoded).unwrap();

        let mut dooot = vec![];
        let parser = Nx;

        parser.invoke(&decoded, &mut dooot, &utils).await.unwrap();
        assert_eq!(dooot.len(), 3);
    }
    #[tokio::test]
    async fn test_leverage_withdraw2() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_f = fs::read_to_string("tests/nx/leverage_withdraw2.json").unwrap();
        let decoded = serde_json::from_str(&txn_f).unwrap();
        let decoded = DecodedTransaction::from_rabbit_message_json(decoded).unwrap();

        let mut dooot = vec![];
        let parser = Nx;

        parser.invoke(&decoded, &mut dooot, &utils).await.unwrap();
        assert_eq!(dooot.len(), 4);
    }
    #[tokio::test]
    async fn test_withdraw_lend() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_f = fs::read_to_string("tests/nx/withdraw_lend.json").unwrap();
        let decoded = serde_json::from_str(&txn_f).unwrap();
        let decoded = DecodedTransaction::from_rabbit_message_json(decoded).unwrap();

        let mut dooot = vec![];
        let parser = Nx;

        parser.invoke(&decoded, &mut dooot, &utils).await.unwrap();
        assert_eq!(dooot.len(), 3);
    }
    #[tokio::test]
    async fn test_bune() {
        let (utils, _) = init_test_extras_bare().await;
        let txn_f = fs::read_to_string("tests/nx/bune.json").unwrap();
        let decoded = serde_json::from_str(&txn_f).unwrap();
        let decoded = DecodedTransaction::from_rabbit_message_json(decoded).unwrap();

        let mut dooot = vec![];
        let parser = Nx;

        parser.invoke(&decoded, &mut dooot, &utils).await.unwrap();
        assert_eq!(dooot.len(), 2);
    }
}
