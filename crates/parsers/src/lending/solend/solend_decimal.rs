//! Math for preserving precision of token amounts which are limited
//! by the SPL Token program to be at most u64::MAX.
//!
//! SolendDecimals are internally scaled by a WAD (10^18) to preserve
//! precision up to 18 SolendDecimal places. SolendDecimals are sized to support
//! both serialization and precise math for the full range of
//! unsigned 64-bit integers. The underlying representation is a
//! u192 rather than u256 to reduce compute cost while losing
//! support for arithmetic operations at the high end of u64 range.

#![allow(clippy::assign_op_pattern)]
#![allow(clippy::ptr_offset_with_cast)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::unwrap_used)]

use crate::lending::solend::common::TryDiv;
use crate::lending::solend::{common::*, Rate};
use borsh::{BorshDeserialize, BorshSerialize};
use std::{convert::TryFrom, fmt};
use uint::construct_uint;

// U192 with 192 bits consisting of 3 x 64-bit words
construct_uint! {
    pub struct U192(3);
}

/// Implement BorshSerialize and BorshDeserialize for U192
impl BorshSerialize for U192 {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // U192 is composed of 3 u64 words
        for word in self.0.iter() {
            word.serialize(writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for U192 {
    fn deserialize(buf: &mut &[u8]) -> std::io::Result<Self> {
        let mut words = [0u64; 3];
        for word in &mut words {
            *word = BorshDeserialize::deserialize(buf)?;
        }
        Ok(U192(words))
    }

    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut words = [0u64; 3];
        for word in &mut words {
            *word = BorshDeserialize::deserialize_reader(reader)?;
        }
        Ok(U192(words))
    }
}

/// Implement BorshSerialize and BorshDeserialize for SolendDecimal
impl BorshSerialize for SolendDecimal {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.0.serialize(writer)
    }
}

impl BorshDeserialize for SolendDecimal {
    fn deserialize(buf: &mut &[u8]) -> std::io::Result<Self> {
        let u192_value = U192::deserialize(buf)?;
        Ok(SolendDecimal(u192_value))
    }

    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let u192_value = U192::deserialize_reader(reader)?;
        Ok(SolendDecimal(u192_value))
    }
}

/// Large SolendDecimal values, precise to 18 digits
#[derive(Clone, Copy, Default, PartialEq, PartialOrd, Eq, Ord)]
pub struct SolendDecimal(pub U192);

impl SolendDecimal {
    /// One
    pub fn one() -> Self {
        Self(Self::wad())
    }

    /// Zero
    pub fn zero() -> Self {
        Self(U192::zero())
    }

    // OPTIMIZE: use const slice when fixed in BPF toolchain
    fn wad() -> U192 {
        U192::from(WAD)
    }

    // OPTIMIZE: use const slice when fixed in BPF toolchain
    fn half_wad() -> U192 {
        U192::from(HALF_WAD)
    }

    /// Create scaled SolendDecimal from percent value
    pub fn from_percent(percent: u8) -> Self {
        Self(U192::from(percent as u64 * PERCENT_SCALER))
    }

    /// Create scaled SolendDecimal from deca bps value
    pub fn from_deca_bps(deca_bps: u8) -> Self {
        Self::from(deca_bps as u64).try_div(1000).unwrap()
    }

    /// Create scaled SolendDecimal from bps value
    pub fn from_bps(bps: u64) -> Self {
        Self::from(bps).try_div(10_000).unwrap()
    }

    /// Return raw scaled value if it fits within u128
    #[allow(clippy::wrong_self_convention)]
    pub fn to_scaled_val(&self) -> anyhow::Result<u128> {
        u128::try_from(self.0).map_err(|_| anyhow::anyhow!("ConversionError"))
    }

    /// Create SolendDecimal from scaled value
    pub fn from_scaled_val(scaled_val: u128) -> Self {
        Self(U192::from(scaled_val))
    }

