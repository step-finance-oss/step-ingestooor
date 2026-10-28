use anchor_lang::solana_program::pubkey::Pubkey;
pub static ID: Pubkey = Pubkey::new_from_array([0u8; 32]);

pub mod defi_tuna_gen;
pub mod kamino_lend_gen;
pub mod marginfi_gen;
pub mod meteora_amm_gen;
pub mod meteora_dlmm_gen;
pub mod meteora_farms_gen;
pub mod nx_lending_gen;
pub mod orca_whirlies_gen;
pub mod raydium_clmm_gen;

pub mod jup_v6_gen {
    use anchor_lang::declare_program;
    declare_program!(jup_v6);
}

pub mod jup_dca_gen {
    anchor_lang::declare_program!(jup_dca);
}

pub mod step_staking_gen {
    anchor_lang::declare_program!(step_staking);
}

pub mod solayer_avs_gen {
    anchor_lang::declare_program!(solayer_avs);
}

pub mod solayer_restake_gen {
    anchor_lang::declare_program!(solayer_restake);
}

pub mod tr_referrals_gen {
    anchor_lang::declare_program!(tr_referrals);
}

pub mod kamino_vaults_gen {
    anchor_lang::declare_program!(kamino_vaults);
}

pub mod atrix_v1_gen {
    anchor_lang::declare_program!(atrix_v1);
}

pub mod openbook_v2_gen {
    anchor_lang::declare_program!(openbook_v2);
}

pub mod meteora_vaults_gen {
    anchor_lang::declare_program!(meteora_vaults);
}

pub mod raydium_v6_gen {
    anchor_lang::declare_program!(raydium_v6);
}

pub mod marinade_finance_gen {
    anchor_lang::declare_program!(marinade_finance);
}

pub mod drift_gen {
    anchor_lang::declare_program!(drift);
}

pub mod reward_options {
    anchor_lang::declare_program!(reward_options);
}

pub mod flash_trade_gen {
    anchor_lang::declare_program!(flash_trade);
}

pub mod hubble_lending_gen {
    anchor_lang::declare_program!(hubble_lending);
}

pub mod jupiter_staking_gen {
    use anchor_lang::declare_program;
    declare_program!(jupiter_staking);
}

pub mod kamino_farms_gen {
    anchor_lang::declare_program!(kamino_farms);
}

pub mod jupiter_perp_gen {
    use anchor_lang::declare_program;
    declare_program!(jup_perps);
}
pub mod nx_leverage_gen {
    use anchor_lang::declare_program;
    declare_program!(nx_leverage);
}

pub mod lulo_gen {
    anchor_lang::declare_program!(lulo);
}

pub mod titan_gen {
    anchor_lang::declare_program!(titan);
}

pub mod dflow_gen {
    anchor_lang::declare_program!(dflow);
}
