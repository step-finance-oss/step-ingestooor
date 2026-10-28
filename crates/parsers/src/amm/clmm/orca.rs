use super::utils::mul_rsh;

use crate::state_source::{AccountProvider, StateSource};
use crate::{TransactionParserFilter, TransactionParserTrait};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use borsh::BorshDeserialize;
use log::error;
use primitive_types::U256;
use rust_decimal::Decimal;
use solana_sdk::account::ReadableAccount;
use solana_sdk::pubkey::Pubkey;
use solana_transaction_status::{UiInstruction, UiParsedInstruction};
use std::array;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::mpsc::SyncSender;
use step_ingestooor_idl_gens::orca_whirlies_gen::orca_whirlies::accounts::TickArray;
use step_ingestooor_idl_gens::orca_whirlies_gen::orca_whirlies::{
    accounts::{Position, Whirlpool},
    utils::InstructionUnion,
};
use step_ingestooor_sdk::dooot::{
    ClmmGlobalDooot, ClmmTick, ClmmTickGlobalDooot, ClmmUserDooot, CurveType, Dooot, LPInfoDooot,
    LPInfoUnderlyingMintVault, LiquidityChangeEventDooot, SwapEventDooot,
};
use step_ingestooor_sdk::trait_defs::transaction::TransactionLike;
use step_ingestooor_sdk::utils::spl_ixs::{
    get_parsed_token_program_instruction, HasAuthority, TokenParsedInstruction,
};
use step_ingestooor_sdk::utils::step_account_reader::StepAccountReader;
use step_ingestooor_sdk::utils::step_utils::StepUtils;
use step_ingestooor_sdk::utils::util_fns::{
    convert_blocktime_to_micros, get_parsed_variant, get_partially_decoded_variant,
};

pub struct DynamicTickData {
    pub liquidity_net: i128,   // 16
    pub liquidity_gross: u128, // 16

    // Q64.64
    pub fee_growth_outside_a: u128, // 16
    // Q64.64
    pub fee_growth_outside_b: u128, // 16

    // Array of Q64.64
    pub reward_growths_outside: [u128; 3], // 48 = 16 * 3
}

#[derive(Default)]
pub enum DynamicTick {
    #[default]
    Uninitialized,
    Initialized(DynamicTickData),
}

impl From<DynamicTick> for ClmmTick {
    fn from(value: DynamicTick) -> Self {
        match value {
            DynamicTick::Uninitialized => ClmmTick {
                fee_growth_outside_a: 0.to_string(),
                fee_growth_outside_b: 0.to_string(),
                liquidity_net: 0.to_string(),
                liquidity_gross: 0.to_string(),
            },
            DynamicTick::Initialized(data) => ClmmTick {
                fee_growth_outside_a: data.fee_growth_outside_a.to_string(),
                fee_growth_outside_b: data.fee_growth_outside_b.to_string(),
                liquidity_net: data.liquidity_net.to_string(),
                liquidity_gross: data.liquidity_gross.to_string(),
            },
        }
    }
}

pub struct DynamicTickArray {
    pub start_index: i32,
    pub whirlpool: Pubkey,
    pub ticks: [DynamicTick; 88],
}

impl DynamicTickArray {
    pub fn deserialize(data: &[u8]) -> Result<Self> {
        let mut reader = StepAccountReader::new(data);
        let start_index = reader.read_next_le_i32()?;
        let whirlpool = *reader.read_next_pubkey()?;
        reader.advance_unclamped(16); // skip tick_bitmap, a u128
        let mut ticks = array::from_fn(|_| DynamicTick::default());

        for tick in &mut ticks {
            let Some(tag_byte) = reader.read_next_single_byte().ok() else {
                // We went out of bounds. The rest of the account is uninitialized.
                break;
            };

            match tag_byte {
                0 => {}
                1 => {
                    let liquidity_net = reader.read_next_le_i128()?;
                    let liquidity_gross = reader.read_next_le_u128()?;
                    let fee_growth_outside_a = reader.read_next_le_u128()?;
                    let fee_growth_outside_b = reader.read_next_le_u128()?;
                    let reward_growths_outside = [
                        reader.read_next_le_u128()?,
                        reader.read_next_le_u128()?,
                        reader.read_next_le_u128()?,
                    ];

                    *tick = DynamicTick::Initialized(DynamicTickData {
                        liquidity_net,
                        liquidity_gross,
                        fee_growth_outside_a,
                        fee_growth_outside_b,
                        reward_growths_outside,
                    });
                }
                _ => {
                    bail!("Invalid tick enum byte read from tick account");
                }
            }
        }

        Ok(DynamicTickArray {
            start_index,
            whirlpool,
            ticks,
        })
    }
}

pub struct OrcaWhirlies;

pub const ORCA_WHIRLPOOL_PROGRAM: &str = "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc";
pub const POSITION_DISC: &[u8] = &[170, 188, 143, 228, 122, 64, 247, 208];
pub const WHIRLPOOL_DISC: &[u8] = &[63, 149, 209, 12, 225, 128, 99, 9];
pub const TICK_ARRAY_DISC: &[u8] = &[0x45, 0x61, 0xbd, 0xbe, 0x6e, 0x07, 0x42, 0xbb];
pub const DYNAMIC_TICK_ARRAY_DISC: &[u8] = &[17, 216, 246, 142, 225, 199, 218, 56];

impl OrcaWhirlies {
    #[inline]
    pub fn get_token_amounts_from_position(pool: &Whirlpool, position: &Position) -> (u64, u64) {
        let lower_sqrt_price_x64 = Self::tick_index_to_sqrt_price_x64(position.tick_lower_index);
        let upper_sqrt_price_x64 = Self::tick_index_to_sqrt_price_x64(position.tick_upper_index);

        let current_sqrt_price_x64 = pool.sqrt_price;
        let total_liquidity = position.liquidity;

        Self::get_token_amounts_from_liquidity(
            current_sqrt_price_x64,
            lower_sqrt_price_x64,
            upper_sqrt_price_x64,
            total_liquidity,
        )
    }

    #[inline]
    fn get_token_amounts_from_liquidity(
        current_sqrt_price_x64: u128,
        lower_sqrt_price: u128,
        upper_sqrt_price: u128,
        liquidity: u128,
    ) -> (u64, u64) {
        if current_sqrt_price_x64 < lower_sqrt_price {
            let token_a = Self::get_token_amount_a_from_liquidity(
                lower_sqrt_price,
                upper_sqrt_price,
                liquidity,
            );

            (token_a, 0)
        } else if current_sqrt_price_x64 < upper_sqrt_price {
            let token_a = Self::get_token_amount_a_from_liquidity(
                current_sqrt_price_x64,
                upper_sqrt_price,
                liquidity,
            );
            let token_b = Self::get_token_amount_b_from_liquidity(
                lower_sqrt_price,
                current_sqrt_price_x64,
                liquidity,
            );

            (token_a, token_b)
        } else {
            let token_b = Self::get_token_amount_b_from_liquidity(
                lower_sqrt_price,
                upper_sqrt_price,
                liquidity,
            );

            (0, token_b)
        }
    }

    #[inline]
    fn get_token_amount_a_from_liquidity(
        mut sqrt_price_a: u128,
        mut sqrt_price_b: u128,
        liquidity: u128,
    ) -> u64 {
        if sqrt_price_a > sqrt_price_b {
            std::mem::swap(&mut sqrt_price_a, &mut sqrt_price_b)
        }

        let shifted_liq = U256::from(liquidity) << 64;
        let diff = U256::from(sqrt_price_b - sqrt_price_a);

        (((shifted_liq * diff) / sqrt_price_b) / sqrt_price_a).as_u64()
    }

    #[inline]
    fn get_token_amount_b_from_liquidity(
        mut sqrt_price_a: u128,
        mut sqrt_price_b: u128,
        liquidity: u128,
    ) -> u64 {
        if sqrt_price_a > sqrt_price_b {
            std::mem::swap(&mut sqrt_price_a, &mut sqrt_price_b)
        }

        // Shift by 64 to do a faster divide by 2^64
        ((liquidity * (sqrt_price_b - sqrt_price_a)) >> 64) as u64
    }

    /// See https://github.com/orca-so/whirlpools/blob/main/programs/whirlpool/src/math/tick_math.rs#L21
    #[inline]
    fn tick_index_to_sqrt_price_x64(tick: i32) -> u128 {
        if tick > 0 {
            Self::tick_index_to_sqrt_price_positive(tick)
        } else {
            Self::tick_index_to_sqrt_price_negative(tick)
        }
    }

