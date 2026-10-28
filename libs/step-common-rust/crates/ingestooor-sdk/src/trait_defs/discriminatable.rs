use anchor_lang::{Discriminator, Owner};
use solana_sdk::pubkey::Pubkey;

pub trait Discriminatable {
    fn discriminator() -> (Pubkey, &'static [u8]);
}

impl<T> Discriminatable for T
where
    T: Discriminator + Owner,
{
    fn discriminator() -> (Pubkey, &'static [u8]) {
        (T::owner(), T::DISCRIMINATOR)
    }
}
