use anyhow::{Context, Result};
use clickhouse::{inserter::Inserter, Client, Row};
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;
use step_ingestooor_sdk::dooot::*;

const FLUSH_MS: u64 = 5000;
const MAX_ROWS: u64 = 5000;

// Master macro that generates all the code related to dooot types
macro_rules! define_ch_writer_lite {
    ( $( ($variant:ident, $type:ty, $field:ident) ),* ) => {
        // Generate the struct for all the inserters
        #[derive(Default)]
        pub struct CHWriteLiteInserters {
            $(
                pub $field: LazyInserter<$type>,
            )*
        }

        impl CHWriterLite {
            #[inline]
            pub fn new(client: Client) -> Self {
                Self {
                    client,
                    inserters: CHWriteLiteInserters::default(),
                }
            }

            /// Insert dooots from an array
            #[inline]
            pub async fn write_doots(&mut self, dooots: &[Dooot]) -> Result<()> {
                for dooot in dooots {
                    //get or create the inserter
                    match dooot {
                        $(
                            Dooot::$variant(d) => {
                                self.inserters.$field.write(&self.client, &d)?;
                            },
                        )*
                    }
                }
                Ok(())
            }

            // Generate flush and end methods
            #[inline]
            pub async fn flush(&mut self) -> Result<()> {
                $(
                    self.inserters.$field.flush().await?;
                )*
                Ok(())
            }

            #[inline]
            pub async fn end(&mut self) -> Result<()> {
                $(
                    self.inserters.$field.end().await?;
                )*
                Ok(())
            }
        }
    };
}

// A single list of all the dooot types
define_ch_writer_lite! {
    (AccountMetadata, AccountInfoDooot, i_account_info),
    (ClmmGlobal, ClmmGlobalDooot, i_clmm_global),
    (ClmmUser, ClmmUserDooot, i_clmm_user),
    (ClmmTickGlobal, ClmmTickGlobalDooot, i_clmm_tick_global),
    (ClobFillEvent, ClobFillEventDooot, i_clob_fill_event),
    (OraclePriceEvent, OraclePriceEventDooot, i_oracle_price_event),
    (DlmmGlobal, DlmmGlobalDooot, i_dlmm_global),
    (DlmmUser, DlmmUserDooot, i_dlmm_user),
    (FarmInfo, FarmInfoDooot, i_farm_info),
    (FarmGlobal, FarmGlobalDooot, i_farm_global),
    (FarmUser, FarmUserDooot, i_farm_user),
    (LendingGlobal, LendingGlobalDooot, i_lending_global),
    (LendingUser, LendingUserDooot, i_lending_user),
    (RepeatingOrderUser, RepeatingOrderUserDooot, i_repeating_order_user),
    (LiquidityChangeEvent, LiquidityChangeEventDooot, i_liquidity_change_event),
    (LPInfo, LPInfoDooot, i_lp_info),
    (MintInfo, MintInfoDooot, i_mint_info),
    (MintUnderlyingsGlobal, MintUnderlyingsGlobalDooot, i_mint_underlyings_global),
    (NativeStakeEvent, NativeStakeEventDooot, i_native_stake_event),
    (NftEvent, NftEventDooot, i_nft_event),
    (ClobUser, ClobUserDooot, i_clob_user),
    (SlotStats, SlotStatsGlobalDooot, i_slot_stats),
    (SolBalanceUser, SolBalanceUserDooot, i_sol_balance_user),
    (SolTransferEvent, SolTransferEventDooot, i_sol_transfer_event),
    (TokenTransferEvent, TokenTransferEventDooot, i_token_transfer_event),
    (TokenBalanceUser, TokenBalanceUserDooot, i_token_balance_user),
    (MintSupplyGlobal, MintSupplyGlobalDooot, i_mint_supply_global),
    (SwapEvent, SwapEventDooot, i_swap_event),
    (TokenMissingOwner, TokenMissingOwnerGlobalDooot, i_token_missing_owner),
    (ClaimableBalanceUser, ClaimableBalanceUserDooot, i_claimable_balance_user),
    (TokenPriceGlobal, TokenPriceGlobalDooot, i_token_price_global),
    (ManagedAccountUser, ManagedAccountUserDooot, i_managed_account_user),
    (RewardOptionUser, RewardOptionUserDooot, i_reward_option_user),
    (VaultInfo, VaultInfoDooot, i_vault_info),
    (YieldBearingGlobal, YieldBearingGlobalDooot, i_yield_bearing_global),
    (SubscriptionUser, SubscriptionUserDooot, i_subscription_purchase_user),
    (SubscriptionPurchaseEvent, SubscriptionPurchaseEventDooot, i_subscription_event),
    (PsDefiTunaFusionPool, PsDefiTunaFusionPoolDooot, i_defituna_fusion_pool_global),
    (PsDefiTunaLendingPosition, PsDefiTunaLendingPositionDooot, i_defituna_lending_position_user),
    (PsDefiTunaMarket, PsDefiTunaMarketDooot, i_defituna_market),
    (PsDefiTunaTunaPosition, PsDefiTunaTunaPositionDooot, i_defituna_tuna_position_user),
    (PsDefiTunaVault, PsDefiTunaVaultDooot, i_defituna_vault_global),
    (PsNxLendPosition, PsNxLendPositionDooot, i_nx_lend_position),
    (PsNxLeverageMarginPool, PsNxLeverageMarginPoolDooot, i_nx_leverage_margin_pool),
    (PsNxLendVSolPool, PsNxLendVSolPoolDooot, i_nx_lend_vsol_pool),
    (PsNxLendVSolPosition, PsNxLendVSolPositionDooot, i_nx_lend_vsol_position),
    (PsNxLendLendingPool, PsNxLendLendingPoolDooot, i_nx_lend_lending_pool),
    (PsNxLendLendingAccount, PsNxLendLendingAccountDooot, i_nx_lend_lending_account),
    (PsNxLeverageMarginAccount, PsNxLeverageMarginAccountDooot, i_nx_leverage_margin_account),
    (PsNxLeverageJupiterAccount, PsNxLeverageJupiterAccountDooot, i_nx_leverage_jupiter_account),
    (PsNxLendSolayerPool, PsNxLendSolayerPoolDooot, i_nx_lend_solayer_pool),
    (PsNxLendFragmetricPool, PsNxLendFragmetricPoolDooot, i_nx_lend_fragmetric_pool),
    (PsNxLendFragmetricUser, PsNxLendFragmetricUserDooot, i_nx_lend_fragmetric_user)
}

