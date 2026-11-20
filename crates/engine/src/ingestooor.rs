use std::future::{self};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use super::parser_manager::ParserManager;
use anyhow::{Context, Result};
use futures_util::{stream::FuturesUnordered, Future, FutureExt, StreamExt};
use itertools::Itertools;
use lapin::message::Delivery;
use lapin::{
    acker::Acker,
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
        BasicQosOptions, ConfirmSelectOptions,
    },
    publisher_confirm::Confirmation,
    types::FieldTable,
    Consumer,
};
use log::{debug, info};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransactionWithStatusMeta,
};
use step_ingestooor_sdk::dooot::{
    AccountInfoDooot, ClaimableBalanceUserDooot, ClmmGlobalDooot, ClmmTickGlobalDooot,
    ClmmUserDooot, ClobFillEventDooot, ClobUserDooot, DlmmGlobalDooot, DlmmUserDooot, DoootTrait,
    FarmGlobalDooot, FarmInfoDooot, FarmUserDooot, LPInfoDooot, LendingGlobalDooot,
    LendingUserDooot, LiquidityChangeEventDooot, ManagedAccountUserDooot, MintInfoDooot,
    MintSupplyGlobalDooot, MintUnderlyingsGlobalDooot, NativeStakeEventDooot,
    OraclePriceEventDooot, PsDefiTunaFusionPoolDooot, PsDefiTunaLendingPositionDooot,
    PsDefiTunaMarketDooot, PsDefiTunaTunaPositionDooot, PsDefiTunaVaultDooot,
    PsNxLendFragmetricPoolDooot, PsNxLendFragmetricUserDooot, PsNxLendLendingAccountDooot,
    PsNxLendLendingPoolDooot, PsNxLendPositionDooot, PsNxLendSolayerPoolDooot,
    PsNxLendVSolPoolDooot, PsNxLendVSolPositionDooot, PsNxLeverageJupiterAccountDooot,
    PsNxLeverageMarginAccountDooot, PsNxLeverageMarginPoolDooot, RepeatingOrderUserDooot,
    RewardOptionUserDooot, SlotStatsGlobalDooot, SolBalanceUserDooot, SolTransferEventDooot,
    SubscriptionPurchaseEventDooot, SubscriptionUserDooot, SwapEventDooot, TokenBalanceUserDooot,
    TokenMissingOwnerGlobalDooot, TokenPriceGlobalDooot, TokenTransferEventDooot, VaultInfoDooot,
    YieldBearingGlobalDooot,
};
use step_ingestooor_sdk::trait_defs::transaction::TransactionLike;
use step_ingestooor_sdk::utils::util_fns::convert_blocktime_to_micros;
use step_ingestooor_sdk::{
    decoded_transaction::{standardized_decoded_transaction, DecodedTransaction},
    dooot::Dooot,
    utils::step_utils::StepUtils,
};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc::Receiver;

use crate::data_access::CHDataWriter;
use crate::rabbit_factory::UserRabbitQueueInfo;
use indexer_rabbitmq::geyser::{Message, TransactionNotify};
use step_ingestooor_parsers::TransactionParserTrait;

const SCHEMAS_EXTRA_SECS_AFTER_STOP: u64 = 5;

pub struct Ingestooor {
    rabbit: lapin::Connection,
    queue_info: UserRabbitQueueInfo,
    ch_client: Option<clickhouse::Client>,
    ch_period_bias_pct: f64,
    ch_max_flush_ms: u64,
    debug_user: Option<String>,
    prefetch: u16,
    step_utils: Rc<StepUtils>,
    term_signal_incoming: Arc<AtomicBool>,
    term_signal_dooot: Arc<AtomicBool>,
    term_signal_ch: Arc<AtomicBool>,
}

