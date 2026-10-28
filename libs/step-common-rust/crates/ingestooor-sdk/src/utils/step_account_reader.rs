use anyhow::{anyhow, Context, Result};
use base64::{prelude::BASE64_STANDARD, Engine};
use borsh::BorshDeserialize;
use bytemuck::Pod;
use solana_sdk::pubkey::Pubkey;

#[must_use = "StepAccountReader holds an immutable reference to potentially-large heap data. Read the data, or drop the reader."]
pub struct StepAccountReader<'a> {
    current_index: usize,
    data: &'a [u8],
}

impl<'a> StepAccountReader<'a> {
    #[inline]
    pub fn new(data: &'a [u8]) -> StepAccountReader<'a> {
        StepAccountReader {
            current_index: 0,
            data,
        }
    }
    #[inline]
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    /// Move the current index to a specific position.
    ///
    /// Bounds are not checked.
    #[inline]
    pub fn move_index_to(&mut self, index: usize) {
        self.current_index = index;
    }

    /// Advance the current index by `by` bytes.
    ///
    /// The new index is clamped to the length of the data.
    #[inline]
    pub fn advance_clamped(&mut self, by: usize) {
        self.current_index = self.current_index.saturating_add(by).min(self.data.len());
    }

    /// Advance the current index to the end of the data.   
    #[inline]
    pub fn advance_to_end(&mut self) {
        self.current_index = self.data.len();
    }

    /// Advance the current index by `by` bytes.
    ///
    /// The new index is **not** clamped to the length of the data, and can go out of bounds.
    #[inline]
    pub fn advance_unclamped(&mut self, by: usize) {
        self.current_index = self.current_index.saturating_add(by);
    }

    /// Rewind the current index by `by` bytes.
    ///
    /// The new index is clamped to 0.
    #[inline]
    pub fn rewind_clamped(&mut self, by: usize) {
        self.current_index = self.current_index.saturating_sub(by);
    }

    /// Read `count` bytes from the data at the current index.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_bytes(&self, count: usize) -> Result<&'a [u8]> {
        let start = &self.current_index;
        let end = start + count;
        self.data.get(*start..end).context(format!(
            "Cannot read bytes from [{start}..{end}]. Went out of bounds (length: {})",
            self.data.len()
        ))
    }

