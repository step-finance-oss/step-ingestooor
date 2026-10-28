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

/// Turns an iterator of `AccountDatumIterItem` into an iterator of `AccountTypedPrePost<T>`.
/// This is a helper function to allow for easy iteration over accounts with typed data.
/// This function will filter out accounts that do not match the expected program id or do not have
/// the expected discriminator.
/// This is broken out into this function so it can be reused for tx and ix
pub(crate) fn make_accounts_typed<'a, T>(
    acct_iter: impl Iterator<Item = AccountDatumIterItem<'a>> + 'a,
) -> Result<Vec<AccountTypedPrePost<'a, T>>>
where
    T: Discriminatable + 'a,
    T: RefFromByteRef,
{
    let mut t = make_typed_helper::<T>();
    let owner_program = &t.0;
    let discriminator = &t.1;
    let new_func = &t.2;
    let acct_vec = &mut t.3;

    for a in acct_iter {
        if a.pre_datum.is_none() && a.post_datum.is_none() {
            continue;
        }

        //see if accounts are of interest
        if Some(owner_program.as_str()) != a.pre_owner_program
            && Some(owner_program.as_str()) != a.post_owner_program
        {
            continue;
        }

        //get bytes
        let pre_bytes = a.unpack_pre_datum()?;
        let post_bytes = a.unpack_post_datum()?;
        //if nothing, go to next account
        if pre_bytes.is_none() && post_bytes.is_none() {
            continue;
        }
        let has_pre = pre_bytes
            .as_ref()
            .map(|b| b.starts_with(discriminator))
            .unwrap_or(false);
        let has_post = post_bytes
            .as_ref()
            .map(|b| b.starts_with(discriminator))
            .unwrap_or(false);
        let was_created = (pre_bytes.is_none() || pre_bytes.as_ref().is_some_and(|b| b.is_empty()))
            && post_bytes.as_ref().is_some_and(|b| !b.is_empty());
        if has_pre || has_post {
            acct_vec.push(new_func(
                a.pubkey,
                pre_bytes != post_bytes,
                was_created,
                pre_bytes.map(|b| (*b)[discriminator.len()..].to_vec()),
                post_bytes.map(|b| (*b)[discriminator.len()..].to_vec()),
            ));
        }
    }

    Ok(t.3)
}

