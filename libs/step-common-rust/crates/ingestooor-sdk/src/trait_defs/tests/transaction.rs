#![cfg(test)]

//! What do we need to test?:
//!
//! - get_flattened_ixs is properly ordered
//! - get_token_account_data_by_key locates a token account
//! - get_grouped_instructions properly groups
//! - get_total_transfer_amounts_for_pubkey gets proper ins and outs
//! - get_missing_ata_info finds missing info
//!
//! What should the transaction be, that does all of this?
//!
//! - One info-complete swap as outer, calling inner txfers
//! - Another swap (inner) with an intermediate OUT token account, that gets sent to a different location after the swap
use base64::{prelude::BASE64_STANDARD, Engine};
use chrono::NaiveDateTime;
use serde::Serialize;
use serde_json::json;
use solana_account_decoder::parse_token::UiTokenAmount;
use solana_transaction_status::{
    option_serializer::OptionSerializer, parse_accounts::ParsedAccount,
    parse_instruction::ParsedInstruction, UiInnerInstructions, UiInstruction, UiParsedInstruction,
    UiPartiallyDecodedInstruction, UiTransactionTokenBalance,
};
use std::vec;

use crate::{
    trait_defs::transaction::TransactionLike,
    utils::{
        constants::TOKEN_PROGRAM_ID,
        spl_ixs::{TokenInitializeAccountIx, TokenParsedInstruction},
        util_fns::{get_parsed_variant, get_partially_decoded_variant},
    },
};

// These don't have to be valid `Pubkey`s for these tests
const SWAP_PROGRAM_ID: &str = "swap_program";
const SWAP_IX_DATA: &str = "swap_data";

const ALICE_PUBKEY: &str = "alice";
const ALICE_ATA_A_PUBKEY: &str = "alice_ata_a";
const ALICE_ATA_B_PUBKEY: &str = "alice_ata_b";

const SWAP_POOL_PUBKEY: &str = "swap_pool";
const SWAP_POOL_ATA_A_PUBKEY: &str = "swap_pool_ata_A";
const SWAP_POOL_ATA_B_PUBKEY: &str = "swap_pool_ata_B";

const BOB_PUBKEY: &str = "bob";
// This will be the temp token account created by the swap
const BOB_ATA_A_PUBKEY: &str = "bob_ata_a";
const BOB_ATA_B_PUBKEY: &str = "bob_ata_b";

const TOKEN_A_MINT: &str = "token_A";
const TOKEN_B_MINT: &str = "token_B";

#[derive(Serialize, Debug)]
struct UtilFuncsTestTransaction {
    account_keys: Vec<ParsedAccount>,
    pre_token_balances: OptionSerializer<Vec<UiTransactionTokenBalance>>,
    post_token_balances: OptionSerializer<Vec<UiTransactionTokenBalance>>,
    outers: Vec<UiInstruction>,
    inners: OptionSerializer<Vec<UiInnerInstructions>>,
    pre_balances: Vec<u64>,
    post_balances: Vec<u64>,
}

impl UtilFuncsTestTransaction {
    fn new() -> Self {
        // Build the first, info complete swap, which is between Alice and the swap pool
        let complete_swap_outer = UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(
            UiPartiallyDecodedInstruction {
                accounts: vec![],
                data: SWAP_IX_DATA.into(),
                program_id: SWAP_PROGRAM_ID.into(),
                stack_height: None,
            },
        ));
        let complete_swap_inner = UiInnerInstructions {
            index: 0,
            instructions: vec![
                UiInstruction::Parsed(UiParsedInstruction::Parsed(ParsedInstruction {
                    program_id: TOKEN_PROGRAM_ID.into(),
                    program: TOKEN_PROGRAM_ID.into(),
                    stack_height: Some(2),
                    parsed: json!({
                        "info": {
                            "authority": ALICE_PUBKEY,
                            "destination": SWAP_POOL_ATA_A_PUBKEY,
                            "source": ALICE_ATA_A_PUBKEY,
                            "amount": "100",
                        },
                        "type": "transfer",
                    }),
                })),
                UiInstruction::Parsed(UiParsedInstruction::Parsed(ParsedInstruction {
                    program_id: TOKEN_PROGRAM_ID.into(),
                    program: TOKEN_PROGRAM_ID.into(),
                    stack_height: Some(2),
                    parsed: json!({
                        "info": {
                            "authority": SWAP_POOL_PUBKEY,
                            "destination": ALICE_ATA_B_PUBKEY,
                            "source": SWAP_POOL_ATA_B_PUBKEY,
                            "amount": "20",
                        },
                        "type": "transfer",
                    }),
                })),
            ],
        };