    #[inline]
    fn tick_index_to_sqrt_price_positive(tick: i32) -> u128 {
        let mut ratio: U256 = if tick & 1 != 0 {
            U256::from(79232123823359799118286999567u128)
        } else {
            U256::from(79228162514264337593543950336u128)
        };

        if tick & 2 != 0 {
            ratio = mul_rsh(ratio, 79236085330515764027303304731u128, 96);
        }
        if tick & 4 != 0 {
            ratio = mul_rsh(ratio, 79244008939048815603706035061u128, 96);
        }
        if tick & 8 != 0 {
            ratio = mul_rsh(ratio, 79259858533276714757314932305u128, 96);
        }
        if tick & 16 != 0 {
            ratio = mul_rsh(ratio, 79291567232598584799939703904u128, 96);
        }
        if tick & 32 != 0 {
            ratio = mul_rsh(ratio, 79355022692464371645785046466u128, 96);
        }
        if tick & 64 != 0 {
            ratio = mul_rsh(ratio, 79482085999252804386437311141u128, 96);
        }
        if tick & 128 != 0 {
            ratio = mul_rsh(ratio, 79736823300114093921829183326u128, 96);
        }
        if tick & 256 != 0 {
            ratio = mul_rsh(ratio, 80248749790819932309965073892u128, 96);
        }
        if tick & 512 != 0 {
            ratio = mul_rsh(ratio, 81282483887344747381513967011u128, 96);
        }
        if tick & 1024 != 0 {
            ratio = mul_rsh(ratio, 83390072131320151908154831281u128, 96);
        }
        if tick & 2048 != 0 {
            ratio = mul_rsh(ratio, 87770609709833776024991924138u128, 96);
        }
        if tick & 4096 != 0 {
            ratio = mul_rsh(ratio, 97234110755111693312479820773u128, 96);
        }
        if tick & 8192 != 0 {
            ratio = mul_rsh(ratio, 119332217159966728226237229890u128, 96);
        }
        if tick & 16384 != 0 {
            ratio = mul_rsh(ratio, 179736315981702064433883588727u128, 96);
        }
        if tick & 32768 != 0 {
            ratio = mul_rsh(ratio, 407748233172238350107850275304u128, 96);
        }
        if tick & 65536 != 0 {
            ratio = mul_rsh(ratio, 2098478828474011932436660412517u128, 96);
        }
        if tick & 131072 != 0 {
            ratio = mul_rsh(ratio, 55581415166113811149459800483533u128, 96);
        }
        if tick & 262144 != 0 {
            ratio = mul_rsh(ratio, 38992368544603139932233054999993551u128, 96);
        }

        (ratio >> 32).as_u128()
    }

    #[inline]
    fn tick_index_to_sqrt_price_negative(tick: i32) -> u128 {
        let abs_tick = tick.abs();

        let mut ratio: u128 = if abs_tick & 1 != 0 {
            18445821805675392311
        } else {
            18446744073709551616
        };

        if abs_tick & 2 != 0 {
            ratio = (ratio * 18444899583751176498) >> 64
        }
        if abs_tick & 4 != 0 {
            ratio = (ratio * 18443055278223354162) >> 64
        }
        if abs_tick & 8 != 0 {
            ratio = (ratio * 18439367220385604838) >> 64
        }
        if abs_tick & 16 != 0 {
            ratio = (ratio * 18431993317065449817) >> 64
        }
        if abs_tick & 32 != 0 {
            ratio = (ratio * 18417254355718160513) >> 64
        }
        if abs_tick & 64 != 0 {
            ratio = (ratio * 18387811781193591352) >> 64
        }
        if abs_tick & 128 != 0 {
            ratio = (ratio * 18329067761203520168) >> 64
        }
        if abs_tick & 256 != 0 {
            ratio = (ratio * 18212142134806087854) >> 64
        }
        if abs_tick & 512 != 0 {
            ratio = (ratio * 17980523815641551639) >> 64
        }
        if abs_tick & 1024 != 0 {
            ratio = (ratio * 17526086738831147013) >> 64
        }
        if abs_tick & 2048 != 0 {
            ratio = (ratio * 16651378430235024244) >> 64
        }
        if abs_tick & 4096 != 0 {
            ratio = (ratio * 15030750278693429944) >> 64
        }
        if abs_tick & 8192 != 0 {
            ratio = (ratio * 12247334978882834399) >> 64
        }
        if abs_tick & 16384 != 0 {
            ratio = (ratio * 8131365268884726200) >> 64
        }
        if abs_tick & 32768 != 0 {
            ratio = (ratio * 3584323654723342297) >> 64
        }
        if abs_tick & 65536 != 0 {
            ratio = (ratio * 696457651847595233) >> 64
        }
        if abs_tick & 131072 != 0 {
            ratio = (ratio * 26294789957452057) >> 64
        }
        if abs_tick & 262144 != 0 {
            ratio = (ratio * 37481735321082) >> 64
        }

        ratio
    }
}