    /// Round scaled SolendDecimal to u64
    pub fn try_round_u64(&self) -> anyhow::Result<u64> {
        let rounded_val = Self::half_wad()
            .checked_add(self.0)
            .ok_or(anyhow::anyhow!("MathOverflow"))?
            .checked_div(Self::wad())
            .ok_or(anyhow::anyhow!("MathOverflow"))?;
        u64::try_from(rounded_val).map_err(|_| anyhow::anyhow!("ConversionError"))
    }

    /// Ceiling scaled SolendDecimal to u64
    pub fn try_ceil_u64(&self) -> anyhow::Result<u64> {
        let ceil_val = Self::wad()
            .checked_sub(U192::from(1u64))
            .ok_or(anyhow::anyhow!("MathOverflow"))?
            .checked_add(self.0)
            .ok_or(anyhow::anyhow!("MathOverflow"))?
            .checked_div(Self::wad())
            .ok_or(anyhow::anyhow!("MathOverflow"))?;
        u64::try_from(ceil_val).map_err(|_| anyhow::anyhow!("ConversionError"))
    }

    /// Floor scaled SolendDecimal to u64
    pub fn try_floor_u64(&self) -> anyhow::Result<u64> {
        let ceil_val = self
            .0
            .checked_div(Self::wad())
            .ok_or(anyhow::anyhow!("MathOverflow"))?;
        u64::try_from(ceil_val).map_err(|_| anyhow::anyhow!("ConversionError"))
    }
}

impl fmt::Display for SolendDecimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut scaled_val = self.0.to_string();
        if scaled_val.len() <= SCALE {
            scaled_val.insert_str(0, &vec!["0"; SCALE - scaled_val.len()].join(""));
            scaled_val.insert_str(0, "0.");
        } else {
            scaled_val.insert(scaled_val.len() - SCALE, '.');
        }
        f.write_str(&scaled_val)
    }
}

impl fmt::Debug for SolendDecimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl From<u64> for SolendDecimal {
    fn from(val: u64) -> Self {
        Self(Self::wad() * U192::from(val))
    }
}

impl From<u128> for SolendDecimal {
    fn from(val: u128) -> Self {
        Self(Self::wad() * U192::from(val))
    }
}

impl From<Rate> for SolendDecimal {
    fn from(val: Rate) -> Self {
        Self(U192::from(val.to_scaled_val()))
    }
}

impl TryAdd for SolendDecimal {
    fn try_add(self, rhs: Self) -> anyhow::Result<Self> {
        Ok(Self(
            self.0
                .checked_add(rhs.0)
                .ok_or(anyhow::anyhow!("MathOverflow"))?,
        ))
    }
}

impl TrySub for SolendDecimal {
    fn try_sub(self, rhs: Self) -> anyhow::Result<Self> {
        Ok(Self(
            self.0
                .checked_sub(rhs.0)
                .ok_or(anyhow::anyhow!("MathOverflow"))?,
        ))
    }
}