    /// Read `count` bytes from the data at the current index and advance the index.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_next_bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        let bytes = self.read_bytes(count)?;
        self.advance_unclamped(count);
        Ok(bytes)
    }

    pub fn read_next_bytes_to_string(&mut self, count: usize) -> Result<String> {
        let bytes = self.read_next_bytes(count)?;
        Ok(String::from_utf8_lossy(bytes).to_string())
    }

    pub fn read_next_bool(&mut self) -> Result<bool> {
        let bytes = self.read_next_bytes(1)?;
        Ok(bytes[0] == 1)
    }

    /// Reads all bytes from the current index to the end of the data.
    ///
    /// Does not advance the index.
    #[inline]
    pub fn read_bytes_to_end_no_advance(&self) -> &'a [u8] {
        &self.data[self.current_index..]
    }

    /// Reads all bytes from the current index to the end of the data and advances the index.
    #[inline]
    pub fn read_bytes_to_end(&mut self) -> &'a [u8] {
        let bytes = self.read_bytes_to_end_no_advance();
        self.advance_unclamped(bytes.len());
        bytes
    }
    /// Reads a little-endian u32 from the data.
    ///
    /// Returns an error if the read would go out of bounds.
    pub fn read_le_u32(&self) -> Result<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes(bytes.try_into()?))
    }
    /// Reads a little-endian u128 from the data.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_le_u128(&self) -> Result<u128> {
        let bytes = self.read_bytes(16)?;
        Ok(u128::from_le_bytes(bytes.try_into()?))
    }

    /// Reads a little-endian u128 from the data and advances the index.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_next_le_u128(&mut self) -> Result<u128> {
        let value = self.read_le_u128()?;
        self.advance_unclamped(16);
        Ok(value)
    }

    /// Reads a little-endian i128 from the data.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_le_i128(&self) -> Result<i128> {
        let bytes = self.read_bytes(16)?;
        Ok(i128::from_le_bytes(bytes.try_into()?))
    }

    /// Reads a little-endian i128 from the data and advances the index.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_next_le_i128(&mut self) -> Result<i128> {
        let value = self.read_le_i128()?;
        self.advance_unclamped(16);
        Ok(value)
    }

    /// Reads a little-endian u64 from the data.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_le_u64(&self) -> Result<u64> {
        let bytes = self.read_bytes(8)?;
        Ok(u64::from_le_bytes(bytes.try_into()?))
    }

    /// Reads a little-endian u32 from the data and advances the index.
    ///
    /// Returns an error if the read would go out of bounds.
    pub fn read_next_le_u32(&mut self) -> Result<u32> {
        let value = self.read_le_u32()?;
        self.advance_unclamped(4);
        Ok(value)
    }

    /// Reads a little-endian u64 from the data and advances the index.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_next_le_u64(&mut self) -> Result<u64> {
        let value = self.read_le_u64()?;
        self.advance_unclamped(8);
        Ok(value)
    }

    /// Reads a little-endian u16 from the data.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_le_u16(&self) -> Result<u16> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_le_bytes(bytes.try_into()?))
    }

    /// Reads a little-endian u16 from the data by reading the next 2 bytes and advances the index.
    ///
    /// Returns an error if the read would go out of bounds.
    pub fn read_next_le_u16(&mut self) -> Result<u16> {
        let bytes = self.read_next_bytes(2)?;
        Ok(u16::from_le_bytes(bytes.try_into()?))
    }

    /// Reads a little-endian i64 from the data.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_le_i64(&self) -> Result<i64> {
        let bytes = self.read_bytes(8)?;
        Ok(i64::from_le_bytes(bytes.try_into()?))
    }

    /// Reads a little-endian i64 from the data and advances the index.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_next_le_i64(&mut self) -> Result<i64> {
        let value = self.read_le_i64()?;
        self.advance_unclamped(8);
        Ok(value)
    }

    /// Reads a little-endian i32 from the data.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_le_i32(&self) -> Result<i32> {
        let bytes = self.read_bytes(4)?;
        Ok(i32::from_le_bytes(bytes.try_into()?))
    }

    /// Reads a little-endian i32 from the data and advances the index.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_next_le_i32(&mut self) -> Result<i32> {
        let value = self.read_le_i32()?;
        self.advance_unclamped(4);
        Ok(value)
    }

    /// Reads a little-endian i16 from the data and advances the index.
    ///
    /// Returns an error if the read would go out of bounds.
    pub fn read_next_le_i16(&mut self) -> Result<i16> {
        let bytes = self.read_next_bytes(2)?;
        Ok(i16::from_le_bytes(bytes.try_into()?))
    }

    /// Reads a little-endian i8 from the data and advances the index.
    ///
    /// Returns an error if the read would go out of bounds.
    pub fn read_next_le_i8(&mut self) -> Result<i8> {
        let bytes = self.read_next_bytes(1)?;
        Ok(i8::from_le_bytes(bytes.try_into()?))
    }

    /// Reads a little-endian f64 from the data and advances the index.
    ///
    /// Returns an error if the read would go out of bounds.
    pub fn read_next_le_f64(&mut self) -> Result<f64> {
        let bytes = self.read_next_bytes(8)?;
        Ok(f64::from_le_bytes(bytes.try_into()?))
    }

    /// Reads a little-endian f32 from the data and advances the index.
    ///
    /// Returns an error if the read would go out of bounds.
    pub fn read_next_le_f32(&mut self) -> Result<f32> {
        let bytes = self.read_next_bytes(4)?;
        Ok(f32::from_le_bytes(bytes.try_into()?))
    }

    /// Reads a single byte from the data.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_single_byte(&self) -> Result<u8> {
        let bytes = self.read_bytes(1)?;
        Ok(bytes[0])
    }

    /// Reads a single byte from the data and advances the index.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_next_single_byte(&mut self) -> Result<u8> {
        let byte = self.read_single_byte()?;
        self.advance_unclamped(1);
        Ok(byte)
    }

    /// Reads a pubkey from the data.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_pubkey(&self) -> Result<&'a Pubkey> {
        self.read_pod_type()
    }

    /// Reads a pubkey from the data and advances the index.
    ///
    /// Returns an error if the read would go out of bounds.
    #[inline]
    pub fn read_next_pubkey(&mut self) -> Result<&'a Pubkey> {
        let pubkey = self.read_pubkey()?;
        self.advance_unclamped(32);
        Ok(pubkey)
    }

    /// Read a Pod type from the data.
    ///
    /// Returns an error if the read would go out of bounds, or if `bytemuck::try_from_bytes` fails.
    #[inline]
    pub fn read_pod_type<T: Pod>(&self) -> Result<&'a T> {
        let bytes = self.read_bytes(std::mem::size_of::<T>())?;
        bytemuck::try_from_bytes(bytes).map_err(|e| anyhow!(e))
    }

    /// Read a Pod type from the data and advance the index.
    ///
    /// Returns an error if the read would go out of bounds, or if `bytemuck::try_from_bytes` fails.
    #[inline]
    pub fn read_next_pod_type<T: Pod>(&mut self) -> Result<&'a T> {
        let pod = self.read_pod_type::<T>()?;
        self.advance_unclamped(std::mem::size_of::<T>());
        Ok(pod)
    }

    /// Read a Borsh-deserializable type from the data.
    ///
    /// Note that this copies the underlying bytes from \[current_index..\] into a new slice.
    #[inline]
    pub fn read_borsh_type<T: BorshDeserialize>(&self) -> Result<T> {
        let mut bytes = self.read_bytes_to_end_no_advance();
        Ok(T::deserialize(&mut bytes)?)
    }

    /// Read a Borsh-deserializable type from the data and advance the index.
    ///
    /// Note that this copies the underlying bytes from \[current_index..\] into a new slice.
    #[inline]
    pub fn read_next_borsh_type<T: BorshDeserialize>(&mut self) -> Result<T> {
        let mut bytes = self.read_bytes_to_end_no_advance();

        let len_before = bytes.len();
        let value = T::deserialize(&mut bytes)?;
        let len_after = bytes.len();

        self.advance_unclamped(len_before - len_after);
        Ok(value)
    }

    pub fn read_next_bytes_to_base_64(&mut self, count: usize) -> Result<String> {
        let bytes = self.read_next_bytes(count)?;
        Ok(BASE64_STANDARD.encode(bytes))
    }
}

