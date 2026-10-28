use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

use crate::utils::{
    step_account_reader::StepAccountReader,
    t22_extensions::{get_extension_data_from_tlv, T22ExtensionType},
};

pub const TOKEN_EXTENSIONS_TYPE_LEN: usize = 2;
pub const TOKEN_EXTENSIONS_LENGTH_LEN: usize = 2;

pub struct MetadataExtensionData {
    pub pointer_address: Option<Pubkey>,
    pub metadata: Option<MetadataExtensionFields>,
}

pub struct MetadataExtensionFields {
    pub name: String,
    pub symbol: String,
    pub uri: String,
}

/// All fields are optional here since anyone can upload anything as metadata,
/// and it may or may not conform to the standard.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TokenJSONMetadata {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
}

impl TokenJSONMetadata {
    pub fn as_trimmed(self) -> Self {
        TokenJSONMetadata {
            name: self.name.as_ref().map(|s| trim_padding_bytes(s)),
            symbol: self.symbol.as_ref().map(|s| trim_padding_bytes(s)),
            description: self.description.as_ref().map(|s| trim_padding_bytes(s)),
            image: self.image.as_ref().map(|s| trim_padding_bytes(s)),
        }
    }
}

#[inline]
pub fn trim_padding_bytes(string: &str) -> String {
    string.replace('\0', "")
}

/// Ripped from the following
///
/// - https://github.com/solana-program/token-2022/blob/main/clients/js-legacy/src/extensions/metadataPointer/state.ts
/// - https://github.com/solana-program/token-metadata/blob/main/clients/js-legacy/src/state.ts
/// - https://github.com/solana-program/token-2022/blob/main/clients/js-legacy/src/extensions/extensionType.ts
/// - https://github.com/solana-program/token-2022/blob/main/clients/js-legacy/src/extensions/extensionType.ts#L280
#[inline]
pub fn get_metadata_extension_data_from_tlv(data: &[u8]) -> Option<MetadataExtensionData> {
    let pointer_data = get_extension_data_from_tlv(T22ExtensionType::MetadataPointer, data);
    let meta = get_extension_data_from_tlv(T22ExtensionType::TokenMetadata, data);

    let pointer_address = pointer_data.and_then(|d| Pubkey::try_from(d.get(32..64)?).ok());
    let metadata = meta.and_then(|data| {
        let mut reader = StepAccountReader::new(data);
        // Skips `authority` and `mint`
        reader.advance_unclamped(64);
        let name = read_next_t22_metadata_string_field(&mut reader)?;
        let symbol = read_next_t22_metadata_string_field(&mut reader)?;
        let uri = read_next_t22_metadata_string_field(&mut reader)?;
        let fields = MetadataExtensionFields { name, symbol, uri };

        Some(fields)
    });

    Some(MetadataExtensionData {
        pointer_address,
        metadata,
    })
}

fn read_next_t22_metadata_string_field(reader: &mut StepAccountReader) -> Option<String> {
    // Sizes of strings in token22 metadata are u32s, different than borsh
    let len = reader.read_next_le_u32().ok()? as usize;
    let bytes = reader.read_next_bytes(len).ok()?;
    String::from_utf8(bytes.to_vec()).ok()
}
