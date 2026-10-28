pub mod amm;
pub mod dex;
pub mod domains;
pub mod farms;
pub mod lending;
pub mod managed;
pub mod margin;
pub mod mint_metadata;
pub mod misc;
pub mod oracles;
pub mod repeating_order;
pub mod spl;
pub mod staking;
pub mod state_source;
pub mod step;
pub mod swaps;
pub mod system;
pub mod vaults;

pub use amm::atrix_v1::AtrixAmmV1;
pub use amm::clmm::orca::OrcaWhirlies;
pub use amm::clmm::raydium::RaydiumCLMM;
pub use amm::dlmm::meteora::MeteoraDLMM;
pub use amm::meteora_amm::MeteoraAMM;
pub use amm::pump_dot_fun::PumpFunAMM;
pub use amm::raydium_v4::RaydiumAmmV4;
pub use amm::raydium_v5::RaydiumV5;
pub use amm::raydium_v6::RaydiumV6;
pub use amm::saber::SaberPools;
pub use amm::saros::Saros;
use anyhow::Result;
use async_trait::async_trait;
pub use dex::jupiter::JupiterLimitOrderV2;
pub use dex::manifest::Manifest;
pub use dex::openbook::parser::Openbook;
pub use dex::openbook_v2::parser::OpenbookV2;
pub use dex::phoenix::parser::PhoenixV1;
pub use domains::ad::AllDomainsParser;
use enum_dispatch::enum_dispatch;
pub use farms::drift::DriftFarms;
pub use farms::kamino_farms::KaminoFarms;
pub use farms::meteora_farms::MeteoraFarms;
pub use farms::raydium_v3::RaydiumFarmsV3;
pub use farms::raydium_v5::RaydiumFarmsV5;
pub use farms::raydium_v6::RaydiumFarmsV6;
pub use lending::hubble::parser::HubbleLend;
pub use lending::kamino::KaminoLend;
pub use lending::klend_vaults::KLendVaults;
pub use lending::marginfi::Marginfi;
pub use lending::nx::Nx;
pub use lending::solend::parser::Solend;
pub use managed::lulo::Lulo;
pub use margin::defi_tuna::DefiTuna;
pub use mint_metadata::metaplex::token_metadata_program::MetaplexTokenMetadata;
pub use misc::tr_referrals::TeamRaccoonsReferrals;
pub use oracles::pyth::PythReader;
pub use repeating_order::JupiterDCA;
pub use spl::tokens::TokenTransfer;
pub use spl::yield_bearing::jlp::JLP;
pub use staking::flash::FlashTrade;
pub use staking::fragmetric_restake::FragmetricRestake;
pub use staking::jupiter::JupiterStaking;
pub use staking::marinade::MarinadeFinance;
pub use staking::solayer::solayer_avs::SolayerAvs;
pub use staking::solayer::solayer_restake::SolayerRestake;
pub use staking::stake_pool::StakePool;
pub use staking::xstep::XStepStaking;
pub use step::reward_options::RewardOptions;
pub use step::subscriptions::StepSubscriptions;
use step_ingestooor_sdk::dooot::Dooot;
use step_ingestooor_sdk::trait_defs::transaction::TransactionLike;
use step_ingestooor_sdk::utils::step_utils::StepUtils;
pub use swaps::dflow::DFlowSwaps;
pub use swaps::jupiter::JupV6;
pub use swaps::okx::OkxSwap;
pub use swaps::titan::TitanSwaps;
pub use swaps::xstep_stake_unstake::StepStakingSwap;
pub use system::native_staking::NativeStaking;
pub use system::sol_balance::SolBalance;
pub use system::sol_transfer::SolTransfer;
pub use vaults::carrot::parser::CarrotVault;
pub use vaults::kamino::KaminoVaults;
pub use vaults::meteora_vaults::MeteoraVaults;

#[derive(Clone)]
pub enum TransactionParserFilter {
    ProgramId(String),
    AccountId(String),
    None,
}

