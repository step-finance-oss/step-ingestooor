use anyhow::{anyhow, Result};
use solana_sdk::pubkey::Pubkey;
use std::ops::Deref;

use super::discriminatable::Discriminatable;

pub trait RefFromByteRef {
    fn from_bytes<'a>(bytes: &'a [u8]) -> Result<MaybeRef<'a, Self>>
    where
        Self: 'a,
        Self: Sized;
}

pub struct MaybeRef<'a, T> {
    inner: Option<T>,
    inner_ref: Option<&'a T>,
}
impl<'a, T> MaybeRef<'a, T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner: Some(inner),
            inner_ref: None,
        }
    }
    pub fn new_ref(inner: &'a T) -> Self {
        Self {
            inner: None,
            inner_ref: Some(inner),
        }
    }
}
impl<T> Deref for MaybeRef<'_, T> {
    type Target = T;

    #[allow(clippy::unwrap_used)]
    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().or(self.inner_ref).unwrap()
    }
}

// Pod implementation
impl<T> RefFromByteRef for T
where
    T: bytemuck::Pod,
{
    fn from_bytes<'a>(bytes: &'a [u8]) -> Result<MaybeRef<'a, T>>
    where
        T: 'a,
    {
        let b = bytemuck::try_from_bytes::<T>(bytes).map_err(|e| {
            anyhow!(
                "Failed to deserialize {} from {} bytes: {}",
                std::any::type_name::<T>(),
                bytes.len(),
                e
            )
        })?;
        Ok(MaybeRef::new_ref(b))
    }
}

// Wrapper type for Borsh deserialization
pub struct AsBorsh<T>(pub T);

impl<T> Deref for AsBorsh<T> {
    type Target = T;

    #[allow(clippy::unwrap_used)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Discriminatable for AsBorsh<T>
where
    T: Discriminatable,
{
    fn discriminator() -> (Pubkey, &'static [u8]) {
        T::discriminator()
    }
}

impl<T> RefFromByteRef for AsBorsh<T>
where
    T: borsh::BorshDeserialize,
{
    fn from_bytes<'a>(bytes: &'a [u8]) -> Result<MaybeRef<'a, Self>>
    where
        T: 'a,
    {
        let mut p = bytes;
        let de = T::deserialize(&mut p)?;
        Ok(MaybeRef::new(AsBorsh(de)))
    }
}