        // Build the second, incomplete swap, which is between Bob and the swap pool
        let incomplete_dummy_outer = UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(
            UiPartiallyDecodedInstruction {
                accounts: vec![],
                data: String::from("dummy_outer_swap_data"),
                program_id: SWAP_PROGRAM_ID.into(),
                stack_height: None,
            },
        ));
        let incomplete_swap_inner = UiInnerInstructions {
            index: 1,
            instructions: vec![
                // Temp creation
                UiInstruction::Parsed(UiParsedInstruction::Parsed(ParsedInstruction {
                    parsed: json!({
                        "info": TokenInitializeAccountIx {
                            account: BOB_ATA_A_PUBKEY.into(),
                            mint: TOKEN_A_MINT.into(),
                            owner: BOB_PUBKEY.into(),
                        },
                        "type": "initializeAccount3".to_string(),
                    }),
                    program_id: TOKEN_PROGRAM_ID.into(),
                    program: TOKEN_PROGRAM_ID.into(),
                    stack_height: Some(2),
                })),
                // Swap parent inner
                UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(
                    UiPartiallyDecodedInstruction {
                        accounts: vec![],
                        data: SWAP_IX_DATA.into(),
                        program_id: SWAP_PROGRAM_ID.into(),
                        stack_height: Some(2),
                    },
                )),
                // Token transfers within inner swap
                UiInstruction::Parsed(UiParsedInstruction::Parsed(ParsedInstruction {
                    program_id: TOKEN_PROGRAM_ID.into(),
                    program: TOKEN_PROGRAM_ID.into(),
                    stack_height: Some(3),
                    parsed: json!({
                        "info": {
                            "authority": BOB_PUBKEY,
                            "destination": SWAP_POOL_ATA_A_PUBKEY,
                            "source": BOB_ATA_A_PUBKEY,
                            "amount": "420",
                        },
                        "type": "transfer",
                    }),
                })),
                UiInstruction::Parsed(UiParsedInstruction::Parsed(ParsedInstruction {
                    program_id: TOKEN_PROGRAM_ID.into(),
                    program: TOKEN_PROGRAM_ID.into(),
                    stack_height: Some(3),
                    parsed: json!({
                        "info": {
                            "authority": SWAP_POOL_PUBKEY,
                            "destination": BOB_ATA_B_PUBKEY,
                            "source": SWAP_POOL_ATA_B_PUBKEY,
                            "amount": "69",
                        },
                        "type": "transfer",
                    }),
                })),
            ],
        };

        Self {
            account_keys: vec![
                // Alice's ATA at index 0,
                // Bob's ATA at index 1
                ParsedAccount {
                    pubkey: ALICE_ATA_A_PUBKEY.into(),
                    signer: true,
                    writable: true,
                    source: None,
                },
                ParsedAccount {
                    pubkey: BOB_ATA_A_PUBKEY.into(),
                    signer: true,
                    writable: true,
                    source: None,
                },
                ParsedAccount {
                    pubkey: ALICE_ATA_B_PUBKEY.into(),
                    signer: true,
                    writable: true,
                    source: None,
                },
                ParsedAccount {
                    pubkey: BOB_ATA_B_PUBKEY.into(),
                    signer: true,
                    writable: true,
                    source: None,
                },
                ParsedAccount {
                    pubkey: SWAP_POOL_ATA_A_PUBKEY.into(),
                    signer: true,
                    writable: true,
                    source: None,
                },
                ParsedAccount {
                    pubkey: SWAP_POOL_ATA_B_PUBKEY.into(),
                    signer: true,
                    writable: true,
                    source: None,
                },
            ],
            outers: vec![complete_swap_outer, incomplete_dummy_outer],
            inners: OptionSerializer::Some(vec![complete_swap_inner, incomplete_swap_inner]),
            pre_token_balances: OptionSerializer::Some(vec![]), // No need for pre balances
            post_token_balances: OptionSerializer::Some(vec![
                UiTransactionTokenBalance {
                    account_index: 0,
                    mint: TOKEN_A_MINT.into(),
                    owner: OptionSerializer::Some(ALICE_PUBKEY.into()),
                    program_id: OptionSerializer::Some(TOKEN_PROGRAM_ID.into()),
                    ui_token_amount: UiTokenAmount {
                        amount: "100".into(),
                        decimals: 0,
                        ui_amount: Some(100.0),
                        ui_amount_string: "100".into(),
                    },
                },
                UiTransactionTokenBalance {
                    account_index: 1,
                    mint: TOKEN_A_MINT.into(),
                    owner: OptionSerializer::Some(BOB_PUBKEY.into()),
                    program_id: OptionSerializer::Some(TOKEN_PROGRAM_ID.into()),
                    ui_token_amount: UiTokenAmount {
                        amount: "420".into(),
                        decimals: 0,
                        ui_amount: Some(420.0),
                        ui_amount_string: "420".into(),
                    },
                },
                UiTransactionTokenBalance {
                    account_index: 2,
                    mint: TOKEN_B_MINT.into(),
                    owner: OptionSerializer::Some(ALICE_PUBKEY.into()),
                    program_id: OptionSerializer::Some(TOKEN_PROGRAM_ID.into()),
                    ui_token_amount: UiTokenAmount {
                        amount: "100".into(),
                        decimals: 0,
                        ui_amount: Some(100.0),
                        ui_amount_string: "100".into(),
                    },
                },
                UiTransactionTokenBalance {
                    account_index: 3,
                    mint: TOKEN_B_MINT.into(),
                    owner: OptionSerializer::Some(BOB_PUBKEY.into()),
                    program_id: OptionSerializer::Some(TOKEN_PROGRAM_ID.into()),
                    ui_token_amount: UiTokenAmount {
                        amount: "69".into(),
                        decimals: 0,
                        ui_amount: Some(69.0),
                        ui_amount_string: "69".into(),
                    },
                },
                UiTransactionTokenBalance {
                    account_index: 4,
                    mint: TOKEN_A_MINT.into(),
                    owner: OptionSerializer::Some(SWAP_POOL_PUBKEY.into()),
                    program_id: OptionSerializer::Some(TOKEN_PROGRAM_ID.into()),
                    ui_token_amount: UiTokenAmount {
                        amount: "20".into(),
                        decimals: 0,
                        ui_amount: Some(20.0),
                        ui_amount_string: "20".into(),
                    },
                },
                UiTransactionTokenBalance {
                    account_index: 5,
                    mint: TOKEN_B_MINT.into(),
                    owner: OptionSerializer::Some(SWAP_POOL_PUBKEY.into()),
                    program_id: OptionSerializer::Some(TOKEN_PROGRAM_ID.into()),
                    ui_token_amount: UiTokenAmount {
                        amount: "69".into(),
                        decimals: 0,
                        ui_amount: Some(69.0),
                        ui_amount_string: "69".into(),
                    },
                },
            ]),
            post_balances: vec![100, 420, 100, 69, 20, 69],
            pre_balances: vec![],
        }
    }
}

