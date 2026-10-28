use std::hash::Hash;
use std::sync::mpsc::SyncSender;

use crate::amm::clmm::orca::OrcaWhirlies;
use crate::amm::clmm::raydium::RaydiumCLMM;
use crate::amm::dlmm::meteora::MeteoraDLMM;
use crate::amm::pump_dot_fun::PumpFunAMM;
use crate::amm::raydium_v4::RaydiumAmmV4;
use crate::amm::raydium_v5::RaydiumV5;
use crate::amm::raydium_v6::RaydiumV6;
use crate::amm::saber::SaberPools;
use crate::amm::saros::Saros;
use crate::managed::lulo::Lulo;
use anyhow::Result;
use enum_dispatch::enum_dispatch;
use solana_sdk::account::{AccountSharedData, ReadableAccount};
use solana_sdk::pubkey::Pubkey;
use step_ingestooor_sdk::dooot::Dooot;

use crate::dex::jupiter::JupiterLimitOrderV2;
use crate::dex::openbook_v2::parser::OpenbookV2;
use crate::lending::hubble::parser::HubbleLend;
use crate::lending::kamino::KaminoLend;
use crate::lending::marginfi::Marginfi;
use crate::lending::nx::Nx;
use crate::lending::solend::parser::Solend;
use crate::mint_metadata::metaplex::token_metadata_program::MetaplexTokenMetadata;
use crate::misc::tr_referrals::TeamRaccoonsReferrals;
use crate::repeating_order::JupiterDCA;
use crate::spl::tokens::TokenTransfer;
use crate::spl::yield_bearing::jlp::JLP;
use crate::staking::flash::FlashTrade;
use crate::staking::fragmetric_restake::FragmetricRestake;
use crate::staking::stake_pool::StakePool;
use crate::step::subscriptions::StepSubscriptions;
use crate::system::sol_balance::SolBalance;
use crate::{
    AtrixAmmV1, DriftFarms, KaminoFarms, MeteoraAMM, MeteoraFarms, MeteoraVaults, RaydiumFarmsV3,
    RaydiumFarmsV5, RaydiumFarmsV6,
};

pub struct StateSourceParserManager;

impl StateSourceParserManager {
    fn all_parsers() -> Vec<StateSourcingParser> {
        // Add all parsers that support StateSource here
        vec![
            StateSourcingParser::from(TeamRaccoonsReferrals),
            StateSourcingParser::from(MetaplexTokenMetadata),
            StateSourcingParser::from(OpenbookV2),
            StateSourcingParser::from(RaydiumV5),
            StateSourcingParser::from(RaydiumV6),
            StateSourcingParser::from(Marginfi),
            StateSourcingParser::from(StakePool),
            StateSourcingParser::from(Marginfi),
            StateSourcingParser::from(SolBalance),
            StateSourcingParser::from(MeteoraAMM),
            StateSourcingParser::from(MeteoraVaults),
            StateSourcingParser::from(OrcaWhirlies),
            StateSourcingParser::from(RaydiumCLMM),
            StateSourcingParser::from(MeteoraFarms),
            StateSourcingParser::from(KaminoLend),
            StateSourcingParser::from(HubbleLend),
            StateSourcingParser::from(TokenTransfer),
            StateSourcingParser::from(MeteoraDLMM),
            StateSourcingParser::from(SaberPools),
            StateSourcingParser::from(Lulo),
            StateSourcingParser::from(AtrixAmmV1),
            StateSourcingParser::from(RaydiumAmmV4),
            StateSourcingParser::from(Saros),
            StateSourcingParser::from(Solend),
            StateSourcingParser::from(RaydiumFarmsV3),
            StateSourcingParser::from(RaydiumFarmsV5),
            StateSourcingParser::from(RaydiumFarmsV6),
            StateSourcingParser::from(DriftFarms),
            StateSourcingParser::from(JupiterDCA),
            StateSourcingParser::from(FlashTrade),
            StateSourcingParser::from(FragmetricRestake),
            StateSourcingParser::from(KaminoFarms),
            StateSourcingParser::from(JLP),
            StateSourcingParser::from(StepSubscriptions),
            StateSourcingParser::from(PumpFunAMM),
            StateSourcingParser::from(JupiterLimitOrderV2),
            StateSourcingParser::from(Nx),
        ]
    }

    pub fn get_parser_by_name(name: &str) -> Option<StateSourcingParser> {
        Self::all_parsers()
            .into_iter()
            .find(|p| <StateSourcingParser>::get_name(p) == name)
    }
}

#[enum_dispatch]
pub trait StateSource {
    fn get_state(
        &self,
        config: Option<String>,
        acct_provider: impl AccountProvider,
        tx: SyncSender<Dooot>,
    ) -> Result<()>;

    fn get_name(&self) -> &'static str;
}

#[enum_dispatch(StateSource)]
pub enum StateSourcingParser {
    MetaplexTokenMetadata,
    TeamRaccoonsReferrals,
    Marginfi,
    RaydiumV5,
    RaydiumV6,
    StakePool,
    SolBalance,
    MeteoraAMM,
    MeteoraVaults,
    OrcaWhirlies,
    MeteoraFarms,
    KaminoLend,
    HubbleLend,
    TokenTransfer,
    OpenbookV2,
    RaydiumCLMM,
    MeteoraDLMM,
    SaberPools,
    Lulo,
    AtrixAmmV1,
    RaydiumAmmV4,
    Solend,
    Saros,
    RaydiumFarmsV3,
    RaydiumFarmsV5,
    RaydiumFarmsV6,
    DriftFarms,
    JupiterDCA,
    FlashTrade,
    FragmetricRestake,
    KaminoFarms,
    StepSubscriptions,
    PumpFunAMM,
    JLP,
    JupiterLimitOrderV2,
    Nx,
}
pub struct AccountProviderAccount {
    pub pubkey: Pubkey,
    pub account: AccountSharedData,
}

impl Hash for AccountProviderAccount {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.pubkey.hash(state);
    }
}

impl Eq for AccountProviderAccount {}

impl PartialEq for AccountProviderAccount {
    fn eq(&self, other: &Self) -> bool {
        self.pubkey == other.pubkey
    }
}

pub trait AccountProvider {
    fn get_block_time(&self) -> i64;
    fn all_accounts(&self) -> impl Iterator<Item = AccountProviderAccount>;
    fn program_accounts(
        &self,
        program_id: Pubkey,
        discriminator: Option<&[u8]>,
    ) -> impl Iterator<Item = AccountProviderAccount> {
        let disc_len = discriminator.map_or(0, |a| a.len());
        self.all_accounts().filter(move |a| {
            let data = &a.account.data();
            let owner_matches = a.account.owner() == &program_id;
            let data_len_enough = data.len() >= disc_len;
            let discrim_matches = discriminator.is_none()
                || discriminator.is_some_and(|disc| data_len_enough && &data[0..disc_len] == disc);

            owner_matches && discrim_matches
        })
    }
}
