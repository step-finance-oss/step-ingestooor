use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use rust_decimal::Decimal;
use solana_transaction_status::{UiInstruction, UiParsedInstruction};
use step_ingestooor_sdk::utils::constants::{STEP_MINT, XSTEP_MINT, XSTEP_PROGRAM_ID};
use step_ingestooor_sdk::utils::spl_ixs::{
    get_parsed_token_program_instruction, TokenParsedInstruction,
};
use step_ingestooor_sdk::{
    dooot::{Dooot, SwapEventDooot},
    trait_defs::transaction::TransactionLike,
};

pub struct StepStakingSwap;

#[async_trait]
impl TransactionParserTrait for StepStakingSwap {
    fn get_name(&self) -> &'static str {
        "Staked Step"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            XSTEP_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _utils: &step_ingestooor_sdk::utils::step_utils::StepUtils,
    ) -> Result<()> {
        let tree = decoded_transaction.get_ix_tree()?;
        //find all xstep program ixs and get the children and parent
        //the children show us amounts via transfers
        let ixs = tree
            .root()
            .context("UNREACHABLE - Tree should have a root")?
            .traverse_level_order()
            .skip(1)
            .filter_map(|a| {
                let node = a.data().as_ref()?;
                let UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(ix)) = node else {
                    return None;
                };
                if ix.program_id != XSTEP_PROGRAM_ID {
                    return None;
                }

                //grab the parent program id
                let parent_program = a.parent().and_then(|p| match p.data() {
                    Some(UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(ix))) => {
                        Some(ix.program_id.clone())
                    }
                    Some(UiInstruction::Parsed(UiParsedInstruction::Parsed(ix))) => {
                        Some(ix.program_id.clone())
                    }
                    _ => None,
                });

                Some((a.children().collect::<Vec<_>>(), parent_program))
            });

        for (kids, parent_program) in ixs {
            if kids.len() != 2 {
                return Err(anyhow!("Expected 2 children, found {}", kids.len()));
            }
            //get the transferred amount of step
            let xfer = get_parsed_token_program_instruction(
                kids[1]
                    .data()
                    .context("UNREACHABLE - Inner node data should be Some(..)")?,
            )?;
            let Some(TokenParsedInstruction::TokenTransfer(xfer)) = xfer else {
                return Err(anyhow!("Expected transfer instruction"));
            };

            let in_mint: String;
            let in_amount: Decimal;
            let out_mint: String;
            let out_amount: Decimal;
            let auth: String;

            //get the mint or burn amount
            let mint_or_burn = get_parsed_token_program_instruction(
                kids[0]
                    .data()
                    .context("UNREACHABLE - Inner node data should be Some(..)")?,
            )?;
            match mint_or_burn {
                Some(TokenParsedInstruction::TokenMintTo(mint)) => {
                    //if we minted xstep, the step amount is a send (negative)
                    out_amount = Decimal::from(mint.amount);
                    out_mint = XSTEP_MINT.to_string();
                    in_amount = Decimal::from(xfer.amount);
                    in_mint = STEP_MINT.to_string();
                    auth = xfer.authority.context("Transfer authority not found")?;
                }
                Some(TokenParsedInstruction::TokenBurn(burn)) => {
                    out_amount = Decimal::from(xfer.amount);
                    out_mint = STEP_MINT.to_string();
                    in_amount = Decimal::from(burn.amount);
                    in_mint = XSTEP_MINT.to_string();
                    auth = burn.authority.context("Burn authority not found")?;
                }
                _ => {
                    return Err(anyhow!("Expected mint or burn instruction"));
                }
            };

            dooot.push(Dooot::SwapEvent(SwapEventDooot {
                in_amount,
                in_mint_pubkey: in_mint,
                out_amount,
                out_mint_pubkey: out_mint,
                program_pubkey: XSTEP_PROGRAM_ID.to_string(),
                swap_auth_pubkey: auth,
                time: decoded_transaction.get_block_time(),
                fee_mint_is_in_mint: None,
                platform_referral_fee: None,
                referring_platform_account_pubkey: None,
                parent_amm_program_pubkey: parent_program,
                market_pubkey: Some(XSTEP_PROGRAM_ID.to_string()),
                fees: vec![],
                slippage_bps: None,
                tx_sig: decoded_transaction.get_signature().to_string(),
            }));
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rust_decimal::Decimal;

    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    use step_ingestooor_sdk::utils::constants::STEP_MINT;

    #[tokio::test]
    async fn parser_invoke_a() {
        let decoded_txn =
            DecodedTransaction::load_rabbit_message_from_file("tests/xstep/full_stake_ix.json")
                .unwrap();

        let mut dooot = vec![];
        let parser = StepStakingSwap;
        parser
            .invoke(&decoded_txn, &mut dooot, &Default::default())
            .await
            .unwrap();

        assert_eq!(dooot.len(), 1);

        let Dooot::SwapEvent(route_ix) = &dooot[0] else {
            panic!("Expected a swap dooot");
        };

        let actual_in_mint = &route_ix.in_mint_pubkey;
        let actual_out_mint = &route_ix.out_mint_pubkey;
        let actual_staker = &route_ix.swap_auth_pubkey;
        let actual_in_amount = route_ix.in_amount;
        let actual_out_amount = route_ix.out_amount;

        let expected_in_mint = STEP_MINT;
        let expected_out_mint = XSTEP_MINT;
        let expected_staker = "qUeL7JzC52V1DvvPkqnMd74QjThWtSJY5G1PkKv1ur7";
        let expected_out_amount = Decimal::from(638253374);
        let expected_in_amount = Decimal::from(1000000000);

        assert_eq!(expected_in_mint, actual_in_mint);
        assert_eq!(expected_out_mint, actual_out_mint);
        assert_eq!(expected_staker, actual_staker);
        assert_eq!(expected_in_amount, actual_in_amount);
        assert_eq!(expected_out_amount, actual_out_amount);
    }
}