impl Ingestooor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rabbit: lapin::Connection,
        queue_info: UserRabbitQueueInfo,
        ch_client: Option<clickhouse::Client>,
        ch_period_bias_pct: f64,
        ch_max_flush_ms: u64,
        debug_user: Option<String>,
        prefetch: u16,
        step_utils: Rc<StepUtils>,
        term_signal_incoming: Arc<AtomicBool>,
        term_signal_dooot: Arc<AtomicBool>,
        term_signal_ch: Arc<AtomicBool>,
    ) -> Result<Ingestooor> {
        let ingestooor = Ingestooor {
            rabbit,
            queue_info,
            ch_client,
            ch_period_bias_pct,
            ch_max_flush_ms,
            debug_user,
            prefetch,
            step_utils,
            term_signal_incoming,
            term_signal_dooot,
            term_signal_ch,
        };

        Ok(ingestooor)
    }

    pub async fn run(&self, consume_parse: bool, write_ch: bool) -> Result<()> {
        debug!("running ingestooor with prefetch {}", self.prefetch);

        //these are futures that are actively waited on
        let mut futures: Vec<Pin<Box<dyn Future<Output = Result<()>>>>> = vec![];

        //used to shutdown rabbit, which ends consumer stream processing
        let (rabbit_close_tx, rabbit_close_rx) = tokio::sync::oneshot::channel::<()>();
        let rabbit_closer = async move {
            rabbit_close_rx.await.ok();
            log::warn!("signal to close rabbit received");
            let _ = self.rabbit.close(200, "Goodbye!").await;
            log::warn!("rabbit closed");
            Ok(())
        };
        futures.push(Box::pin(rabbit_closer));

        //this is our shutdown logic
        //we want aa clean shutdown to gracefully stop consuing from the incoming queue,
        //and to allow the dooot writer to finish writing any dooots it has in memory
        //then allow the db writer to finish writing any dooots in the rabbit queue.
        //we give it a fixed amount of time for that to happen, then we stop the db writer
        //and close the rabbit connection
        let term_signal_incoming_setter = self.term_signal_incoming.clone();
        let term_signal_dooot_setter = self.term_signal_dooot.clone();
        let term_signal_ch_setter = self.term_signal_ch.clone();
        let signal_task = tokio::spawn(
            #[allow(clippy::unwrap_used)]
            async move {
                // Listen for SIGTERM and SIGINT
                let mut term_signal = signal(SignalKind::terminate()).unwrap();
                let mut int_signal = signal(SignalKind::interrupt()).unwrap();

                //wait for either trigger
                tokio::select! {
                    _ = term_signal.recv() => {
                        info!("received SIGTERM");
                    }
                    _ = int_signal.recv() => {
                        info!("received SIGINT");
                    }
                }

                info!("stopping incoming parsing...");
                term_signal_incoming_setter.store(true, std::sync::atomic::Ordering::Relaxed);
                info!(
                    "waiting {SCHEMAS_EXTRA_SECS_AFTER_STOP} seconds to allow dooots to write..."
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    SCHEMAS_EXTRA_SECS_AFTER_STOP,
                ))
                .await;
                info!("stopping dooot writing...");
                //for whatever reason, this wont stop our dooot consumer task
                //leaving here to display intent
                term_signal_dooot_setter.store(true, std::sync::atomic::Ordering::Relaxed);
                term_signal_ch_setter.store(true, std::sync::atomic::Ordering::Relaxed);
                //the dooot writer task will wait for the rabbit to close before stopping
                //so its a bit wonky here but the point is that we need something to interrupt the
                //dooot consumer task which is hanging on for_each_concurrent
                rabbit_close_tx.send(()).unwrap();
            },
        );

        if write_ch {
            // Bind all local output queues before any input can be published.
            let tasks = self.start_clickhouse_consumers().await?;
            futures.push(Box::pin(Self::wait_for_clickhouse_consumers(tasks)));
        }

        if consume_parse {
            let (consumer_task, dooot_receiver) = self.create_message_consumer().await?;
            info!("created consumer task");
            let dooot_writer_task = self.create_dooot_writer(dooot_receiver);
            info!("created dooot writer task");
            futures.push(Box::pin(consumer_task));
            futures.push(Box::pin(dooot_writer_task));
        }

        if let Err(error) = futures::future::try_join_all(futures).await {
            signal_task.abort();
            // Closing the connection releases all remaining unacknowledged inputs.
            let _ = self.rabbit.close(200, "Ingestooor worker stopped").await;
            return Err(error);
        }
        signal_task.await?;
        log::error!("ingestooor has ended, pausing for 3s for cleanest exit");
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        Ok(())
    }

    async fn create_consumer(&self, name: String, queue_name: String) -> Result<Consumer> {
        let recv_channel = self.rabbit.create_channel().await?;
        recv_channel
            .basic_qos(self.prefetch, BasicQosOptions::default())
            .await?;
        let consumer_tag = format!(
            "ingestooor-{}{}",
            name,
            self.debug_user
                .clone()
                .map(|s| format!("-{}", s))
                .unwrap_or("".into())
        );
        let consumer = recv_channel
            .basic_consume(
                &queue_name,
                &consumer_tag,
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;
        Ok(consumer)
    }

    /// Create a consumer task that reads from the transaction queue and sends dooot vectors to a local channel
    async fn create_message_consumer(
        &self,
    ) -> Result<(
        impl Future<Output = Result<()>> + '_,
        Receiver<(Acker, Vec<Dooot>)>,
    )> {
        let consumer = self
            .create_consumer(
                "message".to_string(),
                self.queue_info.incoming_queue_name.clone(),
            )
            .await?;
        // dooot sender
        // todo: make this channel size configurable
        let (dooot_sender, dooot_receiver) = tokio::sync::mpsc::channel(1_000);

        //spawn a thread to read from rabbit and write to the channel
        //create a shared parser manager
        let parser_manager: Rc<ParserManager> = Rc::new(ParserManager::default());
        let read_task = consumer
            //pop a clone of the sender and the parser manager onto the stream
            .map(move |a| {
                (
                    a,
                    dooot_sender.clone(),
                    parser_manager.clone(), //create another reference to the parser manager
                    self.step_utils.clone(),
                    self.term_signal_incoming.clone(),
                )
            })
            .take_while(|(.., term_signal)| {
                let is_term = term_signal.load(std::sync::atomic::Ordering::Relaxed);
                if is_term {
                    info!("received term signal, stopping message consumer");
                }
                future::ready(!is_term)
            })
            //for each message, spawn a new task to process it
            .for_each_concurrent(
                None,
                |(delivery, dooot_sender, parser_manager, step_utils, ..)| async move {
                    //a message was received

                    // multithreading this doesn't really do much
                    // would likely have to use threadlocal rabbit channels for receiving
                    // to get real multithreaded performance

                    // if uncommenting below to multithread, you have to change
                    // ParserManager and Cache reference above to be in an Arc instead of an Rc.

                    // tokio::spawn(async move {
                    Self::process_delivery(delivery, dooot_sender, parser_manager, step_utils)
                        .await;
                    // }).await
                    //     .map_err(anyhow::Error::from)
                    //     .expect("failed to join process_delivery task");
                },
            )
            .map(|_| {
                log::warn!("incoming receiving stopped");
                Ok(())
            });

        Ok((read_task, dooot_receiver))
    }

    async fn process_delivery(
        delivery: lapin::Result<Delivery>,
        dooot_sender: tokio::sync::mpsc::Sender<(Acker, Vec<Dooot>)>,
        parser_manager: Rc<ParserManager>,
        step_utils: Rc<StepUtils>,
    ) {
        match delivery {
            Ok(delivery) => {
                let message = serde_json::from_slice(delivery.data.as_slice());
                match message {
                    Ok(message) => {
                        let dooot =
                            Self::parser_entrypoint(&parser_manager, message, &step_utils).await;
                        match dooot {
                            Ok(dooot) => {
                                if !dooot.is_empty() {
                                    //send the dooot to the dooot sender task
                                    dooot_sender
                                        .send((delivery.acker, dooot))
                                        .await
                                        .expect("failed to send dooot to dooot sender");
                                } else {
                                    //ack the no dooot message
                                    delivery
                                        .ack(BasicAckOptions::default())
                                        .await
                                        .expect("failed to ack message");
                                }
                            }
                            Err(e) => {
                                log::error!("failed to parse message; nacking: {e:#?}");
                                delivery
                                    .nack(BasicNackOptions::default())
                                    .await
                                    .expect("failed to nack message");
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("failed to deserialize message: {e:#?}");
                        delivery
                            .nack(BasicNackOptions::default())
                            .await
                            .expect("failed to nack message");
                    }
                }
            }
            Err(e) => {
                log::error!("failed to receive message: {e:#?}");
                panic!("failed to receive message: {e:#?}")
            }
        }
    }

    /// Create a writer task that sends dooot vectors to the dooot exchange
    async fn create_dooot_writer(
        &self,
        mut dooot_receiver: Receiver<(Acker, Vec<Dooot>)>,
    ) -> Result<()> {
        debug!("creating dooot writer");
        if self.prefetch > 16000 {
            log::warn!(
                "Opening dooot channel... This might fail/hang if prefetch is too high, which it seems to be (current: {})",
                self.prefetch
            );
        }
        let send_channel = self.rabbit.create_channel().await?;
        debug!("Dooot channel opened");
        send_channel
            .confirm_select(ConfirmSelectOptions::default())
            .await?;
        let exchange = &self.queue_info.dooot_queue_name;

        while let Some((delivery, dooots)) = dooot_receiver.recv().await {
            match Self::publish_dooots(&send_channel, exchange, dooots).await {
                Ok(()) => delivery.ack(BasicAckOptions::default()).await?,
                Err(error) => {
                    log::error!("Failed to publish all output for a transaction: {error:#}");
                    // Keep the source delivery for inspection and replay in the configured DLQ.
                    delivery
                        .nack(BasicNackOptions {
                            multiple: false,
                            requeue: false,
                        })
                        .await?;
                    if !send_channel.status().connected() {
                        return Err(error);
                    }
                }
            }
        }

        info!("dooot rabbit writer has ended");
        Ok(())
    }

    async fn publish_dooots(
        channel: &lapin::Channel,
        exchange: &str,
        dooots: Vec<Dooot>,
    ) -> Result<()> {
        let groups = dooots.into_iter().group_by(|dooot| dooot.get_dooot_name());
        for (routing_key, group) in &groups {
            let payload = group
                .map(|dooot| serde_json::to_string(&dooot))
                .collect::<std::result::Result<Vec<_>, _>>()?
                .join("\n");
            tokio::time::timeout(Duration::from_secs(15), async {
                let confirmation = channel
                    .basic_publish(
                        exchange,
                        routing_key,
                        BasicPublishOptions {
                            mandatory: true,
                            ..Default::default()
                        },
                        payload.as_bytes(),
                        lapin::BasicProperties::default().with_delivery_mode(2),
                    )
                    .await?
                    .await?;
                require_routed_confirmation(confirmation)
            })
            .await
            .with_context(|| format!("Timed out publishing {routing_key}"))?
            .with_context(|| format!("Failed to publish {routing_key}"))?;
        }
        Ok(())
    }

    async fn start_clickhouse_consumers(
        &self,
    ) -> Result<FuturesUnordered<tokio::task::JoinHandle<()>>> {
        let Some(ch_client) = &self.ch_client else {
            info!("No clickhouse client found, not writing to clickhouse");
            return Ok(FuturesUnordered::new());
        };
        let channel = self.rabbit.create_channel().await?;
        channel
            .basic_qos(20_000, BasicQosOptions::default())
            .await?;
        let ch_data_writer = CHDataWriter::new(
            ch_client.clone(), // ok to clone. `Client` is actually a pool
            self.ch_period_bias_pct,
            channel,
            &self.queue_info,
            self.ch_max_flush_ms,
            self.term_signal_ch.clone(),
        );

        // When creating a new dooot, add the inserter task here, and add it to the tasks collection below
        let native_stake_event_task = ch_data_writer
            .make_inserter_task::<NativeStakeEventDooot>()
            .await?;
        let sol_transfer_task = ch_data_writer
            .make_inserter_task::<SolTransferEventDooot>()
            .await?;
        let spl_transfer_task = ch_data_writer
            .make_inserter_task::<TokenTransferEventDooot>()
            .await?;
        let spl_token_balance_task = ch_data_writer
            .make_inserter_task::<TokenBalanceUserDooot>()
            .await?;
        let supply_change_task = ch_data_writer
            .make_inserter_task::<MintSupplyGlobalDooot>()
            .await?;
        let mint_underlyings_task = ch_data_writer
            .make_inserter_task::<MintUnderlyingsGlobalDooot>()
            .await?;
        let swap_task = ch_data_writer
            .make_inserter_task::<SwapEventDooot>()
            .await?;
        let slot_stats_task = ch_data_writer
            .make_inserter_task::<SlotStatsGlobalDooot>()
            .await?;
        let token_missing_owner_task = ch_data_writer
            .make_inserter_task::<TokenMissingOwnerGlobalDooot>()
            .await?;
        let mint_metadata_task = ch_data_writer.make_inserter_task::<MintInfoDooot>().await?;
        let account_metadata_task = ch_data_writer
            .make_inserter_task::<AccountInfoDooot>()
            .await?;
        // No NftActivity task
        let clob_fill_event_task = ch_data_writer
            .make_inserter_task::<ClobFillEventDooot>()
            .await?;
        let clob_user_task = ch_data_writer.make_inserter_task::<ClobUserDooot>().await?;
        let lending_gstate_task = ch_data_writer
            .make_inserter_task::<LendingGlobalDooot>()
            .await?;
        let lending_ustate_task = ch_data_writer
            .make_inserter_task::<LendingUserDooot>()
            .await?;
        let repeating_order_user_task = ch_data_writer
            .make_inserter_task::<RepeatingOrderUserDooot>()
            .await?;
        let wallet_claimable_balance_task = ch_data_writer
            .make_inserter_task::<ClaimableBalanceUserDooot>()
            .await?;
        let global_clmm_state_task = ch_data_writer
            .make_inserter_task::<ClmmGlobalDooot>()
            .await?;
        let user_clmm_state_task = ch_data_writer.make_inserter_task::<ClmmUserDooot>().await?;
        let sol_balance_task = ch_data_writer
            .make_inserter_task::<SolBalanceUserDooot>()
            .await?;
        let liquidity_change_event_task = ch_data_writer
            .make_inserter_task::<LiquidityChangeEventDooot>()
            .await?;
        let managed_account_task = ch_data_writer
            .make_inserter_task::<ManagedAccountUserDooot>()
            .await?;
        let lp_info_gstate_task = ch_data_writer.make_inserter_task::<LPInfoDooot>().await?;
        let dlmm_bin_array_task = ch_data_writer
            .make_inserter_task::<DlmmGlobalDooot>()
            .await?;
        let lp_user_dlmm_position_task =
            ch_data_writer.make_inserter_task::<DlmmUserDooot>().await?;
        let farm_info_task = ch_data_writer.make_inserter_task::<FarmInfoDooot>().await?;
        let gfarm_state_task = ch_data_writer
            .make_inserter_task::<FarmGlobalDooot>()
            .await?;
        let ufarm_state_task = ch_data_writer.make_inserter_task::<FarmUserDooot>().await?;
        let oracle_price_task = ch_data_writer
            .make_inserter_task::<OraclePriceEventDooot>()
            .await?;
        let token_price_task = ch_data_writer
            .make_inserter_task::<TokenPriceGlobalDooot>()
            .await?;
        let reward_option_user_task = ch_data_writer
            .make_inserter_task::<RewardOptionUserDooot>()
            .await?;
        let vault_info_task = ch_data_writer
            .make_inserter_task::<VaultInfoDooot>()
            .await?;
        let clmm_tick_task = ch_data_writer
            .make_inserter_task::<ClmmTickGlobalDooot>()
            .await?;
        let subs_user_task = ch_data_writer
            .make_inserter_task::<SubscriptionUserDooot>()
            .await?;
        let subs_purchase_task = ch_data_writer
            .make_inserter_task::<SubscriptionPurchaseEventDooot>()
            .await?;
        let yield_brearing_task = ch_data_writer
            .make_inserter_task::<YieldBearingGlobalDooot>()
            .await?;
        let defituna_fusion_pool_global_task = ch_data_writer
            .make_inserter_task::<PsDefiTunaFusionPoolDooot>()
            .await?;
        let defituna_lending_position_user_task = ch_data_writer
            .make_inserter_task::<PsDefiTunaLendingPositionDooot>()
            .await?;
        let defituna_market_task = ch_data_writer
            .make_inserter_task::<PsDefiTunaMarketDooot>()
            .await?;
        let defituna_tuna_position_user_task = ch_data_writer
            .make_inserter_task::<PsDefiTunaTunaPositionDooot>()
            .await?;
        let defituna_vault_global_task = ch_data_writer
            .make_inserter_task::<PsDefiTunaVaultDooot>()
            .await?;
        let nx_lend_position_task = ch_data_writer
            .make_inserter_task::<PsNxLendPositionDooot>()
            .await?;
        let nx_leverage_margin_pool_task = ch_data_writer
            .make_inserter_task::<PsNxLeverageMarginPoolDooot>()
            .await?;
        let nx_lend_vsol_pool_task = ch_data_writer
            .make_inserter_task::<PsNxLendVSolPoolDooot>()
            .await?;
        let nx_leverage_margin_account_task = ch_data_writer
            .make_inserter_task::<PsNxLeverageMarginAccountDooot>()
            .await?;
        let nx_lend_solayer_pool_task = ch_data_writer
            .make_inserter_task::<PsNxLendSolayerPoolDooot>()
            .await?;
        let nx_leverage_jupiter_account_task = ch_data_writer
            .make_inserter_task::<PsNxLeverageJupiterAccountDooot>()
            .await?;
        let nx_lend_lending_account_task = ch_data_writer
            .make_inserter_task::<PsNxLendLendingAccountDooot>()
            .await?;
        let nx_lend_lending_pool_task = ch_data_writer
            .make_inserter_task::<PsNxLendLendingPoolDooot>()
            .await?;
        let nx_lend_vsol_position_task = ch_data_writer
            .make_inserter_task::<PsNxLendVSolPositionDooot>()
            .await?;
        let nx_lend_fragmetric_pool_task = ch_data_writer
            .make_inserter_task::<PsNxLendFragmetricPoolDooot>()
            .await?;
        let nx_lend_fragmetric_user_task = ch_data_writer
            .make_inserter_task::<PsNxLendFragmetricUserDooot>()
            .await?;

        let tasks = FuturesUnordered::new();
        tasks.push(native_stake_event_task);
        tasks.push(sol_transfer_task);
        tasks.push(spl_transfer_task);
        tasks.push(spl_token_balance_task);
        tasks.push(supply_change_task);
        tasks.push(mint_underlyings_task);
        tasks.push(swap_task);
        tasks.push(clob_fill_event_task);
        tasks.push(slot_stats_task);
        tasks.push(token_missing_owner_task);
        tasks.push(mint_metadata_task);
        tasks.push(account_metadata_task);
        tasks.push(clob_user_task);
        tasks.push(lending_gstate_task);
        tasks.push(lending_ustate_task);
        tasks.push(repeating_order_user_task);
        tasks.push(wallet_claimable_balance_task);
        tasks.push(global_clmm_state_task);
        tasks.push(managed_account_task);
        tasks.push(user_clmm_state_task);
        tasks.push(sol_balance_task);
        tasks.push(liquidity_change_event_task);
        tasks.push(lp_info_gstate_task);
        tasks.push(dlmm_bin_array_task);
        tasks.push(lp_user_dlmm_position_task);
        tasks.push(farm_info_task);
        tasks.push(gfarm_state_task);
        tasks.push(ufarm_state_task);
        tasks.push(oracle_price_task);
        tasks.push(token_price_task);
        tasks.push(reward_option_user_task);
        tasks.push(vault_info_task);
        tasks.push(clmm_tick_task);
        tasks.push(subs_user_task);
        tasks.push(subs_purchase_task);
        tasks.push(yield_brearing_task);
        tasks.push(defituna_fusion_pool_global_task);
        tasks.push(defituna_lending_position_user_task);
        tasks.push(defituna_market_task);
        tasks.push(defituna_tuna_position_user_task);
        tasks.push(defituna_vault_global_task);
        tasks.push(nx_lend_position_task);
        tasks.push(nx_leverage_margin_pool_task);
        tasks.push(nx_lend_vsol_pool_task);
        tasks.push(nx_leverage_margin_account_task);
        tasks.push(nx_lend_solayer_pool_task);
        tasks.push(nx_leverage_jupiter_account_task);
        tasks.push(nx_lend_lending_account_task);
        tasks.push(nx_lend_lending_pool_task);
        tasks.push(nx_lend_vsol_position_task);
        tasks.push(nx_lend_fragmetric_pool_task);
        tasks.push(nx_lend_fragmetric_user_task);

        Ok(tasks)
    }

    async fn wait_for_clickhouse_consumers(
        mut tasks: FuturesUnordered<tokio::task::JoinHandle<()>>,
    ) -> Result<()> {
        while let Some(result) = tasks.next().await {
            match result {
                Ok(_) => {
                    log::info!("An inserter task finished... exiting");
                }
                Err(e) => {
                    log::error!("Task failed: {:?}", e);
                }
            }
        }
        Ok(())
    }

    async fn parser_entrypoint(
        parser_manager: &ParserManager,
        message: Message,
        step_utils: &StepUtils,
    ) -> Result<Vec<Dooot>> {
        // Initialize return value
        let mut dooot: Vec<Dooot> = vec![];

        match message {
            Message::TransactionNotify(tx) => {
                let TransactionNotify {
                    transaction:
                        EncodedConfirmedTransactionWithStatusMeta {
                            slot,
                            block_time,
                            transaction:
                                EncodedTransactionWithStatusMeta {
                                    transaction,
                                    meta,
                                    version: _,
                                },
                            index_in_block,
                            slot_second_idx,
                        },
                } = *tx;

                let block_time = block_time.context("tx has no block time")?;
                let converted_bt_micros =
                    convert_blocktime_to_micros(block_time, slot_second_idx, Some(index_in_block))?;
                // Decode transaction.
                // This was done in this way so that any base58/base64 decoding
                // that needs to get done on binary only gets done once per transaction.
                //
                // This essentially standardizes all the different transaction types
                let decoded_transaction: DecodedTransaction = match standardized_decoded_transaction(
                    converted_bt_micros,
                    transaction,
                    meta.context("failed to get meta")?,
                    slot,
                ) {
                    Ok(value) => value,

                    // May not be able to standardize or decode transaction.
                    // Not parsable, so return empty dooot.
                    Err(_empty_dooot) => {
                        return Err(anyhow::anyhow!("failed to decode transaction"))
                    }
                };

                log::trace!(
                    "decoded transaction: {}",
                    decoded_transaction.get_signature()
                );
                for parser in parser_manager.get_parsers_for_transaction(&decoded_transaction) {
                    let parser_name = parser.get_name();

                    let parser_result = parser
                        .invoke(&decoded_transaction, &mut dooot, step_utils)
                        .await;
                    match parser_result {
                        Ok(_) => {
                            //metrics for each dooot generated
                            for s in dooot.iter() {
                                let counter =
                                    metrics::counter!("Generated", "Dooot" => s.get_dooot_name());
                                counter.increment(1);
                                let counter =
                                    metrics::counter!("InvokeSuccess", "Parser" => parser_name);
                                counter.increment(1);
                            }
                        }
                        Err(e) => {
                            let counter =
                                metrics::counter!("InvokeFailure", "Parser" => parser_name);
                            counter.increment(1);
                            log::error!(
                                "parser {} failed on tx: {} with error: {:#?}",
                                parser_name,
                                decoded_transaction.get_signature(),
                                e
                            );
                            return Err(e);
                        }
                    }
                }
            }

            Message::SlotStatisticsNotify(ss) => {
                log::trace!("received SlotStatisticsNotify");
                parser_manager
                    .get_slot_stats_parser()
                    .invoke(ss, &mut dooot)?
            }

            // Other message types
            Message::BlockMetadataNotify(_)
            | Message::SlotStatusNotify(_)
            | Message::InstructionNotify(_)
            | Message::AccountUpdate(_) => {
                log::error!("ignoring other");
            }
        }

        Ok(dooot)
    }
}

fn require_routed_confirmation(confirmation: Confirmation) -> Result<()> {
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(returned)) => {
            anyhow::bail!(
                "Broker returned unroutable output (reply code {})",
                returned.reply_code
            )
        }
        Confirmation::Nack(_) => anyhow::bail!("Broker rejected output"),
        Confirmation::NotRequested => anyhow::bail!("Publisher confirms must be enabled"),
    }
}

#[cfg(test)]
mod publish_tests {
    use lapin::message::BasicReturnMessage;

    use super::*;

    #[test]
    fn output_requires_an_ack_with_no_returned_message() {
        let returned = BasicReturnMessage {
            reply_code: 312,
            reply_text: "NO_ROUTE".into(),
            delivery: Delivery {
                delivery_tag: 0,
                exchange: "fixture".into(),
                routing_key: "fixture".into(),
                redelivered: false,
                properties: lapin::BasicProperties::default(),
                data: Vec::new(),
                acker: Acker::default(),
            },
        };
        assert!(require_routed_confirmation(Confirmation::Ack(None)).is_ok());
        assert!(require_routed_confirmation(Confirmation::Ack(Some(Box::new(returned)))).is_err());
        assert!(require_routed_confirmation(Confirmation::Nack(None)).is_err());
        assert!(require_routed_confirmation(Confirmation::NotRequested).is_err());
    }
}