#[async_trait]
impl TransactionParserTrait for OrcaWhirlies {
    fn get_name(&self) -> &'static str {
        "Orca Whirlpool Program"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            ORCA_WHIRLPOOL_PROGRAM.to_string(),
        )]
    }
    async fn invoke(
        &self,
        decoded_transaction: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> anyhow::Result<()> {
        let time = decoded_transaction.get_block_time();
        //get all orca swap ixs
        let grouped_instructions =
            decoded_transaction.get_grouped_instructions(|ix| match ix.instruction {
                UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(ix)) => {
                    if ix.program_id != ORCA_WHIRLPOOL_PROGRAM {
                        return false;
                    }
                    let bytes = bs58::decode(&ix.data).into_vec();
                    if let Ok(bytes) = bytes {
                        if let Ok(possible_ix) = InstructionUnion::try_from_slice(&bytes) {
                            return matches!(
                                possible_ix,
                                InstructionUnion::Swap(_)
                                    | InstructionUnion::SwapV2(_)
                                    | InstructionUnion::TwoHopSwap(_)
                                    | InstructionUnion::TwoHopSwapV2(_)
                                    | InstructionUnion::IncreaseLiquidity(_)
                                    | InstructionUnion::IncreaseLiquidityV2(_)
                                    | InstructionUnion::InitializePool(_)
                                    | InstructionUnion::InitializePoolV2(_)
                                    | InstructionUnion::OpenPosition(_)
                                    | InstructionUnion::OpenPositionWithMetadata(_)
                                    | InstructionUnion::DecreaseLiquidity(_)
                                    | InstructionUnion::DecreaseLiquidityV2(_)
                            );
                        }
                    }
                    false
                }
                _ => false,
            })?;

        //loop each orca swap tx
        for grouped_ix in &grouped_instructions {
            let inner = &grouped_ix.involved_instructions;
            //get the swap instruction for this group
            let swap_decoded_ix =
                get_partially_decoded_variant(grouped_ix.parent_instruction.instruction)
                    .ok_or_else(|| {
                        error!(
                            "Failed to decode instruction variant for transaction: {}",
                            decoded_transaction.get_signature()
                        );
                        anyhow!("Failed to decode instruction variant")
                    })?;
            //parse the data for the ix to get the orca enum variant
            let swap_instruction_data = &swap_decoded_ix.data;
            let swap_instruction_data = bs58::decode(swap_instruction_data).into_vec()?;
            let parsed_swap_ix = InstructionUnion::try_from_slice(&swap_instruction_data)?;

            //grab the parent of the swap (if any) to save in the swap ixs
            let parent_amm = grouped_ix
                .parent_instruction
                .get_ix_parent_program_id(decoded_transaction)
                .cloned();

            match parsed_swap_ix {
                InstructionUnion::Swap(_) => {
                    //a standalone swap is simple, we output one dooot for it and that's it
                    let auth = &swap_decoded_ix.accounts[1];
                    let swapper = auth.clone();

                    //there are always 2 child ixs for a swap. they're the transfers in and out of the pool
                    let tx_in = get_parsed_variant(inner[0].instruction)
                        .ok_or(anyhow!("Error on getting inner instructions"))?;
                    let TokenParsedInstruction::TokenTransfer(t_in) =
                        serde_json::from_value(tx_in.parsed.clone())?
                    else {
                        return Err(anyhow!("Failed to parse in instruction"));
                    };

                    let tx_out = get_parsed_variant(inner[1].instruction)
                        .ok_or(anyhow!("Error on getting inner instructions"))?;
                    let TokenParsedInstruction::TokenTransfer(t_out) =
                        serde_json::from_value(tx_out.parsed.clone())?
                    else {
                        return Err(anyhow!("Failed to parse out instruction"));
                    };

                    //grab the amounts transferred
                    let in_amount = t_in.amount;
                    let out_amount = t_out.amount;

                    //the transfer ix doesnt have the mint, so we grab it for each
                    let in_mint = decoded_transaction
                        .get_token_account_data_by_key(&t_in.source)?
                        .mint;
                    let out_mint = decoded_transaction
                        .get_token_account_data_by_key(&t_out.destination)?
                        .mint;

                    //the orca market pubkey is the authority of the pool token accounts, so we grab from there
                    let market_pubkey = t_out.get_authority()?.clone();

                    dooot.push(Dooot::SwapEvent(SwapEventDooot {
                        in_mint_pubkey: in_mint,
                        out_mint_pubkey: out_mint,
                        in_amount: Decimal::from(in_amount),
                        out_amount: Decimal::from(out_amount),
                        swap_auth_pubkey: swapper,
                        time: decoded_transaction.get_block_time(),
                        market_pubkey: Some(market_pubkey),
                        parent_amm_program_pubkey: parent_amm,
                        program_pubkey: ORCA_WHIRLPOOL_PROGRAM.to_string(),
                        fee_mint_is_in_mint: None,
                        platform_referral_fee: None,
                        referring_platform_account_pubkey: None,
                        fees: vec![],
                        slippage_bps: None,
                        tx_sig: decoded_transaction.get_signature().to_string(),
                    }));
                }
                InstructionUnion::SwapV2(_) => {
                    //a standalone swap is simple, we output one dooot for it and that's it
                    let auth = &swap_decoded_ix.accounts[3];
                    let swapper = auth.clone();

                    //inner should have 2 transfers
                    let inner_transfers = inner
                        .iter()
                        .filter_map(|a| {
                            get_parsed_token_program_instruction(a.instruction)
                                .ok()
                                .flatten()
                        })
                        .collect::<Vec<_>>();

                    let mut in_mint: String = String::new();
                    let mut out_mint: String = String::new();
                    let mut in_amount: Decimal = Decimal::from(0);
                    let mut out_amount: Decimal = Decimal::from(0);
                    let mut market_pubkey: String = String::new();

                    //there are always 2 child ixs for a swap. they're the transfers in and out of the pool
                    if let TokenParsedInstruction::TokenTransferChecked(t_in) = &inner_transfers[0]
                    {
                        in_amount = t_in.token_amount.amount.parse()?;
                        // //the transfer ix doesnt have the mint, so we grab it for each
                        in_mint = decoded_transaction
                            .get_token_account_data_by_key(&t_in.source)?
                            .mint;
                    }

                    if let TokenParsedInstruction::TokenTransferChecked(t_out) = &inner_transfers[1]
                    {
                        out_amount = t_out.token_amount.amount.parse()?;

                        // //the transfer ix doesnt have the mint, so we grab it for each
                        out_mint = decoded_transaction
                            .get_token_account_data_by_key(&t_out.destination)?
                            .mint;

                        // //the orca market pubkey is the authority of the pool token accounts, so we grab from there
                        market_pubkey.clone_from(t_out.get_authority()?)
                    }

                    let swap_sec = Dooot::SwapEvent(SwapEventDooot {
                        in_mint_pubkey: in_mint,
                        out_mint_pubkey: out_mint,
                        in_amount,
                        out_amount,
                        swap_auth_pubkey: swapper,
                        time: decoded_transaction.get_block_time(),
                        market_pubkey: Some(market_pubkey),
                        parent_amm_program_pubkey: parent_amm,
                        program_pubkey: ORCA_WHIRLPOOL_PROGRAM.to_string(),
                        fee_mint_is_in_mint: None,
                        platform_referral_fee: None,
                        referring_platform_account_pubkey: None,
                        fees: vec![],
                        slippage_bps: None,
                        tx_sig: decoded_transaction.get_signature().to_string(),
                    });
                    dooot.push(swap_sec.clone());
                }
                InstructionUnion::TwoHopSwap(_) => {
                    //a two hop swap has 4 inner instructions
                    //these are the same as above but one swap flows into the next
                    //so we output each individually, then a routed swap
                    //representing the full swap
                    let auth = &swap_decoded_ix.accounts[1];
                    let swapper = auth.clone();

                    //fill these out as we go
                    let mut routed_in_mint: Option<String> = None;
                    let mut routed_in_amount: Option<u64> = None;
                    let mut routed_out_mint: Option<String> = None;
                    let mut routed_out_amount: Option<u64> = None;

                    //do the 2 inner swaps, represented by 2 pairs of 2 ixs (indexes 0-3 inclusive)
                    for i in (0..4).step_by(2) {
                        let tx_in = get_parsed_variant(inner[i].instruction)
                            .ok_or(anyhow!("Error on getting inner instructions"))?;
                        let TokenParsedInstruction::TokenTransfer(t_in) =
                            serde_json::from_value(tx_in.parsed.clone())?
                        else {
                            return Err(anyhow!("Failed to parse in instruction"));
                        };

                        let tx_out = get_parsed_variant(inner[i + 1].instruction)
                            .ok_or(anyhow!("Error on getting inner instructions"))?;
                        let TokenParsedInstruction::TokenTransfer(t_out) =
                            serde_json::from_value(tx_out.parsed.clone())?
                        else {
                            return Err(anyhow!("Failed to parse out instruction"));
                        };

                        //amounts
                        let in_amount = t_in.amount;
                        let out_amount = t_out.amount;

                        //the transfer ix doesnt have the mint, so we grab it for each
                        let in_mint = decoded_transaction
                            .get_token_account_data_by_key(&t_in.source)?
                            .mint;
                        let out_mint = decoded_transaction
                            .get_token_account_data_by_key(&t_out.destination)?
                            .mint;
                        //the orca market pubkey is the authority of the pool token accounts, so we grab from there
                        let market_pubkey = t_out.get_authority()?;

                        dooot.push(Dooot::SwapEvent(SwapEventDooot {
                            in_mint_pubkey: in_mint.clone(),
                            out_mint_pubkey: out_mint.clone(),
                            in_amount: Decimal::from(in_amount),
                            out_amount: Decimal::from(out_amount),
                            swap_auth_pubkey: swapper.clone(),
                            time: decoded_transaction.get_block_time(),
                            market_pubkey: Some(market_pubkey.clone()),
                            //we're inside a routed swap, so oddly, the parent amm is the same as the program
                            parent_amm_program_pubkey: Some(ORCA_WHIRLPOOL_PROGRAM.to_string()),
                            program_pubkey: ORCA_WHIRLPOOL_PROGRAM.to_string(),
                            fee_mint_is_in_mint: None,
                            platform_referral_fee: None,
                            referring_platform_account_pubkey: None,
                            fees: vec![],
                            slippage_bps: None,
                            tx_sig: decoded_transaction.get_signature().to_string(),
                        }));

                        //hold onto things we need for our routed swap
                        if i == 0 {
                            routed_in_mint.replace(in_mint);
                            routed_in_amount.replace(in_amount);
                        } else {
                            routed_out_mint.replace(out_mint);
                            routed_out_amount.replace(out_amount);
                        }
                    }

                    //now do the routed swap
                    dooot.push(Dooot::SwapEvent(SwapEventDooot {
                        in_mint_pubkey: routed_in_mint.context("missing routed in mint")?,
                        out_mint_pubkey: routed_out_mint.context("missing routed out mint")?,
                        in_amount: Decimal::from(
                            routed_in_amount.context("missing routed in amount")?,
                        ),
                        out_amount: Decimal::from(
                            routed_out_amount.context("missing routed out amount")?,
                        ),
                        swap_auth_pubkey: swapper,
                        time: decoded_transaction.get_block_time(),
                        //a routed swap isnt over an individual market, by nature
                        market_pubkey: None,
                        parent_amm_program_pubkey: parent_amm,
                        program_pubkey: ORCA_WHIRLPOOL_PROGRAM.to_string(),
                        fee_mint_is_in_mint: None,
                        platform_referral_fee: None,
                        referring_platform_account_pubkey: None,
                        fees: vec![],
                        slippage_bps: None,
                        tx_sig: decoded_transaction.get_signature().to_string(),
                    }));
                }
                InstructionUnion::TwoHopSwapV2(_) => {
                    //a two hop swap has 3 inner instructions
                    //these are the same as above but one swap flows into the next
                    //so we output each individually, then a routed swap
                    //representing the full swap
                    let auth = &swap_decoded_ix.accounts[14];
                    let swapper = auth.clone();
                    let token_input = &swap_decoded_ix.accounts[2];
                    let mut amount_input: Decimal = Decimal::ZERO;
                    let token_intermediate = &swap_decoded_ix.accounts[3];
                    let mut amount_intermediate: Decimal = Decimal::ZERO;
                    let token_output = &swap_decoded_ix.accounts[4];
                    let mut amount_output: Decimal = Decimal::ZERO;
                    let mut market_pubkey_hop_swap: String = String::new();

                    //inner should have 3 transfers
                    let inner_transfers = inner
                        .iter()
                        .filter_map(|a| {
                            get_parsed_token_program_instruction(a.instruction)
                                .ok()
                                .flatten()
                        })
                        .collect::<Vec<_>>();

                    if let TokenParsedInstruction::TokenTransferChecked(t_in) = &inner_transfers[0]
                    {
                        amount_input = Decimal::from_str(&t_in.token_amount.amount.to_string())?;
                    }

                    if let TokenParsedInstruction::TokenTransferChecked(t_intermediate) =
                        &inner_transfers[1]
                    {
                        amount_intermediate =
                            Decimal::from_str(&t_intermediate.token_amount.amount.to_string())?;
                        market_pubkey_hop_swap.clone_from(t_intermediate.get_authority()?)
                    }
                    //first
                    let dooot_sec = Dooot::SwapEvent(SwapEventDooot {
                        in_mint_pubkey: token_input.to_string(),
                        out_mint_pubkey: token_intermediate.to_string(),
                        in_amount: amount_input,
                        out_amount: amount_intermediate,
                        swap_auth_pubkey: swapper.clone(),
                        time: decoded_transaction.get_block_time(),
                        market_pubkey: Some(market_pubkey_hop_swap.clone()),
                        parent_amm_program_pubkey: Some(ORCA_WHIRLPOOL_PROGRAM.to_string()),
                        program_pubkey: ORCA_WHIRLPOOL_PROGRAM.to_string(),
                        fee_mint_is_in_mint: None,
                        platform_referral_fee: None,
                        referring_platform_account_pubkey: None,
                        fees: vec![],
                        slippage_bps: None,
                        tx_sig: decoded_transaction.get_signature().to_string(),
                    });
                    dooot.push(dooot_sec);

                    if let TokenParsedInstruction::TokenTransferChecked(t_out) = &inner_transfers[2]
                    {
                        amount_output = Decimal::from_str(&t_out.token_amount.amount.to_string())?;
                        market_pubkey_hop_swap.clone_from(t_out.get_authority()?)
                    }

                    let dooot_sec = Dooot::SwapEvent(SwapEventDooot {
                        in_mint_pubkey: token_intermediate.to_string(),
                        out_mint_pubkey: token_output.to_string(),
                        in_amount: amount_intermediate,
                        out_amount: amount_output,
                        swap_auth_pubkey: swapper.clone(),
                        time: decoded_transaction.get_block_time(),
                        market_pubkey: Some(market_pubkey_hop_swap),
                        parent_amm_program_pubkey: parent_amm.clone(),
                        program_pubkey: ORCA_WHIRLPOOL_PROGRAM.to_string(),
                        fee_mint_is_in_mint: None,
                        platform_referral_fee: None,
                        referring_platform_account_pubkey: None,
                        fees: vec![],
                        slippage_bps: None,
                        tx_sig: decoded_transaction.get_signature().to_string(),
                    });
                    dooot.push(dooot_sec);

                    //routed
                    let dooot_sec = Dooot::SwapEvent(SwapEventDooot {
                        in_mint_pubkey: token_input.to_string(),
                        out_mint_pubkey: token_output.to_string(),
                        in_amount: amount_input,
                        out_amount: amount_output,
                        swap_auth_pubkey: swapper.clone(),
                        time: decoded_transaction.get_block_time(),
                        //a routed swap isnt over an individual market, by nature
                        market_pubkey: None,
                        parent_amm_program_pubkey: parent_amm.clone(),
                        program_pubkey: ORCA_WHIRLPOOL_PROGRAM.to_string(),
                        fee_mint_is_in_mint: None,
                        platform_referral_fee: None,
                        referring_platform_account_pubkey: None,
                        fees: vec![],
                        slippage_bps: None,
                        tx_sig: decoded_transaction.get_signature().to_string(),
                    });
                    dooot.push(dooot_sec);
                }
                InstructionUnion::IncreaseLiquidity(_) => {
                    let pool_owner = &swap_decoded_ix.accounts[2];
                    let token_owner_acc_a = &swap_decoded_ix.accounts[5];
                    let token_mint_a = &decoded_transaction
                        .get_token_account_data_by_key(token_owner_acc_a)?
                        .mint;
                    let token_owner_acc_b = &swap_decoded_ix.accounts[6];
                    let token_mint_b = &decoded_transaction
                        .get_token_account_data_by_key(token_owner_acc_b)?
                        .mint;
                    let token_vault_a = &swap_decoded_ix.accounts[7];
                    let vault_a_balance_pre = &decoded_transaction
                        .get_pre_token_account_balance(token_vault_a)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let vault_a_balance_post = &decoded_transaction
                        .get_post_token_account_balance(token_vault_a)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let change_a = *vault_a_balance_post as i128 - *vault_a_balance_pre as i128;
                    let token_vault_b = &swap_decoded_ix.accounts[8];
                    let vault_b_balance_pre = &decoded_transaction
                        .get_pre_token_account_balance(token_vault_b)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let vault_b_balance_post = &decoded_transaction
                        .get_post_token_account_balance(token_vault_b)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let change_b = *vault_b_balance_post as i128 - *vault_b_balance_pre as i128;
                    let whirlpool = &swap_decoded_ix.accounts[0];
                    let liq_change = Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                        time: decoded_transaction.get_block_time(),
                        owner: pool_owner.to_string(),
                        pool: whirlpool.to_string(),
                        mints: vec![token_mint_a.to_string(), token_mint_b.to_string()],
                        amounts: vec![Decimal::from(change_a), Decimal::from(change_b)],
                    });
                    dooot.push(liq_change);
                }
                InstructionUnion::IncreaseLiquidityV2(_) => {
                    let pool_owner = &swap_decoded_ix.accounts[4];
                    let token_owner_acc_a = &swap_decoded_ix.accounts[9];
                    let token_mint_a = &decoded_transaction
                        .get_token_account_data_by_key(token_owner_acc_a)?
                        .mint;
                    let token_owner_acc_b = &swap_decoded_ix.accounts[10];
                    let token_mint_b = &decoded_transaction
                        .get_token_account_data_by_key(token_owner_acc_b)?
                        .mint;
                    let token_vault_a = &swap_decoded_ix.accounts[11];
                    let vault_a_balance_pre = &decoded_transaction
                        .get_pre_token_account_balance(token_vault_a)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let vault_a_balance_post = &decoded_transaction
                        .get_post_token_account_balance(token_vault_a)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let change_a = *vault_a_balance_post as i128 - *vault_a_balance_pre as i128;
                    let token_vault_b = &swap_decoded_ix.accounts[12];
                    let vault_b_balance_pre = &decoded_transaction
                        .get_pre_token_account_balance(token_vault_b)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let vault_b_balance_post = &decoded_transaction
                        .get_post_token_account_balance(token_vault_b)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let change_b = *vault_b_balance_post as i128 - *vault_b_balance_pre as i128;
                    let whirlpool = &swap_decoded_ix.accounts[0];
                    let liq_change = Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                        time: decoded_transaction.get_block_time(),
                        owner: pool_owner.to_string(),
                        pool: whirlpool.to_string(),
                        mints: vec![token_mint_a.to_string(), token_mint_b.to_string()],
                        amounts: vec![Decimal::from(change_a), Decimal::from(change_b)],
                    });
                    dooot.push(liq_change);
                }
                InstructionUnion::DecreaseLiquidity(_) => {
                    let pool_owner = &swap_decoded_ix.accounts[2];
                    let token_owner_acc_a = &swap_decoded_ix.accounts[5];
                    let token_mint_a = &decoded_transaction
                        .get_token_account_data_by_key(token_owner_acc_a)?
                        .mint;
                    let token_owner_acc_b = &swap_decoded_ix.accounts[6];
                    let token_mint_b = &decoded_transaction
                        .get_token_account_data_by_key(token_owner_acc_b)?
                        .mint;

                    let token_vault_a = &swap_decoded_ix.accounts[7];
                    let vault_a_balance_pre = &decoded_transaction
                        .get_pre_token_account_balance(token_vault_a)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let vault_a_balance_post = &decoded_transaction
                        .get_post_token_account_balance(token_vault_a)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let change_a = *vault_a_balance_pre as i128 - *vault_a_balance_post as i128;

                    let token_vault_b = &swap_decoded_ix.accounts[8];
                    let vault_b_balance_pre = &decoded_transaction
                        .get_pre_token_account_balance(token_vault_b)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let vault_b_balance_post = &decoded_transaction
                        .get_post_token_account_balance(token_vault_b)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let change_b = *vault_b_balance_pre as i128 - *vault_b_balance_post as i128;
                    let whirlpool = &swap_decoded_ix.accounts[0];

                    let liq_change = Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                        time: decoded_transaction.get_block_time(),
                        owner: pool_owner.to_string(),
                        pool: whirlpool.to_string(),
                        mints: vec![token_mint_a.to_string(), token_mint_b.to_string()],
                        amounts: vec![
                            Decimal::from(change_a) * Decimal::NEGATIVE_ONE,
                            Decimal::from(change_b) * Decimal::NEGATIVE_ONE,
                        ],
                    });
                    dooot.push(liq_change);
                }
                InstructionUnion::DecreaseLiquidityV2(_) => {
                    let pool_owner = &swap_decoded_ix.accounts[4];
                    let token_owner_acc_a = &swap_decoded_ix.accounts[9];
                    let token_mint_a = &decoded_transaction
                        .get_token_account_data_by_key(token_owner_acc_a)?
                        .mint;
                    let token_owner_acc_b = &swap_decoded_ix.accounts[10];
                    let token_mint_b = &decoded_transaction
                        .get_token_account_data_by_key(token_owner_acc_b)?
                        .mint;

                    let token_vault_a = &swap_decoded_ix.accounts[11];
                    let vault_a_balance_pre = &decoded_transaction
                        .get_pre_token_account_balance(token_vault_a)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let vault_a_balance_post = &decoded_transaction
                        .get_post_token_account_balance(token_vault_a)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;

                    let change_a = *vault_a_balance_pre as i128 - *vault_a_balance_post as i128;

                    let token_vault_b = &swap_decoded_ix.accounts[12];
                    let vault_b_balance_pre = &decoded_transaction
                        .get_pre_token_account_balance(token_vault_b)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let vault_b_balance_post = &decoded_transaction
                        .get_post_token_account_balance(token_vault_b)?
                        .ok_or_else(|| {
                            anyhow!(
                                "Missing post token account balance for address: {}",
                                token_vault_a
                            )
                        })?;
                    let change_b = *vault_b_balance_pre as i128 - *vault_b_balance_post as i128;
                    let whirlpool = &swap_decoded_ix.accounts[0];

                    let liq_change = Dooot::LiquidityChangeEvent(LiquidityChangeEventDooot {
                        time: decoded_transaction.get_block_time(),
                        owner: pool_owner.to_string(),
                        pool: whirlpool.to_string(),
                        mints: vec![token_mint_a.to_string(), token_mint_b.to_string()],
                        amounts: vec![
                            Decimal::from(change_a) * Decimal::NEGATIVE_ONE,
                            Decimal::from(change_b) * Decimal::NEGATIVE_ONE,
                        ],
                    });
                    dooot.push(liq_change);
                }
                _ => {
                    continue;
                }
            };
        }

        let acct_iter = decoded_transaction.iter_accounts().filter(|acc| {
            acc.datum_changed
                && (acc.post_owner_program == Some(ORCA_WHIRLPOOL_PROGRAM)
                    || acc.pre_owner_program == Some(ORCA_WHIRLPOOL_PROGRAM))
        });
        for acc in acct_iter {
            let pre_data = acc.unpack_pre_datum()?.map(|d| (d.is_empty(), d));
            let post_data = acc.unpack_post_datum()?.map(|d| (d.is_empty(), d));

            let (closed, data) = match (pre_data, post_data) {
                (_, Some((false, post))) => (false, post),
                (Some((false, pre)), None | Some((true, _))) => (true, pre),
                _ => continue,
            };

            let disc = &data[..8];
            match disc {
                WHIRLPOOL_DISC => {
                    let whirlpool = Whirlpool::try_from_slice(&data[8..])?;
                    let global_clmm_state = Dooot::ClmmGlobal(ClmmGlobalDooot {
                        time: decoded_transaction.get_block_time(),
                        pool_pubkey: acc.pubkey.to_string(),
                        total_liquidity_shares: Some(whirlpool.liquidity.to_string()),
                        current_price: whirlpool.sqrt_price.to_string(),
                        current_tick_index: whirlpool.tick_current_index,
                        tick_spacing: whirlpool.tick_spacing as u32,
                    });

                    dooot.push(global_clmm_state);

                    if acc.was_created {
                        let lp_info = Dooot::LPInfo(LPInfoDooot {
                            time,
                            pool: acc.pubkey.to_string(),
                            program: ORCA_WHIRLPOOL_PROGRAM.to_string(),
                            lp_mint: None,
                            curve_type: CurveType::ConcentratedLiquidity,
                            underlyings: vec![
                                LPInfoUnderlyingMintVault {
                                    mint: whirlpool.token_mint_a.to_string(),
                                    vault: whirlpool.token_vault_a.to_string(),
                                },
                                LPInfoUnderlyingMintVault {
                                    mint: whirlpool.token_mint_b.to_string(),
                                    vault: whirlpool.token_vault_b.to_string(),
                                },
                            ],
                        });

                        dooot.push(lp_info);
                    }
                }
                POSITION_DISC => {
                    let position = Position::try_from_slice(&data[8..])?;

                    let upper_price_sqrt =
                        Self::tick_index_to_sqrt_price_x64(position.tick_upper_index);
                    let lower_price_sqrt =
                        Self::tick_index_to_sqrt_price_x64(position.tick_lower_index);

                    let position_mint = position.position_mint.to_string();

                    if let Some(nft_owner) =
                        decoded_transaction.get_position_nft_owner(&position_mint)
                    {
                        let user_dooot = Dooot::ClmmUser(ClmmUserDooot {
                            time: decoded_transaction.get_block_time(),
                            pool_pubkey: position.whirlpool.to_string(),
                            user_pubkey: nft_owner.to_string(),
                            position_mint: Some(position_mint),
                            upper_price: upper_price_sqrt.to_string(),
                            lower_price: lower_price_sqrt.to_string(),
                            liquidity_shares: if closed {
                                0.to_string()
                            } else {
                                position.liquidity.to_string()
                            },
                            closed,
                        });
                        dooot.push(user_dooot);
                    }
                }
                TICK_ARRAY_DISC => {
                    let tick_array = TickArray::deserialize(&mut &data[8..])?;
                    let tick_dooot = Dooot::ClmmTickGlobal(ClmmTickGlobalDooot {
                        time: decoded_transaction.get_block_time(),
                        pool_pubkey: tick_array.whirlpool.to_string(),
                        tick_account_pubkey: acc.pubkey.to_string(),
                        tick_index: tick_array.start_tick_index,
                        ticks: tick_array
                            .ticks
                            .into_iter()
                            .map(|tick| tick.into())
                            .collect(),
                    });

                    dooot.push(tick_dooot);
                }
                DYNAMIC_TICK_ARRAY_DISC => {
                    let dyn_tick_array = DynamicTickArray::deserialize(&data[8..])?;
                    let tick_dooot = Dooot::ClmmTickGlobal(ClmmTickGlobalDooot {
                        time: decoded_transaction.get_block_time(),
                        pool_pubkey: dyn_tick_array.whirlpool.to_string(),
                        tick_account_pubkey: acc.pubkey.to_string(),
                        tick_index: dyn_tick_array.start_index,
                        ticks: dyn_tick_array
                            .ticks
                            .into_iter()
                            .map(|tick| tick.into())
                            .collect(),
                    });

                    dooot.push(tick_dooot);
                }
                _ => {
                    continue;
                }
            }
        }
        Ok(())
    }
}
impl StateSource for OrcaWhirlies {
    fn get_state(
        &self,
        _config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()> {
        let time = convert_blocktime_to_micros(acct_provider.get_block_time(), Some(0), None)
            .map_err(|e| anyhow!("Failed to convert blocktime: {}", e))?;

        let mut positions: HashMap<String, Position> = HashMap::new();
        for acct in acct_provider.program_accounts(ORCA_WHIRLPOOL_PROGRAM.try_into()?, None) {
            let data = acct.account.data();
            let disc = &data[..8];

            match disc {
                WHIRLPOOL_DISC => {
                    if let Ok(pool_data) = Whirlpool::deserialize(&mut &data[8..]) {
                        let dooot = Dooot::LPInfo(LPInfoDooot {
                            time,
                            pool: acct.pubkey.to_string(),
                            program: ORCA_WHIRLPOOL_PROGRAM.to_string(),
                            lp_mint: None,
                            underlyings: vec![
                                LPInfoUnderlyingMintVault {
                                    mint: pool_data.token_mint_a.to_string(),
                                    vault: pool_data.token_vault_a.to_string(),
                                },
                                LPInfoUnderlyingMintVault {
                                    mint: pool_data.token_mint_b.to_string(),
                                    vault: pool_data.token_vault_b.to_string(),
                                },
                            ],
                            curve_type: CurveType::ConcentratedLiquidity,
                        });
                        let _ = tx.send(dooot);
                        let global_clmm_state = Dooot::ClmmGlobal(ClmmGlobalDooot {
                            time,
                            pool_pubkey: acct.pubkey.to_string(),
                            total_liquidity_shares: Some(pool_data.liquidity.to_string()),
                            current_price: pool_data.sqrt_price.to_string(),
                            current_tick_index: pool_data.tick_current_index,
                            tick_spacing: pool_data.tick_spacing as u32,
                        });
                        let _ = tx.send(global_clmm_state);
                    }
                }
                POSITION_DISC => {
                    if let Ok(position) = Position::deserialize(&mut &data[8..]) {
                        positions.insert(position.position_mint.to_string(), position);
                    }
                }
                TICK_ARRAY_DISC => {
                    let Ok(tick_array) = TickArray::deserialize(&mut &data[8..]) else {
                        continue;
                    };

                    let tick_dooot = Dooot::ClmmTickGlobal(ClmmTickGlobalDooot {
                        time,
                        pool_pubkey: tick_array.whirlpool.to_string(),
                        tick_account_pubkey: acct.pubkey.to_string(),
                        tick_index: tick_array.start_tick_index,
                        ticks: tick_array
                            .ticks
                            .into_iter()
                            .map(|tick| tick.into())
                            .collect(),
                    });

                    tx.send(tick_dooot)?;
                }
                _ => {
                    continue;
                }
            }
        }

        for acct in acct_provider.all_accounts() {
            let data = acct.account.data();
            let data_len = acct.account.data().len();
            if data_len == 165 {
                let mint = Pubkey::try_from(&data[0..32])?.to_string();
                if let Some(position) = positions.get(&mint) {
                    let owner = Pubkey::try_from(&data[32..64])?.to_string();
                    let upper_price_sqrt =
                        Self::tick_index_to_sqrt_price_x64(position.tick_upper_index);
                    let lower_price_sqrt =
                        Self::tick_index_to_sqrt_price_x64(position.tick_lower_index);
                    let user_clmm_state = Dooot::ClmmUser(ClmmUserDooot {
                        time,
                        pool_pubkey: position.whirlpool.to_string(),
                        user_pubkey: owner.to_string(),
                        position_mint: Some(position.position_mint.to_string()),
                        upper_price: upper_price_sqrt.to_string(),
                        lower_price: lower_price_sqrt.to_string(),
                        liquidity_shares: position.liquidity.to_string(),
                        closed: false,
                    });
                    tx.send(user_clmm_state)?;
                }
            }
        }
        Ok(())
    }

    fn get_name(&self) -> &'static str {
        "OrcaWhirlpool"
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, rc::Rc};