pub struct CHWriterLite {
    client: Client,
    inserters: CHWriteLiteInserters,
}

pub struct LazyInserter<T> {
    inserter: Option<Inserter<T>>,
}

impl<T> Default for LazyInserter<T>
where
    T: Row + ClickhouseDoootTrait + Serialize + DeserializeOwned + Send + 'static,
{
    fn default() -> Self {
        Self { inserter: None }
    }
}

impl<T> LazyInserter<T>
where
    T: Row + ClickhouseDoootTrait + Serialize + DeserializeOwned + Send + 'static,
{
    #[inline]
    pub fn write(&mut self, client: &Client, dooot: &T) -> Result<()> {
        match &mut self.inserter {
            Some(inserter) => inserter.write(dooot).map_err(Into::into),
            None => {
                //not created, so create it
                let mut i = self.create_inserter(client)?;
                i.write(dooot)?;
                self.inserter = Some(i);
                Ok(())
            }
        }
    }

    #[inline]
    pub async fn flush(&mut self) -> Result<()> {
        match &mut self.inserter {
            Some(inserter) => inserter.commit().await.map_err(Into::into).map(|_| ()),
            None => Ok(()),
        }
    }

    #[inline]
    pub async fn end(&mut self) -> Result<()> {
        if let Some(inserter) = self.inserter.take() {
            inserter.end().await.map_err(Into::into).map(|_| ())
        } else {
            Ok(())
        }
    }

    /// Create an inserter for a specific dooot type
    #[inline]
    pub fn create_inserter(&self, client: &Client) -> Result<Inserter<T>>
    where
        T: Row + ClickhouseDoootTrait + Serialize + DeserializeOwned + Send + 'static,
    {
        let def = T::default();
        let table_name = def.get_table_name().context("table name not found")?;
        let dooot_name = def.get_dooot_name();

        log::info!(
            "Creating inserter for dooot {} with table {}",
            dooot_name,
            table_name
        );

        let inserter = client
            .inserter(table_name)?
            .with_max_rows(MAX_ROWS)
            .with_period(Some(Duration::from_millis(FLUSH_MS)));

        Ok(inserter)
    }
}