#[async_trait]
#[enum_dispatch]
pub trait TransactionParserTrait {
    fn get_name(&self) -> &'static str;
    fn get_filters(&self) -> Vec<TransactionParserFilter>;
    /// Given a [DecodedTransaction], this function should parse the inner instructions and
    /// append any [Dooot] to the input `dooot` vector.
    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        step_utils: &StepUtils,
    ) -> Result<()>;
}

#[enum_dispatch(TransactionParserTrait)]
pub enum TransactionParser {
    NativeStaking,
    TokenTransfer,
    JupV6,
    PhoenixV1,
    SolTransfer,
    Lulo,
    Manifest,
    PythReader,
    XStepStaking,
    SolayerAvs,
    SolayerRestake,
    StepStakingSwap,
    Openbook,
    RaydiumAmmV4,
    MeteoraAMM,
    AllDomainsParser,
    RaydiumV5,
    RaydiumV6,
    MeteoraVaults,
    MeteoraFarms,
    MetaplexTokenMetadata,
    TeamRaccoonsReferrals,
    MarinadeFinance,
    KaminoVaults,
    OpenbookV2,
    Marginfi,
    SolBalance,
    StakePool,
    OrcaWhirlies,
    KaminoLend,
    KLendVaults,
    HubbleLend,
    MeteoraDLMM,
    RaydiumCLMM,
    SaberPools,
    AtrixAmmV1,
    DefiTuna,
    Saros,
    Solend,
    RaydiumFarmsV3,
    RaydiumFarmsV5,
    RaydiumFarmsV6,
    DriftFarms,
    OkxSwap,
    RewardOptions,
    JupiterDCA,
    JupiterStaking,
    FlashTrade,
    FragmetricRestake,
    CarrotVault,
    KaminoFarms,
    JLP,
    StepSubscriptions,
    PumpFunAMM,
    JupiterLimitOrderV2,
    Nx,
    TitanSwaps,
    DFlowSwaps,
}