impl TransactionLike for UtilFuncsTestTransaction {
    fn get_block_time(&self) -> NaiveDateTime {
        unimplemented!()
    }

    fn get_slot(&self) -> u64 {
        unimplemented!()
    }

    fn get_signature(&self) -> &str {
        "test_sig"
    }

    fn get_outer_instructions(&self) -> &Vec<UiInstruction> {
        &self.outers
    }

    fn get_inner_instructions(&self) -> &OptionSerializer<Vec<UiInnerInstructions>> {
        &self.inners
    }

    fn get_account_keys(&self) -> &Vec<ParsedAccount> {
        &self.account_keys
    }

    fn get_pre_token_balances(&self) -> &OptionSerializer<Vec<UiTransactionTokenBalance>> {
        &self.pre_token_balances
    }

    fn get_post_token_balances(&self) -> &OptionSerializer<Vec<UiTransactionTokenBalance>> {
        &self.post_token_balances
    }

    fn get_pre_balances(&self) -> &Vec<u64> {
        unimplemented!()
    }

    fn get_post_balances(&self) -> &Vec<u64> {
        unimplemented!()
    }

    fn get_pre_datums(&self) -> &Option<Vec<Option<String>>> {
        unimplemented!()
    }

    fn get_post_datums(&self) -> &Option<Vec<Option<String>>> {
        unimplemented!()
    }

    fn get_pre_owners(&self) -> &Option<Vec<Option<String>>> {
        unimplemented!()
    }

    fn get_post_owners(&self) -> &Option<Vec<Option<String>>> {
        unimplemented!()
    }

    fn get_logs(&self) -> &OptionSerializer<Vec<String>> {
        unimplemented!()
    }
}

