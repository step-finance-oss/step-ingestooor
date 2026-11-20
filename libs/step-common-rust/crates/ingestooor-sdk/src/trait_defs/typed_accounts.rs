use anyhow::{anyhow, Result};

use super::{
    discriminatable::Discriminatable,
    pod_types::{MaybeRef, RefFromByteRef},
    transaction::AccountDatumIterItem,
};

pub struct AccountTypedPrePost<'a, T>
where
    T: RefFromByteRef,
{
    pub pubkey: &'a str,
    pub is_changed: bool,
    pub was_created: bool,
    pre_bytes: Option<Vec<u8>>,
    post_bytes: Option<Vec<u8>>,
    _marker: std::marker::PhantomData<T>,
}

impl<'a, T> AccountTypedPrePost<'a, T>
where
    T: RefFromByteRef,
{
    pub fn new(
        pubkey: &'a str,
        is_changed: bool,
        was_created: bool,
        pre_bytes: Option<Vec<u8>>,
        post_bytes: Option<Vec<u8>>,
    ) -> AccountTypedPrePost<'a, T> {
        Self {
            pubkey,
            is_changed,
            was_created,
            pre_bytes,
            post_bytes,
            _marker: std::marker::PhantomData,
        }
    }

    #[inline]
    pub fn get_pre(&'a self) -> Result<Option<MaybeRef<'a, T>>> {
        self.pre_bytes
            .as_ref()
            .map(|v| T::from_bytes(v).map_err(|e| anyhow!(e)))
            .transpose()
    }

    #[inline]
    pub fn get_post(&'a self) -> Result<Option<MaybeRef<'a, T>>> {
        self.post_bytes
            .as_ref()
            .map(|v| T::from_bytes(v).map_err(|e| anyhow!(e)))
            .transpose()
    }

    #[inline]
    pub fn pre_bytes_exist(&self) -> bool {
        self.pre_bytes.is_some()
    }

    #[inline]
    pub fn post_bytes_exist(&self) -> bool {
        self.post_bytes.is_some()
    }
}

/// Read the snapshots independently. An absent or differently owned snapshot
/// cannot be interpreted as this account type even if the other snapshot matches.
pub(crate) fn make_accounts_typed<'a, T>(
    acct_iter: impl Iterator<Item = AccountDatumIterItem<'a>> + 'a,
) -> Result<Vec<AccountTypedPrePost<'a, T>>>
where
    T: Discriminatable + RefFromByteRef + 'a,
{
    let mut typed = TypedAccounts::<T>::new();
    for account in acct_iter {
        let bytes = AccountSnapshots::read(&account, &[&typed.owner_program])?;
        typed.push_matching(&account, &bytes);
    }
    Ok(typed.accounts)
}

#[allow(clippy::type_complexity)]
pub(crate) fn make_accounts_typed_2<'a, T1, T2>(
    acct_iter: impl Iterator<Item = AccountDatumIterItem<'a>> + 'a,
) -> Result<(
    Vec<AccountTypedPrePost<'a, T1>>,
    Vec<AccountTypedPrePost<'a, T2>>,
)>
where
    T1: Discriminatable + RefFromByteRef + 'a,
    T2: Discriminatable + RefFromByteRef + 'a,
{
    let mut first = TypedAccounts::<T1>::new();
    let mut second = TypedAccounts::<T2>::new();
    for account in acct_iter {
        let bytes =
            AccountSnapshots::read(&account, &[&first.owner_program, &second.owner_program])?;
        if !first.push_matching(&account, &bytes) {
            second.push_matching(&account, &bytes);
        }
    }
    Ok((first.accounts, second.accounts))
}

#[allow(clippy::type_complexity)]
pub(crate) fn make_accounts_typed_3<'a, T1, T2, T3>(
    acct_iter: impl Iterator<Item = AccountDatumIterItem<'a>> + 'a,
) -> Result<(
    Vec<AccountTypedPrePost<'a, T1>>,
    Vec<AccountTypedPrePost<'a, T2>>,
    Vec<AccountTypedPrePost<'a, T3>>,
)>
where
    T1: Discriminatable + RefFromByteRef + 'a,
    T2: Discriminatable + RefFromByteRef + 'a,
    T3: Discriminatable + RefFromByteRef + 'a,
{
    let mut first = TypedAccounts::<T1>::new();
    let mut second = TypedAccounts::<T2>::new();
    let mut third = TypedAccounts::<T3>::new();
    for account in acct_iter {
        let bytes = AccountSnapshots::read(
            &account,
            &[
                &first.owner_program,
                &second.owner_program,
                &third.owner_program,
            ],
        )?;
        if !first.push_matching(&account, &bytes) && !second.push_matching(&account, &bytes) {
            third.push_matching(&account, &bytes);
        }
    }
    Ok((first.accounts, second.accounts, third.accounts))
}

struct AccountSnapshots {
    pre: Option<Vec<u8>>,
    post: Option<Vec<u8>>,
}

impl AccountSnapshots {
    fn read(account: &AccountDatumIterItem<'_>, owners: &[&str]) -> Result<Self> {
        let pre = match account.pre_owner_program {
            Some(owner) if owners.contains(&owner) => account.unpack_pre_datum()?,
            _ => None,
        };
        let post = match account.post_owner_program {
            Some(owner) if owners.contains(&owner) => account.unpack_post_datum()?,
            _ => None,
        };
        Ok(Self { pre, post })
    }
}