impl Clone for TransactionParser {
    fn clone(&self) -> Self {
        match *self {
            TransactionParser::AtrixAmmV1(_) => TransactionParser::AtrixAmmV1(AtrixAmmV1 {}),
            TransactionParser::NativeStaking(_) => {
                TransactionParser::NativeStaking(NativeStaking {})
            }
            TransactionParser::TokenTransfer(_) => {
                TransactionParser::TokenTransfer(TokenTransfer {})
            }
            TransactionParser::JupV6(_) => TransactionParser::JupV6(JupV6 {}),
            TransactionParser::PhoenixV1(_) => TransactionParser::PhoenixV1(PhoenixV1 {}),
            TransactionParser::PythReader(_) => TransactionParser::PythReader(PythReader {}),
            TransactionParser::DefiTuna(_) => TransactionParser::DefiTuna(DefiTuna {}),
            TransactionParser::SolTransfer(_) => TransactionParser::SolTransfer(SolTransfer {}),
            TransactionParser::AllDomainsParser(_) => {
                TransactionParser::AllDomainsParser(AllDomainsParser {})
            }
            TransactionParser::Lulo(_) => TransactionParser::Lulo(Lulo {}),
            TransactionParser::Manifest(_) => TransactionParser::Manifest(Manifest {}),
            TransactionParser::XStepStaking(_) => TransactionParser::XStepStaking(XStepStaking {}),
            TransactionParser::SolayerAvs(_) => TransactionParser::SolayerAvs(SolayerAvs {}),
            TransactionParser::SolayerRestake(_) => {
                TransactionParser::SolayerRestake(SolayerRestake {})
            }
            TransactionParser::StepStakingSwap(_) => {
                TransactionParser::StepStakingSwap(StepStakingSwap {})
            }
            TransactionParser::Openbook(_) => TransactionParser::Openbook(Openbook {}),
            TransactionParser::RaydiumAmmV4(_) => TransactionParser::RaydiumAmmV4(RaydiumAmmV4 {}),
            TransactionParser::MeteoraFarms(_) => TransactionParser::MeteoraFarms(MeteoraFarms {}),
            TransactionParser::RaydiumV5(_) => TransactionParser::RaydiumV5(RaydiumV5 {}),
            TransactionParser::RaydiumV6(_) => TransactionParser::RaydiumV6(RaydiumV6 {}),
            TransactionParser::MetaplexTokenMetadata(_) => {
                TransactionParser::MetaplexTokenMetadata(MetaplexTokenMetadata {})
            }
            TransactionParser::TeamRaccoonsReferrals(_) => {
                TransactionParser::TeamRaccoonsReferrals(TeamRaccoonsReferrals {})
            }
            TransactionParser::MarinadeFinance(_) => {
                TransactionParser::MarinadeFinance(MarinadeFinance {})
            }
            TransactionParser::KaminoVaults(_) => TransactionParser::KaminoVaults(KaminoVaults {}),
            TransactionParser::MeteoraAMM(_) => TransactionParser::MeteoraAMM(MeteoraAMM {}),
            TransactionParser::MeteoraVaults(_) => {
                TransactionParser::MeteoraVaults(MeteoraVaults {})
            }
            TransactionParser::OpenbookV2(_) => TransactionParser::OpenbookV2(OpenbookV2 {}),
            TransactionParser::Marginfi(_) => TransactionParser::Marginfi(Marginfi {}),
            TransactionParser::SolBalance(_) => TransactionParser::SolBalance(SolBalance {}),
            TransactionParser::StakePool(_) => TransactionParser::StakePool(StakePool {}),
            TransactionParser::OrcaWhirlies(_) => TransactionParser::OrcaWhirlies(OrcaWhirlies {}),
            TransactionParser::MeteoraDLMM(_) => TransactionParser::MeteoraDLMM(MeteoraDLMM {}),
            TransactionParser::RaydiumCLMM(_) => TransactionParser::RaydiumCLMM(RaydiumCLMM {}),
            TransactionParser::SaberPools(_) => TransactionParser::SaberPools(SaberPools {}),
            TransactionParser::KaminoLend(_) => TransactionParser::KaminoLend(KaminoLend {}),
            TransactionParser::KLendVaults(_) => TransactionParser::KLendVaults(KLendVaults {}),
            TransactionParser::HubbleLend(_) => TransactionParser::HubbleLend(HubbleLend {}),
            TransactionParser::Saros(_) => TransactionParser::Saros(Saros {}),
            TransactionParser::Solend(_) => TransactionParser::Solend(Solend {}),
            TransactionParser::RaydiumFarmsV3(_) => {
                TransactionParser::RaydiumFarmsV3(RaydiumFarmsV3 {})
            }
            TransactionParser::RaydiumFarmsV5(_) => {
                TransactionParser::RaydiumFarmsV5(RaydiumFarmsV5 {})
            }
            TransactionParser::RaydiumFarmsV6(_) => {
                TransactionParser::RaydiumFarmsV6(RaydiumFarmsV6 {})
            }
            TransactionParser::DriftFarms(_) => TransactionParser::DriftFarms(DriftFarms {}),
            TransactionParser::OkxSwap(_) => TransactionParser::OkxSwap(OkxSwap {}),
            TransactionParser::RewardOptions(_) => {
                TransactionParser::RewardOptions(RewardOptions {})
            }
            TransactionParser::JupiterDCA(_) => TransactionParser::JupiterDCA(JupiterDCA {}),
            TransactionParser::JupiterStaking(_) => {
                TransactionParser::JupiterStaking(JupiterStaking {})
            }
            TransactionParser::FlashTrade(_) => TransactionParser::FlashTrade(FlashTrade {}),
            TransactionParser::FragmetricRestake(_) => {
                TransactionParser::FragmetricRestake(FragmetricRestake {})
            }
            TransactionParser::CarrotVault(_) => TransactionParser::CarrotVault(CarrotVault {}),
            TransactionParser::KaminoFarms(_) => TransactionParser::KaminoFarms(KaminoFarms {}),
            TransactionParser::JLP(_) => TransactionParser::JLP(JLP {}),
            TransactionParser::StepSubscriptions(_) => {
                TransactionParser::StepSubscriptions(StepSubscriptions {})
            }
            TransactionParser::PumpFunAMM(_) => TransactionParser::PumpFunAMM(PumpFunAMM {}),
            TransactionParser::JupiterLimitOrderV2(_) => {
                TransactionParser::JupiterLimitOrderV2(JupiterLimitOrderV2 {})
            }
            TransactionParser::Nx(_) => TransactionParser::Nx(Nx {}),
            TransactionParser::TitanSwaps(_) => TransactionParser::TitanSwaps(TitanSwaps {}),
            TransactionParser::DFlowSwaps(_) => TransactionParser::DFlowSwaps(DFlowSwaps {}),
        }
    }
}

