use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

pub mod account_info;
pub mod clmm;
pub mod clob;
pub mod defi_tuna;
pub mod dlmm;
pub mod farms;
pub mod lending;
pub mod liquidity;
pub mod managed_account;
pub mod mint_metadata;
pub mod mint_underlyings;
pub mod native_stake;
pub mod nft_activity;
pub mod nx;
pub mod oracle_price;
pub mod repeating_order;
pub mod reward_options;
pub mod slot_stats;
pub mod sol_balance;
pub mod spl;
pub mod subscriptions;
pub mod swap;
pub mod token_missing_owner;
pub mod token_price;
pub mod vaults;
pub mod wallet_claimable_balance;
pub mod yield_bearing;

#[doc(inline)]
pub use account_info::*;
#[doc(inline)]
pub use clmm::*;
#[doc(inline)]
pub use clob::*;
#[doc(inline)]
pub use defi_tuna::*;
#[doc(inline)]
pub use dlmm::*;
#[doc(inline)]
pub use farms::*;
#[doc(inline)]
pub use lending::*;
#[doc(inline)]
pub use liquidity::*;
#[doc(inline)]
pub use managed_account::*;
#[doc(inline)]
pub use mint_metadata::*;
#[doc(inline)]
pub use mint_underlyings::*;
#[doc(inline)]
pub use native_stake::*;
#[doc(inline)]
pub use nft_activity::*;
#[doc(inline)]
pub use nx::*;
#[doc(inline)]
pub use oracle_price::*;
#[doc(inline)]
pub use repeating_order::*;
#[doc(inline)]
pub use reward_options::*;
#[doc(inline)]
pub use slot_stats::*;
#[doc(inline)]
pub use sol_balance::*;
#[doc(inline)]
pub use spl::*;
use strum_macros::EnumIter;
#[doc(inline)]
pub use subscriptions::*;
#[doc(inline)]
pub use swap::*;
#[doc(inline)]
pub use token_missing_owner::*;
#[doc(inline)]
pub use token_price::*;
use ts_rs::TS;
#[doc(inline)]
pub use vaults::*;
#[doc(inline)]
pub use wallet_claimable_balance::*;
#[doc(inline)]
pub use yield_bearing::*;

#[enum_dispatch]
pub trait ClickhouseDoootTrait: DoootTrait + Default {
    fn get_table_name(&self) -> Option<&'static str> {
        None
    }

    fn get_queue_name(&self) -> Option<&'static str>;

    fn get_flush_ms(&self) -> u64;

    fn get_inserter_max_rows(&self) -> u64;
}

#[enum_dispatch]
pub trait DoootTrait {
    /// The name of the dooot STRUCT, used for binding to the CH_EXCHANGE
    fn get_dooot_name(&self) -> &'static str;
    fn get_topics(&self) -> Vec<String>;
    fn get_expr_context(&self) -> evalexpr::HashMapContext;
}

#[enum_dispatch(DoootTrait, ClickhouseDoootTrait)]
#[derive(Serialize, Deserialize, Debug, Clone, EnumIter, TS)]
#[serde(tag = "t")]
pub enum Dooot {
    AccountMetadata(AccountInfoDooot),
    ClmmGlobal(ClmmGlobalDooot),
    ClmmUser(ClmmUserDooot),
    ClmmTickGlobal(ClmmTickGlobalDooot),
    ClobFillEvent(ClobFillEventDooot),
    OraclePriceEvent(OraclePriceEventDooot),
    DlmmGlobal(DlmmGlobalDooot),
    DlmmUser(DlmmUserDooot),
    FarmInfo(FarmInfoDooot),
    FarmGlobal(FarmGlobalDooot),
    FarmUser(FarmUserDooot),
    RepeatingOrderUser(RepeatingOrderUserDooot),
    LendingGlobal(LendingGlobalDooot),
    LendingUser(LendingUserDooot),
    LiquidityChangeEvent(LiquidityChangeEventDooot),
    LPInfo(LPInfoDooot),
    MintInfo(MintInfoDooot),
    MintUnderlyingsGlobal(MintUnderlyingsGlobalDooot),
    ManagedAccountUser(ManagedAccountUserDooot),
    NativeStakeEvent(NativeStakeEventDooot),
    NftEvent(NftEventDooot),
    ClobUser(ClobUserDooot),
    SlotStats(SlotStatsGlobalDooot),
    SolBalanceUser(SolBalanceUserDooot),
    SolTransferEvent(SolTransferEventDooot),
    TokenTransferEvent(TokenTransferEventDooot),
    TokenBalanceUser(TokenBalanceUserDooot),
    MintSupplyGlobal(MintSupplyGlobalDooot),
    SwapEvent(SwapEventDooot),
    TokenMissingOwner(TokenMissingOwnerGlobalDooot),
    ClaimableBalanceUser(ClaimableBalanceUserDooot),
    TokenPriceGlobal(TokenPriceGlobalDooot),
    RewardOptionUser(RewardOptionUserDooot),
    VaultInfo(VaultInfoDooot),
    SubscriptionUser(SubscriptionUserDooot),
    SubscriptionPurchaseEvent(SubscriptionPurchaseEventDooot),
    YieldBearingGlobal(YieldBearingGlobalDooot),
    PsDefiTunaFusionPool(PsDefiTunaFusionPoolDooot),
    PsDefiTunaLendingPosition(PsDefiTunaLendingPositionDooot),
    PsDefiTunaMarket(PsDefiTunaMarketDooot),
    PsDefiTunaTunaPosition(PsDefiTunaTunaPositionDooot),
    PsDefiTunaVault(PsDefiTunaVaultDooot),
    PsNxLendPosition(PsNxLendPositionDooot),
    PsNxLeverageMarginPool(PsNxLeverageMarginPoolDooot),
    PsNxLendVSolPool(PsNxLendVSolPoolDooot),
    PsNxLeverageMarginAccount(PsNxLeverageMarginAccountDooot),
    PsNxLendSolayerPool(PsNxLendSolayerPoolDooot),
    PsNxLeverageJupiterAccount(PsNxLeverageJupiterAccountDooot),
    PsNxLendLendingAccount(PsNxLendLendingAccountDooot),
    PsNxLendLendingPool(PsNxLendLendingPoolDooot),
    PsNxLendVSolPosition(PsNxLendVSolPositionDooot),
    PsNxLendFragmetricPool(PsNxLendFragmetricPoolDooot),
    PsNxLendFragmetricUser(PsNxLendFragmetricUserDooot),
}

/// Unused, but required for ClickhouseDoootTrait
impl Default for Dooot {
    fn default() -> Self {
        Dooot::NativeStakeEvent(NativeStakeEventDooot::default())
    }
}
