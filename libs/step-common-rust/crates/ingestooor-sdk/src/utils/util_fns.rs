use std::collections::VecDeque;

use anyhow::{anyhow, Result};
use base64::prelude::*;
use bytemuck::{Pod, Zeroable};
use chrono::{DateTime, NaiveDateTime};
use solana_sdk::pubkey::Pubkey;
use solana_transaction_status::{
    parse_instruction::ParsedInstruction, UiInstruction, UiParsedInstruction,
    UiPartiallyDecodedInstruction,
};
use spl_token::{solana_program::program_pack::Pack, state::Mint};

use crate::utils::constants::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID};

#[inline]
pub fn get_program_id_of_ix(ix: &UiInstruction) -> Option<String> {
    match ix {
        UiInstruction::Parsed(p) => match p {
            UiParsedInstruction::Parsed(parsed) => Some(parsed.program_id.clone()),
            UiParsedInstruction::PartiallyDecoded(partial_dec) => {
                Some(partial_dec.program_id.clone())
            }
        },
        UiInstruction::Compiled(_) => None,
    }
}

#[inline]
pub fn get_stack_height_of_ix(ix: &UiInstruction) -> Option<&u32> {
    match ix {
        UiInstruction::Parsed(p) => match p {
            UiParsedInstruction::Parsed(parsed) => parsed.stack_height.as_ref(),
            UiParsedInstruction::PartiallyDecoded(partial_dec) => partial_dec.stack_height.as_ref(),
        },
        UiInstruction::Compiled(comp) => comp.stack_height.as_ref(),
    }
}

#[inline]
pub fn get_partially_decoded_variant(ix: &UiInstruction) -> Option<&UiPartiallyDecodedInstruction> {
    match ix {
        UiInstruction::Parsed(UiParsedInstruction::PartiallyDecoded(ix)) => Some(ix),
        _ => None,
    }
}

#[inline]
pub fn get_parsed_variant(ix: &UiInstruction) -> Option<&ParsedInstruction> {
    match ix {
        UiInstruction::Parsed(UiParsedInstruction::Parsed(ix)) => Some(ix),
        _ => None,
    }
}

#[inline]
pub fn get_spl_version_from_program_id(program_id: &str) -> i16 {
    if program_id == TOKEN_2022_PROGRAM_ID {
        2
    } else {
        1
    }
}

/// Unpacks a mint without the strict 82 byte requirement. Useful for getting basic info for token22s
///
/// Will fail if the slice is too small, of course
#[inline]
pub fn unpack_mint_any_size(data: &[u8]) -> Result<Mint> {
    let first_82 = data
        .get(0..82)
        .ok_or_else(|| anyhow!("Slice too small for mint"))?;
    Ok(Mint::unpack_unchecked(first_82)?)
}

#[inline]
pub fn decode_base64_data(data: &str) -> Result<Vec<u8>> {
    let decoded = BASE64_STANDARD.decode(data)?;
    Ok(decoded)
}

#[inline]
pub fn decode_base64_compressed_data(data: &str) -> Result<Vec<u8>> {
    let decoded = BASE64_STANDARD.decode(data)?;
    let decompressed = zstd::decode_all(decoded.as_slice())?;
    Ok(decompressed)
}

pub fn zerocopy_data<T: Pod + Zeroable>(
    data: Vec<u8>,
    disc_offset: Option<usize>,
) -> Result<Box<T>> {
    //move to a vecdeque, this does not copy
    let mut data = VecDeque::from(data);
    //split the discriminator off, this does not copy
    let mut data = VecDeque::split_off(&mut data, disc_offset.unwrap_or(8));
    // check if the data is larger than T
    if data.len() < std::mem::size_of::<T>() {
        return Err(anyhow!("Data size mismatch in zerocopy_data"));
    }
    // Take all the bytes to size of T
    data.truncate(std::mem::size_of::<T>());
    //move back to a Vec, this does not copy
    let data = Vec::from(data);
    //change into a boxed array, this does not copy
    let data = data.into_boxed_slice();
    //get a pointer to the array, this does not copy
    let data = Box::into_raw(data);
    //cast the pointer to the type, this does not copy
    let data = data.cast::<T>();
    unsafe {
        //move the pointer into a Box<T>, this does not copy
        //but is unsafe because now T must now own the memory
        let data = Box::from_raw(data);
        Ok(data)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TokenAccountType {
    TokenKegTokenAccount,
    TokenKegMint,
    TokenT22TokenAccount,
    TokenT22TokenAccountWithExtensions,
    TokenT22Mint,
    TokenT22MintWithExtensions,
    TokenT22Multisig,
}

pub fn check_token_account_type(pid: &str, data: &[u8]) -> Option<TokenAccountType> {
    if pid == TOKEN_PROGRAM_ID {
        if data.len() == 82 {
            return Some(TokenAccountType::TokenKegMint);
        } else if data.len() == 165 {
            return Some(TokenAccountType::TokenKegTokenAccount);
        }
    } else if pid == TOKEN_2022_PROGRAM_ID {
        if data.len() == 82 {
            return Some(TokenAccountType::TokenT22Mint);
        } else if data.len() == 355 {
            return Some(TokenAccountType::TokenT22Multisig);
        } else if data.len() > 165 {
            // 0 is Uninitialized, 1 is Mint, 2 is Account
            let descrim = data[165];
            match descrim {
                1 => return Some(TokenAccountType::TokenT22MintWithExtensions),
                2 => return Some(TokenAccountType::TokenT22TokenAccount),
                _ => return None,
            }
        } else if data.len() == 165 {
            return Some(TokenAccountType::TokenT22TokenAccount);
        }
    }
    None
}

pub fn convert_blocktime_to_micros(
    block_time: i64,
    slot_second_idx: Option<u8>,
    index_in_block: Option<usize>,
) -> Result<NaiveDateTime> {
    // This should always exist during realtime. Make it Some(0) for state init
    let slot_second_idx = slot_second_idx.ok_or(anyhow!("Failed to get slot second index"))?;
    // can actually be None by slot stats parser
    let index_in_block = index_in_block.unwrap_or(0);
    // Expand the blocktime to microseconds
    let blocktime_in_micros = block_time * 1_000_000;
    // We add the slot .1 adjustment to the blocktime
    let slot_adjustment = (slot_second_idx as i64) * 100_000;
    let adjusted_blocktime = blocktime_in_micros + slot_adjustment;
    // We add the index in block adjustment to the blocktime
    let adjusted_bt_with_index = adjusted_blocktime + (index_in_block as i64);
    let datetime_with_index = DateTime::from_timestamp_micros(adjusted_bt_with_index)
        .ok_or(anyhow!("Failed to convert blocktime to datetime"))?;
    Ok(datetime_with_index.naive_utc())
}

pub fn get_associated_token_address(
    wallet_address: &Pubkey,
    token_mint_address: &Pubkey,
    program_id: &Pubkey,
    token_program_id: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            &wallet_address.to_bytes(),
            &token_program_id.to_bytes(),
            &token_mint_address.to_bytes(),
        ],
        program_id,
    )
}