/// Check if the flattened ixs are ordered properly
/// - First, the outer instructions
/// - Then, the inner instructions
#[test]
fn get_flattened_ixs_ordering_and_items() {
    let txn = UtilFuncsTestTransaction::new();
    let ixs = txn.get_flattened_ixs().collect::<Vec<_>>();

    assert_eq!(ixs.len(), 8);
    assert!(!ixs[0].is_inner);
    assert_eq!(ixs[0].index, 0);
    assert!(!ixs[1].is_inner);
    assert_eq!(ixs[1].index, 1);
    let all_next_are_inner = ixs[2..].iter().all(|ix| ix.is_inner);
    assert!(all_next_are_inner);

    let next_2_are_swap_0 = ixs[2..4].iter().all(|ix| ix.index == 0);
    assert!(next_2_are_swap_0);

    let next_4_are_swap_1 = ixs[4..8].iter().all(|ix| ix.index == 1);
    assert!(next_4_are_swap_1);
}

#[test]
fn get_token_account_data_by_key_can_locate_in_balances() {
    let txn = UtilFuncsTestTransaction::new();
    let acct_info = txn
        .get_token_account_data_by_key(ALICE_ATA_A_PUBKEY)
        .unwrap();

    assert_eq!(acct_info.account_index, 0);
    assert_eq!(acct_info.mint, TOKEN_A_MINT);
    assert_eq!(acct_info.owner, Some(ALICE_PUBKEY.into()));
}

#[test]
fn get_token_account_data_by_key_looks_for_creations() {
    let txn = UtilFuncsTestTransaction::new();
    let acct_info = txn.get_token_account_data_by_key(BOB_ATA_A_PUBKEY).unwrap();

    assert_eq!(acct_info.account_index, 1);
    assert_eq!(acct_info.mint, TOKEN_A_MINT);
    assert_eq!(acct_info.owner, Some(BOB_PUBKEY.into()));
}

#[test]
fn group_and_isolate_swaps() {
    let txn = UtilFuncsTestTransaction::new();
    let grouped = txn
        .get_grouped_instructions(|ix| {
            let Some(partial) = get_partially_decoded_variant(ix.instruction) else {
                return false;
            };

            partial.data == SWAP_IX_DATA
        })
        .expect("Bad testing txn");

    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped[0].involved_instructions.len(), 2);
    assert_eq!(grouped[1].involved_instructions.len(), 2); // Still only 2 ixs for the inner swap

    let all_parent_ixs_have_matching_data = grouped.iter().all(|group| {
        let parent_data =
            get_partially_decoded_variant(group.parent_instruction.instruction).unwrap();
        parent_data.data == SWAP_IX_DATA
    });

    assert!(all_parent_ixs_have_matching_data);

    let all_complete_swap_inners_are_transfers =
        grouped[0].involved_instructions.iter().all(|ix| {
            let parsed = get_parsed_variant(ix.instruction).unwrap();

            matches!(
                serde_json::from_value::<TokenParsedInstruction>(parsed.parsed.clone()),
                Ok(TokenParsedInstruction::TokenTransfer(_))
            )
        });

    assert!(all_complete_swap_inners_are_transfers);

    let all_incomplete_swap_inners_are_transfers =
        grouped[1].involved_instructions.iter().all(|ix| {
            let parsed = get_parsed_variant(ix.instruction).unwrap();

            matches!(
                serde_json::from_value::<TokenParsedInstruction>(parsed.parsed.clone()),
                Ok(TokenParsedInstruction::TokenTransfer(_))
            )
        });

    assert!(all_incomplete_swap_inners_are_transfers);
}

#[test]
fn amount_by_pubkey_match() {
    let txn = UtilFuncsTestTransaction::new();
    let ixs = txn.get_flattened_ixs().collect::<Vec<_>>();

    let alice_txfer_amounts = txn
        .get_total_transfer_amounts_for_pubkey_flattened_ixs(
            ixs.iter(),
            &[ALICE_PUBKEY],
            &[TOKEN_A_MINT],
            &[TOKEN_B_MINT],
        )
        .unwrap();

    let bob_txfer_amounts = txn
        .get_total_transfer_amounts_for_pubkey_flattened_ixs(
            ixs.iter(),
            &[BOB_PUBKEY],
            &[TOKEN_A_MINT],
            &[TOKEN_B_MINT],
        )
        .unwrap();

    let alice_in = alice_txfer_amounts.get_in_amount_u64(TOKEN_A_MINT, ALICE_PUBKEY);
    let alice_out = alice_txfer_amounts.get_out_amount_u64(TOKEN_B_MINT, ALICE_PUBKEY);

    let bob_in = bob_txfer_amounts.get_in_amount_u64(TOKEN_A_MINT, BOB_PUBKEY);
    let bob_out = bob_txfer_amounts.get_out_amount_u64(TOKEN_B_MINT, BOB_PUBKEY);

    assert_eq!(alice_in, 100);
    assert_eq!(alice_out, 20);
    assert_eq!(bob_in, 420);
    assert_eq!(bob_out, 69);
}

