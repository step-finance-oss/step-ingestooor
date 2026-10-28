use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;
use step_ingestooor_idl_gens::hubble_lending_gen::hubble_lending::types::{
    MarketCollateralAmounts, UserCollateralAmounts,
};
use step_ingestooor_sdk::trait_defs::transaction::{AccountDatumIterItem, TransactionLike};

#[allow(clippy::type_complexity)]
pub fn get_accounts_data(
    txn: &impl TransactionLike,
) -> anyhow::Result<Vec<(AccountDatumIterItem, Option<Vec<u8>>, Option<Vec<u8>>)>> {
    let accounts = txn
        .iter_accounts()
        .map(|a| {
            let unpacked_pre_data = a.unpack_pre_datum().ok().flatten();
            let unpacked_post_data = a.unpack_post_datum().ok().flatten();
            Ok((a, unpacked_pre_data, unpacked_post_data))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(accounts)
}

pub fn get_discriminator_from_pre_or_post<'a>(
    data_pre: &'a Option<Vec<u8>>,
    data_post: &'a Option<Vec<u8>>,
) -> anyhow::Result<Option<&'a [u8]>> {
    let disc = data_pre
        .as_ref()
        .and_then(|d| d.get(0..8))
        .or(data_post.as_ref().and_then(|d| d.get(0..8)));

    Ok(disc)
}

pub fn user_collateral_to_amount_map(
    inactive_collateral: UserCollateralAmounts,
    deposited_collateral: UserCollateralAmounts,
    collateral_mints: [Pubkey; 500],
) -> anyhow::Result<HashMap<Pubkey, u64>> {
    let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112")?;
    let eth_mint = Pubkey::from_str("7vfCXTUXx5WJV5JADk17DUJ4ksgau7utNKj4b963voxs")?;
    let btc_mint = Pubkey::from_str("9n4nbM75f5Ui33ZbPYXn59EwSgE8CGsHtAeTH5YFeJ9E")?;
    let srm_mint = Pubkey::from_str("SRMuApVNdxXokk5GT7XD5cUUgXMBCoAz2LHeuAoKWRt")?;
    let ray_mint = Pubkey::from_str("4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R")?;
    let ftt_mint = Pubkey::from_str("AGFEad2et2ZJif9jaGpdMixQqvW5i81aBdvKe7PHNfz3")?;
    let msol_mint = Pubkey::from_str("mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So")?;

    let mut amount_map: HashMap<Pubkey, u64> = HashMap::new();

    amount_map.insert(sol_mint, inactive_collateral.sol + deposited_collateral.sol);
    amount_map.insert(eth_mint, inactive_collateral.eth + deposited_collateral.eth);
    amount_map.insert(btc_mint, inactive_collateral.btc + deposited_collateral.btc);
    amount_map.insert(srm_mint, inactive_collateral.srm + deposited_collateral.srm);
    amount_map.insert(ray_mint, inactive_collateral.ray + deposited_collateral.ray);
    amount_map.insert(ftt_mint, inactive_collateral.ftt + deposited_collateral.ftt);
    amount_map.insert(
        msol_mint,
        inactive_collateral.msol + deposited_collateral.msol,
    );

    // loop through inactive_collateral and match mint using token_id
    for collateral in inactive_collateral.extra_collaterals.iter() {
        let mint = collateral_mints[collateral.token_id as usize];
        let old_amount = amount_map.get(&mint).copied().unwrap_or(0);
        amount_map.insert(mint, old_amount + collateral.amount);
    }

    // loop through deposited_collateral and match mint using token_id
    for collateral in deposited_collateral.extra_collaterals.iter() {
        let mint = collateral_mints[collateral.token_id as usize];
        let old_amount = amount_map.get(&mint).copied().unwrap_or(0);
        amount_map.insert(mint, old_amount + collateral.amount);
    }

    Ok(amount_map)
}

pub fn market_collateral_to_amount_map(
    inactive_collateral: MarketCollateralAmounts,
    deposited_collateral: MarketCollateralAmounts,
    collateral_mints: [Pubkey; 500],
) -> anyhow::Result<HashMap<Pubkey, u64>> {
    let mut amount_map: HashMap<Pubkey, u64> = HashMap::new();

    // loop through deposited_collateral and match mint using token_id
    for (i, amount) in deposited_collateral.amounts.iter().enumerate() {
        let mint = collateral_mints[i];
        amount_map.insert(mint, *amount);
    }

    // loop through inactive_collateral and match mint using token_id
    for (i, amount) in inactive_collateral.amounts.iter().enumerate() {
        let mint = collateral_mints[i];
        let old_amount = amount_map.get(&mint).unwrap_or(&0);
        amount_map.insert(mint, old_amount + amount);
    }

    Ok(amount_map)
}