#[cfg(test)]
mod tests {
    use borsh::{BorshDeserialize, BorshSerialize};
    use bytemuck::{Pod, Zeroable};
    use solana_sdk::pubkey::Pubkey;

    use crate::utils::step_account_reader::StepAccountReader;

    #[derive(BorshSerialize, BorshDeserialize)]
    pub struct Thing {
        pk1: Pubkey,
        v1: u64,
        pk2: Pubkey,
        pod1: SomePodType,
        pod2: SomePodType,
        borsh1: SomePodType,
        borsh2: SomePodType,
    }

    #[derive(Clone, Copy, Zeroable, BorshSerialize, BorshDeserialize, PartialEq, Eq, Debug)]
    pub struct SomePodType {
        v1: u64,
        v2: u64,
    }

    unsafe impl Pod for SomePodType {}

    #[test]
    fn test_reader() {
        let pk1 = Pubkey::new_unique();
        let pk2 = Pubkey::new_unique();
        let pod1 = SomePodType { v1: 1, v2: 2 };
        let pod2 = SomePodType { v1: 3, v2: 4 };
        let borsh1 = SomePodType { v1: 5, v2: 6 };
        let borsh2 = SomePodType { v1: 7, v2: 8 };

        let t1 = Thing {
            pk1,
            v1: 42,
            pk2,
            pod1,
            pod2,
            borsh1,
            borsh2,
        };

        let mut buf = vec![];
        t1.serialize(&mut buf).unwrap();

        let mut reader = StepAccountReader::new(&buf);

        let pk1_read = reader.read_next_pubkey();
        let v1_read = reader.read_next_le_u64();
        let pk2_read = reader.read_next_pubkey();
        let pod1_read = reader.read_next_pod_type::<SomePodType>();
        let pod2_read = reader.read_next_pod_type::<SomePodType>();
        let borsh1_read = reader.read_next_borsh_type::<SomePodType>();
        let borsh2_read = reader.read_next_borsh_type::<SomePodType>();

        assert_eq!(pk1, *pk1_read.unwrap());
        assert_eq!(42, v1_read.unwrap());
        assert_eq!(pk2, *pk2_read.unwrap());
        assert_eq!(pod1, *pod1_read.unwrap());
        assert_eq!(pod2, *pod2_read.unwrap());
        assert_eq!(borsh1, borsh1_read.unwrap());
        assert_eq!(borsh2, borsh2_read.unwrap());
    }
}