#[allow(clippy::type_complexity)]
pub(crate) fn make_accounts_typed_2<'a, T1, T2>(
    acct_iter: impl Iterator<Item = AccountDatumIterItem<'a>> + 'a,
) -> Result<(
    Vec<AccountTypedPrePost<'a, T1>>,
    Vec<AccountTypedPrePost<'a, T2>>,
)>
where
    T1: Discriminatable + 'a,
    T1: RefFromByteRef,
    T2: Discriminatable + 'a,
    T2: RefFromByteRef,
{
    let mut t1 = make_typed_helper::<T1>();
    let mut t2 = make_typed_helper::<T2>();

    let owner_program1 = &t1.0;
    let discriminator1 = &t1.1;
    let new_func1 = &t1.2;
    let acct_vec1 = &mut t1.3;

    let owner_program2 = &t2.0;
    let discriminator2 = &t2.1;
    let new_func2 = &t2.2;
    let acct_vec2 = &mut t2.3;

    for a in acct_iter {
        if a.pre_datum.is_none() && a.post_datum.is_none() {
            continue;
        }
        //see if accounts are of interest
        if Some(owner_program1.as_str()) != a.pre_owner_program
            && Some(owner_program2.as_str()) != a.pre_owner_program
            && Some(owner_program1.as_str()) != a.post_owner_program
            && Some(owner_program2.as_str()) != a.post_owner_program
        {
            continue;
        }
        //get bytes
        let pre_bytes = a.unpack_pre_datum()?;
        let post_bytes = a.unpack_post_datum()?;
        //if nothing, go to next account
        if pre_bytes.is_none() && post_bytes.is_none() {
            continue;
        }

        let has_pre = pre_bytes
            .as_ref()
            .map(|b| b.starts_with(discriminator1))
            .unwrap_or(false);
        let has_post = post_bytes
            .as_ref()
            .map(|b| b.starts_with(discriminator1))
            .unwrap_or(false);
        let was_created = (pre_bytes.is_none() || pre_bytes.as_ref().is_some_and(|b| b.is_empty()))
            && post_bytes.as_ref().is_some_and(|b| !b.is_empty());
        if has_pre || has_post {
            acct_vec1.push(new_func1(
                a.pubkey,
                pre_bytes != post_bytes,
                was_created,
                pre_bytes.map(|b| (*b)[discriminator1.len()..].to_vec()),
                post_bytes.map(|b| (*b)[discriminator1.len()..].to_vec()),
            ));
            continue;
        }

        let has_pre = pre_bytes
            .as_ref()
            .map(|b| b.starts_with(discriminator2))
            .unwrap_or(false);
        let has_post = post_bytes
            .as_ref()
            .map(|b| b.starts_with(discriminator2))
            .unwrap_or(false);
        let was_created = (pre_bytes.is_none() || pre_bytes.as_ref().is_some_and(|b| b.is_empty()))
            && post_bytes.as_ref().is_some_and(|b| !b.is_empty());
        if has_pre || has_post {
            acct_vec2.push(new_func2(
                a.pubkey,
                pre_bytes != post_bytes,
                was_created,
                pre_bytes.map(|b| (*b)[discriminator2.len()..].to_vec()),
                post_bytes.map(|b| (*b)[discriminator2.len()..].to_vec()),
            ));
            continue;
        }
    }

    Ok((t1.3, t2.3))
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
    T1: Discriminatable + 'a,
    T1: RefFromByteRef,
    T2: Discriminatable + 'a,
    T2: RefFromByteRef,
    T3: Discriminatable + 'a,
    T3: RefFromByteRef,
{
    let mut t1 = make_typed_helper::<T1>();
    let mut t2 = make_typed_helper::<T2>();
    let mut t3 = make_typed_helper::<T3>();

    let owner_program1 = &t1.0;
    let discriminator1 = &t1.1;
    let new_func1 = &t1.2;
    let acct_vec1 = &mut t1.3;

    let owner_program2 = &t2.0;
    let discriminator2 = &t2.1;
    let new_func2 = &t2.2;
    let acct_vec2 = &mut t2.3;

    let owner_program3 = &t3.0;
    let discriminator3 = &t3.1;
    let new_func3 = &t3.2;
    let acct_vec3 = &mut t3.3;

    for a in acct_iter {
        if a.pre_datum.is_none() && a.post_datum.is_none() {
            continue;
        }
        //see if accounts are of interest
        if Some(owner_program1.as_str()) != a.pre_owner_program
            && Some(owner_program2.as_str()) != a.pre_owner_program
            && Some(owner_program3.as_str()) != a.pre_owner_program
            && Some(owner_program1.as_str()) != a.post_owner_program
            && Some(owner_program2.as_str()) != a.post_owner_program
            && Some(owner_program3.as_str()) != a.post_owner_program
        {
            continue;
        }
        //get bytes
        let pre_bytes = a.unpack_pre_datum()?;
        let post_bytes = a.unpack_post_datum()?;
        //if nothing, go to next account
        if pre_bytes.is_none() && post_bytes.is_none() {
            continue;
        }

        let has_pre = pre_bytes
            .as_ref()
            .map(|b| b.starts_with(discriminator1))
            .unwrap_or(false);
        let has_post = post_bytes
            .as_ref()
            .map(|b| b.starts_with(discriminator1))
            .unwrap_or(false);
        let was_created = (pre_bytes.is_none() || pre_bytes.as_ref().is_some_and(|b| b.is_empty()))
            && post_bytes.as_ref().is_some_and(|b| !b.is_empty());
        if has_pre || has_post {
            acct_vec1.push(new_func1(
                a.pubkey,
                pre_bytes != post_bytes,
                was_created,
                pre_bytes.map(|b| (*b)[discriminator1.len()..].to_vec()),
                post_bytes.map(|b| (*b)[discriminator1.len()..].to_vec()),
            ));
            continue;
        }

        let has_pre = pre_bytes
            .as_ref()
            .map(|b| b.starts_with(discriminator2))
            .unwrap_or(false);
        let has_post = post_bytes
            .as_ref()
            .map(|b| b.starts_with(discriminator2))
            .unwrap_or(false);
        let was_created = (pre_bytes.is_none() || pre_bytes.as_ref().is_some_and(|b| b.is_empty()))
            && post_bytes.as_ref().is_some_and(|b| !b.is_empty());
        if has_pre || has_post {
            acct_vec2.push(new_func2(
                a.pubkey,
                pre_bytes != post_bytes,
                was_created,
                pre_bytes.map(|b| (*b)[discriminator2.len()..].to_vec()),
                post_bytes.map(|b| (*b)[discriminator2.len()..].to_vec()),
            ));
            continue;
        }

        let has_pre = pre_bytes
            .as_ref()
            .map(|b| b.starts_with(discriminator3))
            .unwrap_or(false);
        let has_post = post_bytes
            .as_ref()
            .map(|b| b.starts_with(discriminator3))
            .unwrap_or(false);
        let was_created = (pre_bytes.is_none() || pre_bytes.as_ref().is_some_and(|b| b.is_empty()))
            && post_bytes.as_ref().is_some_and(|b| !b.is_empty());
        if has_pre || has_post {
            acct_vec3.push(new_func3(
                a.pubkey,
                pre_bytes != post_bytes,
                was_created,
                pre_bytes.map(|b| (*b)[discriminator3.len()..].to_vec()),
                post_bytes.map(|b| (*b)[discriminator3.len()..].to_vec()),
            ));
            continue;
        }
    }

    Ok((t1.3, t2.3, t3.3))
}

//used to shorten the above variants
type TypedHelper<'a, T> = (
    String,
    &'a [u8],
    fn(&'a str, bool, bool, Option<Vec<u8>>, Option<Vec<u8>>) -> AccountTypedPrePost<'a, T>,
    Vec<AccountTypedPrePost<'a, T>>,
);
fn make_typed_helper<'a, T>() -> TypedHelper<'a, T>
where
    T: Discriminatable + 'a,
    T: RefFromByteRef,
{
    let (pgm, disc) = T::discriminator();
    (
        pgm.to_string(),
        disc,
        AccountTypedPrePost::<'a, T>::new,
        Vec::<AccountTypedPrePost<T>>::new(),
    )
}