pub fn construct_all_parsers() -> Vec<TransactionParser> {
    vec![
        TransactionParser::from(AtrixAmmV1),
        // TransactionParser::from(NativeStaking),
        TransactionParser::from(TeamRaccoonsReferrals),
        TransactionParser::from(TokenTransfer),
        // TransactionParser::from(PhoenixV1),
        TransactionParser::from(SolTransfer),
        TransactionParser::from(Lulo),
        // TransactionParser::from(AllDomainsParser),
        TransactionParser::from(PythReader),
        // TransactionParser::from(Manifest),
        TransactionParser::from(XStepStaking),
        TransactionParser::from(SolayerAvs),
        TransactionParser::from(SolayerRestake),
        // TransactionParser::from(Openbook),
        TransactionParser::from(RaydiumAmmV4),
        TransactionParser::from(MeteoraAMM),
        TransactionParser::from(RaydiumV5),
        TransactionParser::from(RaydiumV6),
        TransactionParser::from(MeteoraVaults),
        TransactionParser::from(MeteoraFarms),
        TransactionParser::from(MetaplexTokenMetadata),
        TransactionParser::from(JupV6),
        TransactionParser::from(StepStakingSwap),
        TransactionParser::from(MarinadeFinance),
        // TransactionParser::from(DefiTuna),
        // TransactionParser::from(KaminoVaults),
        // TransactionParser::from(OpenbookV2),
        TransactionParser::from(Marginfi),
        TransactionParser::from(SolBalance),
        TransactionParser::from(StakePool),
        TransactionParser::from(OrcaWhirlies),
        TransactionParser::from(RaydiumCLMM),
        TransactionParser::from(KaminoLend),
        TransactionParser::from(KLendVaults),
        TransactionParser::from(HubbleLend),
        TransactionParser::from(MeteoraDLMM),
        TransactionParser::from(SaberPools),
        // TransactionParser::from(Saros),
        // TransactionParser::from(Solend),
        TransactionParser::from(RaydiumFarmsV3),
        TransactionParser::from(RaydiumFarmsV5),
        TransactionParser::from(RaydiumFarmsV6),
        TransactionParser::from(DriftFarms),
        TransactionParser::from(OkxSwap),
        TransactionParser::from(RewardOptions),
        TransactionParser::from(JupiterDCA),
        // TransactionParser::from(JupiterStaking),
        TransactionParser::from(FlashTrade),
        TransactionParser::from(JupiterStaking),
        TransactionParser::from(FragmetricRestake),
        TransactionParser::from(CarrotVault),
        TransactionParser::from(KaminoFarms),
        TransactionParser::from(JLP),
        TransactionParser::from(StepSubscriptions),
        TransactionParser::from(PumpFunAMM),
        // TransactionParser::from(JupiterLimitOrderV2),
        // TransactionParser::from(Nx),
        TransactionParser::from(TitanSwaps),
        TransactionParser::from(DFlowSwaps),
    ]
}