    use super::*;
    use rust_decimal::Decimal;
    use serde_json::Value;
    use step_ingestooor_sdk::decoded_transaction::DecodedTransaction;
    use step_ingestooor_sdk::utils::test_utils::init_test_extras_bare;

    #[tokio::test]
    async fn new_tick_arrays() {
        let (utils, _) = init_test_extras_bare().await;
        let txn =
            DecodedTransaction::load_rabbit_message_from_file("./tests/orca/new_tick_arrays.json")
                .unwrap();
        let mut dooot = vec![];
        let parser = OrcaWhirlies;
        parser.invoke(&txn, &mut dooot, &utils).await.unwrap();
    }

    #[tokio::test]
    async fn one_standalone_swap_and_two_2_hop_swaps() {
        let message_json = fs::read_to_string("./tests/orca/sol_orca_swap_tx.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = OrcaWhirlies;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        //7 swaps are output
        //this tx has a single standalone swap, and 2 2-hop swaps
        assert_eq!(dooot.len(), 7);

        //standalone swap
        let Dooot::SwapEvent(route_ix) = &dooot[0] else {
            panic!("Expected a swap dooot");
        };

        let expected_swapper = "HNarxovTeqhyjdVgn6jvDkkFRtbmGGPo5VoP4n9u31Ba";
        let expected_in_mint = "So11111111111111111111111111111111111111112";
        let expected_out_mint = "orcaEKTdK7LKz57vaAYr9QeNsVEPfiu6QeMU1kektZE";
        let expected_in_amount = Decimal::from(3303105);
        let expected_out_amount = Decimal::from(300000);
        let expected_parent_amm: Option<String> = None;
        let expected_market_pubkey: Option<String> = Some(
            "Hxw77h9fEx598afiiZunwHaX3vYu9UskDk9EpPNZp1mG"
                .parse()
                .unwrap(),
        );

        let actual_in_amount = route_ix.in_amount;
        let actual_out_amount = route_ix.out_amount;
        let actual_in_mint = &route_ix.in_mint_pubkey;
        let actual_out_mint = &route_ix.out_mint_pubkey;
        let actual_swapper = &route_ix.swap_auth_pubkey;
        let actual_parent_amm = &route_ix.parent_amm_program_pubkey;
        let actual_market_pubkey = &route_ix.market_pubkey;

        assert_eq!(actual_swapper, expected_swapper);
        assert_eq!(actual_in_mint, expected_in_mint);
        assert_eq!(actual_out_mint, expected_out_mint);
        assert_eq!(actual_in_amount, expected_in_amount);
        assert_eq!(actual_out_amount, expected_out_amount);
        assert_eq!(*actual_parent_amm, expected_parent_amm);
        assert_eq!(*actual_market_pubkey, expected_market_pubkey);

        //first leg of first 2 hop swap
        let Dooot::SwapEvent(route_ix) = &dooot[1] else {
            panic!("Expected a swap dooot");
        };

        let expected_swapper = "HNarxovTeqhyjdVgn6jvDkkFRtbmGGPo5VoP4n9u31Ba";
        let expected_in_mint = "So11111111111111111111111111111111111111112";
        let expected_out_mint = "octo82drBEdm8CSDaEKBymVn86TBtgmPnDdmE64PTqJ";
        let expected_in_amount = Decimal::from(1078140);
        let expected_out_amount = Decimal::from(5593220329i64);
        let expected_parent_amm: Option<String> = Some(
            "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc"
                .parse()
                .unwrap(),
        );
        let expected_market_pubkey: Option<String> = Some(
            "Df7vWhJQXNvtQACxiDBFYba8ogaAi7oqTGvsW7jYphHD"
                .parse()
                .unwrap(),
        );

        let actual_in_amount = route_ix.in_amount;
        let actual_out_amount = route_ix.out_amount;
        let actual_in_mint = &route_ix.in_mint_pubkey;
        let actual_out_mint = &route_ix.out_mint_pubkey;
        let actual_swapper = &route_ix.swap_auth_pubkey;
        let actual_parent_amm = &route_ix.parent_amm_program_pubkey;
        let actual_market_pubkey = &route_ix.market_pubkey;

        assert_eq!(actual_swapper, expected_swapper);
        assert_eq!(actual_in_mint, expected_in_mint);
        assert_eq!(actual_out_mint, expected_out_mint);
        assert_eq!(actual_in_amount, expected_in_amount);
        assert_eq!(actual_out_amount, expected_out_amount);
        assert_eq!(*actual_parent_amm, expected_parent_amm);
        assert_eq!(*actual_market_pubkey, expected_market_pubkey);

        //second leg of first 2 hop swap
        let Dooot::SwapEvent(route_ix) = &dooot[2] else {
            panic!("Expected a swap dooot");
        };

        let expected_swapper = "HNarxovTeqhyjdVgn6jvDkkFRtbmGGPo5VoP4n9u31Ba";
        let expected_in_mint = "octo82drBEdm8CSDaEKBymVn86TBtgmPnDdmE64PTqJ";
        let expected_out_mint = "orcaEKTdK7LKz57vaAYr9QeNsVEPfiu6QeMU1kektZE";
        let expected_in_amount = Decimal::from(5593220329i64);
        let expected_out_amount = Decimal::from(100000);
        let expected_parent_amm: Option<String> = Some(
            "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc"
                .parse()
                .unwrap(),
        );
        let expected_market_pubkey: Option<String> = Some(
            "3t5fcfQ7cLRe1Cydoy19miMWcLaF335V2FVEKTErom5N"
                .parse()
                .unwrap(),
        );

        let actual_in_amount = route_ix.in_amount;
        let actual_out_amount = route_ix.out_amount;
        let actual_in_mint = &route_ix.in_mint_pubkey;
        let actual_out_mint = &route_ix.out_mint_pubkey;
        let actual_swapper = &route_ix.swap_auth_pubkey;
        let actual_parent_amm = &route_ix.parent_amm_program_pubkey;
        let actual_market_pubkey = &route_ix.market_pubkey;

        assert_eq!(actual_swapper, expected_swapper);
        assert_eq!(actual_in_mint, expected_in_mint);
        assert_eq!(actual_out_mint, expected_out_mint);
        assert_eq!(actual_in_amount, expected_in_amount);
        assert_eq!(actual_out_amount, expected_out_amount);
        assert_eq!(*actual_parent_amm, expected_parent_amm);
        assert_eq!(*actual_market_pubkey, expected_market_pubkey);

        //routed swap for first 2 hop swap
        let Dooot::SwapEvent(route_ix) = &dooot[3] else {
            panic!("Expected a swap dooot");
        };

        let expected_swapper = "HNarxovTeqhyjdVgn6jvDkkFRtbmGGPo5VoP4n9u31Ba";
        let expected_in_mint = "So11111111111111111111111111111111111111112";
        let expected_out_mint = "orcaEKTdK7LKz57vaAYr9QeNsVEPfiu6QeMU1kektZE";
        let expected_in_amount = Decimal::from(1078140);
        let expected_out_amount = Decimal::from(100000);
        let expected_parent_amm: Option<String> = None;
        let expected_market_pubkey: Option<String> = None;

        let actual_in_amount = route_ix.in_amount;
        let actual_out_amount = route_ix.out_amount;
        let actual_in_mint = &route_ix.in_mint_pubkey;
        let actual_out_mint = &route_ix.out_mint_pubkey;
        let actual_swapper = &route_ix.swap_auth_pubkey;
        let actual_parent_amm = &route_ix.parent_amm_program_pubkey;
        let actual_market_pubkey = &route_ix.market_pubkey;

        assert_eq!(actual_swapper, expected_swapper);
        assert_eq!(actual_in_mint, expected_in_mint);
        assert_eq!(actual_out_mint, expected_out_mint);
        assert_eq!(actual_in_amount, expected_in_amount);
        assert_eq!(actual_out_amount, expected_out_amount);
        assert_eq!(*actual_parent_amm, expected_parent_amm);
        assert_eq!(*actual_market_pubkey, expected_market_pubkey);

        //first leg of second 2 hop swap
        let Dooot::SwapEvent(route_ix) = &dooot[4] else {
            panic!("Expected a swap dooot");
        };

        let expected_swapper = "HNarxovTeqhyjdVgn6jvDkkFRtbmGGPo5VoP4n9u31Ba";
        let expected_in_mint = "So11111111111111111111111111111111111111112";
        let expected_out_mint = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263";
        let expected_in_amount = Decimal::from(1097300);
        let expected_out_amount = Decimal::from(633950781);
        let expected_parent_amm: Option<String> = Some(
            "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc"
                .parse()
                .unwrap(),
        );
        let expected_market_pubkey: Option<String> = Some(
            "3ne4mWqdYuNiYrYZC9TrA3FcfuFdErghH97vNPbjicr1"
                .parse()
                .unwrap(),
        );

        let actual_in_amount = route_ix.in_amount;
        let actual_out_amount = route_ix.out_amount;
        let actual_in_mint = &route_ix.in_mint_pubkey;
        let actual_out_mint = &route_ix.out_mint_pubkey;
        let actual_swapper = &route_ix.swap_auth_pubkey;
        let actual_parent_amm = &route_ix.parent_amm_program_pubkey;
        let actual_market_pubkey = &route_ix.market_pubkey;

        assert_eq!(actual_swapper, expected_swapper);
        assert_eq!(actual_in_mint, expected_in_mint);
        assert_eq!(actual_out_mint, expected_out_mint);
        assert_eq!(actual_in_amount, expected_in_amount);
        assert_eq!(actual_out_amount, expected_out_amount);
        assert_eq!(*actual_parent_amm, expected_parent_amm);
        assert_eq!(*actual_market_pubkey, expected_market_pubkey);

        //second leg of second 2 hop swap
        let Dooot::SwapEvent(route_ix) = &dooot[5] else {
            panic!("Expected a swap dooot");
        };

        let expected_swapper = "HNarxovTeqhyjdVgn6jvDkkFRtbmGGPo5VoP4n9u31Ba";
        let expected_in_mint = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263";
        let expected_out_mint = "orcaEKTdK7LKz57vaAYr9QeNsVEPfiu6QeMU1kektZE";
        let expected_in_amount = Decimal::from(633950781);
        let expected_out_amount = Decimal::from(100000);
        let expected_parent_amm: Option<String> = Some(
            "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc"
                .parse()
                .unwrap(),
        );
        let expected_market_pubkey: Option<String> =
            Some("6u3MNQnnaRsc3rJ4fiKsQ2SZZZmNenhEigJDGRFUTQYS".to_string());

        let actual_in_amount = route_ix.in_amount;
        let actual_out_amount = route_ix.out_amount;
        let actual_in_mint = &route_ix.in_mint_pubkey;
        let actual_out_mint = &route_ix.out_mint_pubkey;
        let actual_swapper = &route_ix.swap_auth_pubkey;
        let actual_parent_amm = &route_ix.parent_amm_program_pubkey;
        let actual_market_pubkey = &route_ix.market_pubkey;

        assert_eq!(actual_swapper, expected_swapper);
        assert_eq!(actual_in_mint, expected_in_mint);
        assert_eq!(actual_out_mint, expected_out_mint);
        assert_eq!(actual_in_amount, expected_in_amount);
        assert_eq!(actual_out_amount, expected_out_amount);
        assert_eq!(*actual_parent_amm, expected_parent_amm);
        assert_eq!(*actual_market_pubkey, expected_market_pubkey);

        //routed swap for second 2 hop swap
        let Dooot::SwapEvent(route_ix) = &dooot[6] else {
            panic!("Expected a swap dooot");
        };

        let expected_swapper = "HNarxovTeqhyjdVgn6jvDkkFRtbmGGPo5VoP4n9u31Ba";
        let expected_in_mint = "So11111111111111111111111111111111111111112";
        let expected_out_mint = "orcaEKTdK7LKz57vaAYr9QeNsVEPfiu6QeMU1kektZE";
        let expected_in_amount = Decimal::from(1097300);
        let expected_out_amount = Decimal::from(100000);
        let expected_parent_amm: Option<String> = None;
        let expected_market_pubkey: Option<String> = None;

        let actual_in_amount = route_ix.in_amount;
        let actual_out_amount = route_ix.out_amount;
        let actual_in_mint = &route_ix.in_mint_pubkey;
        let actual_out_mint = &route_ix.out_mint_pubkey;
        let actual_swapper = &route_ix.swap_auth_pubkey;
        let actual_parent_amm = &route_ix.parent_amm_program_pubkey;
        let actual_market_pubkey = &route_ix.market_pubkey;

        assert_eq!(actual_swapper, expected_swapper);
        assert_eq!(actual_in_mint, expected_in_mint);
        assert_eq!(actual_out_mint, expected_out_mint);
        assert_eq!(actual_in_amount, expected_in_amount);
        assert_eq!(actual_out_amount, expected_out_amount);
        assert_eq!(*actual_parent_amm, expected_parent_amm);
        assert_eq!(*actual_market_pubkey, expected_market_pubkey);
    }
    #[tokio::test]
    async fn add_liquidity_parser_invoke() {
        let message_json = fs::read_to_string("./tests/orca/orca_add_liq.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = OrcaWhirlies;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        let Dooot::LiquidityChangeEvent(liquidity_event) = &dooot[0] else {
            panic!("Expected a liquidity dooot");
        };
        let expected_owner = "6jDxfurJaWDBoFyyLAuJBZi24vynka6EuEPbvS88RRuu";
        let expected_pool = "ARs3pZiSyCutnm3X83MwP8zeg1BWCb5F7xeGszp4gHiz";
        let expected_mint_1 = "So11111111111111111111111111111111111111112";
        let expected_mint_2 = "5LafQUrVco6o7KMz42eqVEJ9LW31StPyGjeeu5sKoMtA";

        let expected_amount_1 = Decimal::from(12056133);
        let expected_amount_2 = Decimal::from(66769884520u64);

        let actual_owner = &liquidity_event.owner;
        let actual_pool = &liquidity_event.pool;
        let actual_mint_1 = &liquidity_event.mints[0];
        let actual_mint_2 = &liquidity_event.mints[1];
        let actual_amount_1 = &liquidity_event.amounts[0];
        let actual_amount_2 = &liquidity_event.amounts[1];

        assert_eq!(actual_owner, expected_owner);
        assert_eq!(actual_pool, expected_pool);
        assert_eq!(actual_mint_1, expected_mint_1);
        assert_eq!(actual_mint_2, expected_mint_2);
        assert_eq!(*actual_amount_1, expected_amount_1);
        assert_eq!(*actual_amount_2, expected_amount_2);
    }

    #[tokio::test]
    async fn remove_partial_liquidity_parser_invoke() {
        let message_json =
            fs::read_to_string("./tests/orca/orca_clmm_remove_part_liq.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = OrcaWhirlies;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        let user_dooot = dooot
            .iter()
            .find(|dooot| matches!(dooot, Dooot::ClmmUser(_)));

        let Dooot::ClmmUser(clmm_user) = (match user_dooot {
            Some(dooot) => dooot,
            None => panic!("Expected a clmm user dooot but none was found"),
        }) else {
            panic!("Expected a clmm user dooot");
        };
        assert_eq!(
            clmm_user.user_pubkey,
            "BoRiSxtNsHGQNKzic4sBa6779wPpp5e8UXywSVdHhpEG"
        );
        assert_eq!(clmm_user.liquidity_shares, "113978076");
    }

    #[tokio::test]
    async fn remove_all_liquidity_parser_invoke() {
        let message_json = fs::read_to_string("./tests/orca/orca_clmm_close_liq.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = OrcaWhirlies;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        let user_dooot = dooot
            .iter()
            .find(|dooot| matches!(dooot, Dooot::ClmmUser(_)));

        let Dooot::ClmmUser(clmm_user) = (match user_dooot {
            Some(dooot) => dooot,
            None => panic!("Expected a clmm user dooot but none was found"),
        }) else {
            panic!("Expected a clmm user dooot");
        };
        assert_eq!(
            clmm_user.user_pubkey,
            "BoRiSxtNsHGQNKzic4sBa6779wPpp5e8UXywSVdHhpEG"
        );
        assert_eq!(clmm_user.liquidity_shares, "0");
    }

    #[tokio::test]
    async fn remove_liquidity_parser_invoke() {
        let message_json = fs::read_to_string("./tests/orca/orca_remove_liq.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = OrcaWhirlies;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        let Dooot::LiquidityChangeEvent(liquidity_event) = &dooot[0] else {
            panic!("Expected a liquidity dooot");
        };

        let expected_owner = "JC8m5y9D7atuzD7mToWN8VVrtxyxCXQ3SFWMHFiLZagN";
        let expected_pool = "HGNEs8KUP9Ry2k2BAwxVByUYjQMUNXMSKDkAi3dX5B76";
        let expected_mint_1 = "So11111111111111111111111111111111111111112";
        let expected_mint_2 = "oreoN2tQbHXVaZsr3pf66A48miqcBXCDJozganhEJgz";

        let expected_amount_1 = Decimal::from(38840912u64) * Decimal::NEGATIVE_ONE;
        let expected_amount_2 = Decimal::from(1687820576u64) * Decimal::NEGATIVE_ONE;

        let actual_owner = &liquidity_event.owner;
        let actual_pool = &liquidity_event.pool;
        let actual_mint_1 = &liquidity_event.mints[0];
        let actual_mint_2 = &liquidity_event.mints[1];
        let actual_amount_1 = &liquidity_event.amounts[0];
        let actual_amount_2 = &liquidity_event.amounts[1];

        assert_eq!(actual_owner, expected_owner);
        assert_eq!(actual_pool, expected_pool);
        assert_eq!(actual_mint_1, expected_mint_1);
        assert_eq!(actual_mint_2, expected_mint_2);
        assert_eq!(*actual_amount_1, expected_amount_1);
        assert_eq!(*actual_amount_2, expected_amount_2);
    }
    #[tokio::test]
    async fn initialize_pool_parser_invoke() {
        // Load test txn json
        let message_json = fs::read_to_string("./tests/orca/orca_initialize_pool_v2.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = OrcaWhirlies;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();
        let Dooot::LPInfo(lp_info) = &dooot[7] else {
            panic!("Expected a LPInfoGState dooot");
        };

        let expected_pool = "H4eUyVBFw7WcQcpEabuqmDc6w33VDZocgC93waBwqWMz";
        let expected_lp_mint: Option<String> = None;
        let expected_underlyings1_mint = "5z3EqYQo9HiCEs3R84RCDMu2n7anpDMxRhdK8PSWmrRC".to_string();
        let expected_underlyings1_vault =
            "HDc5bxemLDk7JXo8GHezD7owMnbRFyXM3cEwtxJyMJ2F".to_string();
        let expected_underlyings2_mint = "HZ1JovNiVvGrGNiiYvEozEVgZ58xaU3RKwX8eACQBCt3".to_string();
        let expected_underlyings2_vault =
            "2A4YDAGEvjKwKc5DFhAHfoieiuMyreFtW6NpvZjoY2Aa".to_string();

        let actual_pool = &lp_info.pool;
        let actual_lp_mint = &lp_info.lp_mint;
        let actual_underlyings1_mint = &lp_info.underlyings[0].mint;
        let actual_underlyings1_vault = &lp_info.underlyings[0].vault;
        let actual_underlyings2_mint = &lp_info.underlyings[1].mint;
        let actual_underlyings2_vault = &lp_info.underlyings[1].vault;

        assert_eq!(actual_pool, expected_pool);
        assert_eq!(actual_lp_mint, &expected_lp_mint);
        assert_eq!(actual_underlyings1_mint, &expected_underlyings1_mint);
        assert_eq!(actual_underlyings1_vault, &expected_underlyings1_vault);
        assert_eq!(actual_underlyings2_mint, &expected_underlyings2_mint);
        assert_eq!(actual_underlyings2_vault, &expected_underlyings2_vault);
    }

    #[tokio::test]
    async fn swap_v2_parser() {
        // Load test txn json
        let message_json = fs::read_to_string("./tests/orca/orca_swap_v2.json").unwrap();
        let json_val: Value = serde_json::from_str(&message_json).unwrap();
        let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
        let step_utils = Rc::new(StepUtils::new(None));
        let mut dooot = vec![];
        let parser = OrcaWhirlies;
        parser
            .invoke(&decoded_txn, &mut dooot, &step_utils)
            .await
            .unwrap();

        let Dooot::SwapEvent(swap) = &dooot[0] else {
            panic!("Expected a Swap dooot");
        };

        let expected_swapper = "7iMGewmUKG7PXyQEi3NAhk424DJ6tNzNsRyDvW2k2jm5";
        let expected_in_mint = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";
        let expected_out_mint = "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo";
        let expected_in_amount = Decimal::from(499000000);
        let expected_out_amount = Decimal::from(498747529);
        let expected_parent_amm: Option<String> =
            "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".parse().ok();
        let expected_market_pubkey: Option<String> =
            "39GrsozbzM9Sg1U7EDnEtQ69fsVF3pmVtmV2DGDAQQJ5".parse().ok();

        let actual_swapper = &swap.swap_auth_pubkey;
        let actual_in_mint = &swap.in_mint_pubkey;
        let actual_out_mint = &swap.out_mint_pubkey;
        let actual_in_amount = swap.in_amount;
        let actual_out_amount = swap.out_amount;
        let actual_parent_amm = &swap.parent_amm_program_pubkey;
        let actual_market_pubkey = &swap.market_pubkey;

        assert_eq!(actual_swapper, expected_swapper);
        assert_eq!(actual_in_mint, expected_in_mint);
        assert_eq!(actual_out_mint, expected_out_mint);
        assert_eq!(actual_in_amount, expected_in_amount);
        assert_eq!(actual_out_amount, expected_out_amount);
        assert_eq!(*actual_parent_amm, expected_parent_amm);
        assert_eq!(*actual_market_pubkey, expected_market_pubkey);
    }

    // #[tokio::test]
    // async fn position_parser() {
    //     let cache = SimpleMemCache::new(None).await;

    //     // Load test txn json
    //     let message_json = fs::read_to_string("./tests/orca/position.json").unwrap();
    //     let json_val: Value = serde_json::from_str(&message_json).unwrap();
    //     let decoded_txn = DecodedTransaction::from_rabbit_message_json(json_val).unwrap();
    //     let step_utils = Rc::new(StepUtils::new(None));
    //     let mut dooot = vec![];
    //     let parser = OrcaWhirlies;
    //     parser
    //         .invoke(&decoded_txn, &mut dooot, &cache, &step_utils)
    //         .await
    //         .unwrap();
    //     let Dooot::LiquidityChangeEvent(liquidity_event) = &dooot[0] else {
    //         panic!("Expected a LiquidityChangeEvent dooot");
    //     };

    //     let expected_owner = "GLHc85L5BFzbMn9dHamTbwTjJmNRC8YfxSSgPrFMGkVz";
    //     let expected_pool = "Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE";
    //     let expected_mints = vec![
    //         "So11111111111111111111111111111111111111112",
    //         "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
    //     ];
    //     let expected_amounts = vec![Decimal::from(714046813), Decimal::from(128324770)];

    //     assert_eq!(liquidity_event.owner, expected_owner);
    //     assert_eq!(liquidity_event.pool, expected_pool);
    //     assert_eq!(liquidity_event.mints, expected_mints);
    //     assert_eq!(liquidity_event.amounts, expected_amounts);

    //     // Test GlobalCLMMState
    //     let Dooot::ClmmGlobal(global_state) = &dooot[1] else {
    //         panic!("Expected a ClmmGlobal dooot");
    //     };

    //     let expected_pool_pubkey = "Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE";
    //     let expected_total_liquidity_shares = Some("460785553701896".to_string());
    //     let expected_current_price = "8056209755388915043".to_string();

    //     assert_eq!(global_state.pool_pubkey, expected_pool_pubkey);
    //     assert_eq!(
    //         global_state.total_liquidity_shares,
    //         expected_total_liquidity_shares
    //     );
    //     assert_eq!(global_state.current_price, expected_current_price);

    //     // Test UserCLMMState
    //     let Dooot::ClmmUser(user_state) = &dooot[2] else {
    //         panic!("Expected a ClmmUser dooot");
    //     };

    //     let expected_user_pubkey = "GLHc85L5BFzbMn9dHamTbwTjJmNRC8YfxSSgPrFMGkVz";
    //     let expected_position_mint =
    //         Some("2ZnjPYxY8L7PWehPLzUbmWkzrJvXS6ZUQgWESuD9csvt".to_string());
    //     let expected_upper_price = "8163954235021617342".to_string();
    //     let expected_lower_price = "7956028388242727239".to_string();
    //     let expected_liquidity_shares = "56709805344".to_string();

    //     assert_eq!(user_state.user_pubkey, expected_user_pubkey);
    //     assert_eq!(user_state.position_mint, expected_position_mint);
    //     assert_eq!(user_state.upper_price, expected_upper_price);
    //     assert_eq!(user_state.lower_price, expected_lower_price);
    //     assert_eq!(user_state.liquidity_shares, expected_liquidity_shares);
    // }
}