#[test]
fn check_program_event_parsing() -> anyhow::Result<()> {
    let logs: Vec<String> = vec![
          "Program ComputeBudget111111111111111111111111111111 invoke [1]".into(),
          "Program ComputeBudget111111111111111111111111111111 success".into(),
          "Program ComputeBudget111111111111111111111111111111 invoke [1]".into(),
          "Program ComputeBudget111111111111111111111111111111 success".into(),
          "Program Stk5NCWomVN3itaFjLu382u9ibb5jMSHEsh6CuhaGjB invoke [1]".into(),
          "Program log: Instruction: Stake".into(),
          "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]".into(),
          "Program log: Instruction: MintTo".into(),
          "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA consumed 4500 of 166221 compute units".into(),
          "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success".into(),
          "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]".into(),
          "Program log: Instruction: Transfer".into(),
          "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA consumed 4645 of 158848 compute units".into(),
          "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success".into(),
          "Program data: mFK8y42ZJnWvpYxMAAAAABIAAAAxLjI4NDI4NTg3MTMxMjk2NDavpYxMAAAAABIAAAAxLjI4NDI4NTg3MTMxMjk2NDg=".into(),
          "Program Stk5NCWomVN3itaFjLu382u9ibb5jMSHEsh6CuhaGjB consumed 52030 of 199700 compute units".into(),
          "Program Stk5NCWomVN3itaFjLu382u9ibb5jMSHEsh6CuhaGjB success".into()
        ];

    #[derive(Serialize, Debug)]
    struct TestTxn {
        logs: OptionSerializer<Vec<String>>,
    }

    impl TransactionLike for TestTxn {
        fn get_block_time(&self) -> NaiveDateTime {
            todo!()
        }

        fn get_slot(&self) -> u64 {
            todo!()
        }

        fn get_signature(&self) -> &str {
            todo!()
        }

        fn get_outer_instructions(&self) -> &Vec<UiInstruction> {
            todo!()
        }

        fn get_inner_instructions(&self) -> &OptionSerializer<Vec<UiInnerInstructions>> {
            todo!()
        }

        fn get_account_keys(&self) -> &Vec<ParsedAccount> {
            todo!()
        }

        fn get_pre_token_balances(&self) -> &OptionSerializer<Vec<UiTransactionTokenBalance>> {
            todo!()
        }

        fn get_post_token_balances(&self) -> &OptionSerializer<Vec<UiTransactionTokenBalance>> {
            todo!()
        }

        fn get_pre_datums(&self) -> &Option<Vec<Option<String>>> {
            todo!()
        }

        fn get_post_datums(&self) -> &Option<Vec<Option<String>>> {
            todo!()
        }

        fn get_pre_owners(&self) -> &Option<Vec<Option<String>>> {
            todo!()
        }

        fn get_post_owners(&self) -> &Option<Vec<Option<String>>> {
            todo!()
        }

        fn get_pre_balances(&self) -> &Vec<u64> {
            todo!()
        }

        fn get_post_balances(&self) -> &Vec<u64> {
            todo!()
        }

        fn get_logs(&self) -> &OptionSerializer<Vec<String>> {
            &self.logs
        }
    }

    let program_id: &str = "Stk5NCWomVN3itaFjLu382u9ibb5jMSHEsh6CuhaGjB";

    let event_bytes = BASE64_STANDARD.decode("mFK8y42ZJnWvpYxMAAAAABIAAAAxLjI4NDI4NTg3MTMxMjk2NDavpYxMAAAAABIAAAAxLjI4NDI4NTg3MTMxMjk2NDg=")?;

    let txn1 = TestTxn {
        logs: OptionSerializer::Some(logs.clone()),
    };
    let events = txn1.get_program_events_from_logs(program_id);
    assert_eq!(events.len(), 1, "Should only be one event");
    assert_eq!(events[0], event_bytes, "Event bytes should match");

    // token program should not emit any events
    let no_events =
        txn1.get_program_events_from_logs("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
    assert_eq!(
        no_events.len(),
        0,
        "Token program should not emit any events"
    );

    Ok(())
}