impl SaturatingSub for SolendDecimal {
    fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl TryDiv<u64> for SolendDecimal {
    fn try_div(self, rhs: u64) -> anyhow::Result<Self> {
        Ok(Self(
            self.0
                .checked_div(U192::from(rhs))
                .ok_or(anyhow::anyhow!("MathOverflow"))?,
        ))
    }
}

impl TryDiv<Rate> for SolendDecimal {
    fn try_div(self, rhs: Rate) -> anyhow::Result<Self> {
        self.try_div(Self::from(rhs))
    }
}

impl TryDiv<SolendDecimal> for SolendDecimal {
    fn try_div(self, rhs: Self) -> anyhow::Result<Self> {
        Ok(Self(
            self.0
                .checked_mul(Self::wad())
                .ok_or(anyhow::anyhow!("MathOverflow"))?
                .checked_div(rhs.0)
                .ok_or(anyhow::anyhow!("MathOverflow"))?,
        ))
    }
}

impl TryMul<u64> for SolendDecimal {
    fn try_mul(self, rhs: u64) -> anyhow::Result<Self> {
        Ok(Self(
            self.0
                .checked_mul(U192::from(rhs))
                .ok_or(anyhow::anyhow!("MathOverflow"))?,
        ))
    }
}

impl TryMul<Rate> for SolendDecimal {
    fn try_mul(self, rhs: Rate) -> anyhow::Result<Self> {
        self.try_mul(Self::from(rhs))
    }
}

impl TryMul<SolendDecimal> for SolendDecimal {
    fn try_mul(self, rhs: Self) -> anyhow::Result<Self> {
        Ok(Self(
            self.0
                .checked_mul(rhs.0)
                .ok_or(anyhow::anyhow!("MathOverflow"))?
                .checked_div(Self::wad())
                .ok_or(anyhow::anyhow!("MathOverflow"))?,
        ))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_scaler() {
        assert_eq!(U192::exp10(SCALE), SolendDecimal::wad());
    }

    #[test]
    fn test_u192() {
        let one = U192::from(1);
        assert_eq!(one.0, [1u64, 0, 0]);

        let wad = SolendDecimal::wad();
        assert_eq!(wad.0, [WAD, 0, 0]);

        let hundred = SolendDecimal::from(100u64);
        // 2^64 * 5 + 7766279631452241920 = 1e20
        assert_eq!(hundred.0 .0, [7766279631452241920, 5, 0]);
    }

    #[test]
    fn test_from_percent() {
        let left = SolendDecimal::from_percent(20);
        let right = SolendDecimal::from(20u64)
            .try_div(SolendDecimal::from(100u64))
            .unwrap();

        assert_eq!(left, right);
    }

    #[test]
    fn test_from_deca_bps() {
        let left = SolendDecimal::from_deca_bps(250);
        assert_eq!(left, SolendDecimal::from_percent(25));
    }

    #[test]
    fn test_from_bps() {
        let left = SolendDecimal::from_bps(190000);
        assert_eq!(left, SolendDecimal::from(19u64));
    }

    #[test]
    fn test_to_scaled_val() {
        assert_eq!(
            SolendDecimal(U192::from(u128::MAX))
                .to_scaled_val()
                .unwrap(),
            u128::MAX
        );

        assert!(SolendDecimal(U192::from(u128::MAX))
            .try_add(SolendDecimal(U192::from(1)))
            .unwrap()
            .to_scaled_val()
            .is_err());
    }

    #[test]
    fn test_round_floor_ceil_u64() {
        let mut val = SolendDecimal::one();
        assert_eq!(val.try_round_u64().unwrap(), 1);
        assert_eq!(val.try_floor_u64().unwrap(), 1);
        assert_eq!(val.try_ceil_u64().unwrap(), 1);

        val = val
            .try_add(SolendDecimal::from_scaled_val(HALF_WAD as u128 - 1))
            .unwrap();
        assert_eq!(val.try_round_u64().unwrap(), 1);
        assert_eq!(val.try_floor_u64().unwrap(), 1);
        assert_eq!(val.try_ceil_u64().unwrap(), 2);

        val = val.try_add(SolendDecimal::from_scaled_val(1)).unwrap();
        assert_eq!(val.try_round_u64().unwrap(), 2);
        assert_eq!(val.try_floor_u64().unwrap(), 1);
        assert_eq!(val.try_ceil_u64().unwrap(), 2);
    }

    #[test]
    fn test_display() {
        assert_eq!(
            SolendDecimal::from(1u64).to_string(),
            "1.000000000000000000"
        );
        assert_eq!(
            SolendDecimal::from_scaled_val(1u128).to_string(),
            "0.000000000000000001"
        );
    }

    #[test]
    fn test_saturating_sub() {
        assert_eq!(
            SolendDecimal::from(1u64).saturating_sub(SolendDecimal::from(2u64)),
            SolendDecimal::zero()
        );
        assert_eq!(
            SolendDecimal::from(2u64).saturating_sub(SolendDecimal::from(1u64)),
            SolendDecimal::one()
        );
    }
}
