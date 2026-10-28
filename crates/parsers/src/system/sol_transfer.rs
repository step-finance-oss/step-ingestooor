use anyhow::Result;

use async_trait::async_trait;

use solana_transaction_status::{UiInstruction, UiParsedInstruction};
use step_ingestooor_sdk::{
    dooot::{Dooot, SolTransferEventDooot},
    trait_defs::transaction::TransactionLike,
    utils::{
        constants::SYSTEM_PROGRAM_ID, step_utils::StepUtils,
        sys_ixs::SystemProgramParsedInstruction, util_fns::get_program_id_of_ix,
    },
};

use crate::{TransactionParserFilter, TransactionParserTrait};

pub struct SolTransfer;

#[async_trait]
impl TransactionParserTrait for SolTransfer {
    fn get_name(&self) -> &'static str {
        "SOL transfer"
    }
    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            SYSTEM_PROGRAM_ID.to_string(),
        )]
    }
    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        for ix in decoded_transaction.get_flattened_ixs() {
            if let UiInstruction::Parsed(UiParsedInstruction::Parsed(parsed)) = ix.instruction {
                if parsed.program_id != SYSTEM_PROGRAM_ID {
                    continue;
                }
                let des_ix = serde_json::from_value(parsed.parsed.clone())?;

                if let SystemProgramParsedInstruction::SystemTransfer(sol_transfer_ix) = des_ix {
                    let mut parent_program: Option<String> = None;
                    // Check if we're even iterating through the inners
                    if parsed.stack_height.is_some() {
                        let parent_ix = decoded_transaction
                            .get_outer_instructions()
                            .get(ix.index as usize);
                        if let Some(p_ix) = parent_ix {
                            parent_program = get_program_id_of_ix(p_ix);
                        }
                    }

                    let sol_dooot = Dooot::SolTransferEvent(SolTransferEventDooot {
                        amount: sol_transfer_ix.lamports.try_into()?,
                        destination_pubkey: sol_transfer_ix.destination,
                        source_pubkey: sol_transfer_ix.source,
                        parent_program,
                        time: decoded_transaction.get_block_time(),
                    });

                    dooot.push(sol_dooot);
                }
            }
        }
        Ok(())
    }
}