struct TypedAccounts<'a, T: RefFromByteRef> {
    owner_program: String,
    discriminator: &'static [u8],
    accounts: Vec<AccountTypedPrePost<'a, T>>,
}

impl<'a, T: Discriminatable + RefFromByteRef> TypedAccounts<'a, T> {
    fn new() -> Self {
        let (owner_program, discriminator) = T::discriminator();
        Self {
            owner_program: owner_program.to_string(),
            discriminator,
            accounts: Vec::new(),
        }
    }

    fn matching_body<'b>(&self, owner: Option<&str>, bytes: Option<&'b [u8]>) -> Option<&'b [u8]> {
        if owner != Some(self.owner_program.as_str()) {
            return None;
        }
        let bytes = bytes.filter(|bytes| !bytes.is_empty())?;
        bytes.strip_prefix(self.discriminator)
    }

    fn push_matching(
        &mut self,
        account: &AccountDatumIterItem<'a>,
        bytes: &AccountSnapshots,
    ) -> bool {
        let pre = self.matching_body(account.pre_owner_program, bytes.pre.as_deref());
        let post = self.matching_body(account.post_owner_program, bytes.post.as_deref());
        if pre.is_none() && post.is_none() {
            return false;
        }

        let was_created = (account.was_created || bytes.pre.as_ref().is_some_and(Vec::is_empty))
            && post.is_some();
        self.accounts.push(AccountTypedPrePost::new(
            account.pubkey,
            pre != post,
            was_created,
            pre.map(<[u8]>::to_vec),
            post.map(<[u8]>::to_vec),
        ));
        true
    }
}

#[cfg(test)]
mod tests {
    use base64::prelude::*;
    use solana_sdk::pubkey::Pubkey;

    use super::*;

    fn owner() -> Pubkey {
        Pubkey::new_from_array([1; 32])
    }

    macro_rules! account_type {
        ($name:ident, $disc:expr) => {
            #[repr(transparent)]
            #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
            struct $name(u8);

            impl Discriminatable for $name {
                fn discriminator() -> (Pubkey, &'static [u8]) {
                    (owner(), $disc)
                }
            }
        };
    }

    account_type!(First, b"first___");
    account_type!(Second, b"second__");
    account_type!(Third, b"third___");

    fn datum<T: Discriminatable>(value: u8) -> String {
        let (_, discriminator) = T::discriminator();
        let mut data = discriminator.to_vec();
        data.push(value);
        BASE64_STANDARD.encode(zstd::encode_all(&data[..], 0).expect("encode fixture"))
    }

    #[test]
    fn opening_and_closing_ignore_empty_snapshots_in_all_readers() -> Result<()> {
        let program = owner().to_string();
        let first_data = datum::<First>(10);
        let second_data = datum::<Second>(20);
        let third_data = datum::<Third>(30);
        let item = |pre, post, was_created, was_closed| AccountDatumIterItem {
            pubkey: "fixture",
            pre_datum: Some(pre),
            post_datum: Some(post),
            pre_owner_program: Some(program.as_str()),
            post_owner_program: Some(program.as_str()),
            datum_changed: true,
            was_created,
            was_closed,
        };
        let opened =
            make_accounts_typed::<First>(std::iter::once(item("", &first_data, true, false)))?;
        assert!(!opened[0].pre_bytes_exist());
        assert!(opened[0].was_created);
        assert_eq!(opened[0].get_post()?.expect("opened account").0, 10);

        let (first, closed) = make_accounts_typed_2::<First, Second>(std::iter::once(item(
            &second_data,
            "",
            false,
            true,
        )))?;
        assert!(first.is_empty());
        assert!(!closed[0].post_bytes_exist());
        assert_eq!(closed[0].get_pre()?.expect("closed account").0, 20);

        let (first, second, opened) = make_accounts_typed_3::<First, Second, Third>(
            std::iter::once(item("", &third_data, true, false)),
        )?;
        assert!(first.is_empty() && second.is_empty());
        assert!(!opened[0].pre_bytes_exist());
        assert_eq!(opened[0].get_post()?.expect("opened account").0, 30);
        Ok(())
    }

    #[test]
    fn snapshots_must_match_their_own_owner_and_discriminator() -> Result<()> {
        let program = owner().to_string();
        let first_data = datum::<First>(10);
        let second_data = datum::<Second>(20);
        let mut item = AccountDatumIterItem {
            pubkey: "fixture",
            pre_datum: Some(&first_data),
            post_datum: Some(&second_data),
            pre_owner_program: Some(program.as_str()),
            post_owner_program: Some(program.as_str()),
            datum_changed: true,
            was_created: false,
            was_closed: false,
        };
        let first = make_accounts_typed::<First>(std::iter::once(item.clone()))?;
        assert!(first[0].pre_bytes_exist());
        assert!(!first[0].post_bytes_exist());

        item.post_datum = Some(&first_data);
        item.post_owner_program = Some("different-program");
        let first = make_accounts_typed::<First>(std::iter::once(item))?;
        assert!(first[0].pre_bytes_exist());
        assert!(!first[0].post_bytes_exist());
        assert!(first[0].is_changed);
        Ok(())
    }
}
