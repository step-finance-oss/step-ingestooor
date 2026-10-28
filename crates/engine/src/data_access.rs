use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use anyhow::{Context, Result};
use clickhouse::{
    inserter::{Inserter, Quantities},
    Row,
};
use deadpool_postgres::{Config, Object, Pool, Runtime};
use futures::StreamExt;
use lapin::{
    message::Delivery,
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, QueueBindOptions,
        QueueDeclareOptions,
    },
    types::{AMQPValue, FieldTable},
};
use serde::{de::DeserializeOwned, Serialize};
use step_ingestooor_sdk::dooot::ClickhouseDoootTrait;
use tokio::time::timeout;
use tokio_postgres::NoTls;

use crate::rabbit_factory::UserRabbitQueueInfo;

// this is used in state init crate so marking as dead_code here
#[allow(dead_code)]
#[derive(Clone)]
pub struct DataReader {
    connection_pool: Pool,
}

#[allow(dead_code)]
impl DataReader {
    pub async fn new(config: Config) -> Result<Self> {
        let connection_pool = config.create_pool(Some(Runtime::Tokio1), NoTls)?;

        Ok(Self { connection_pool })
    }

    pub async fn get(&self) -> Result<Object> {
        let client = self.connection_pool.get().await?;
        Ok(client)
    }
}

/// Lives on main thread
pub struct CHDataWriter<'a> {
    connection: clickhouse::Client,
    ch_period_bias_pct: f64,
    amqp_channel: lapin::Channel,
    amqp_settings: &'a UserRabbitQueueInfo,
    ch_max_flush_ms: u64,
    term_signal_ch: Arc<AtomicBool>,
}

impl<'a> CHDataWriter<'a> {
    pub fn new(
        connection: clickhouse::Client,
        ch_period_bias_pct: f64,
        amqp_channel: lapin::Channel,
        amqp_settings: &'a UserRabbitQueueInfo,
        ch_max_flush_ms: u64,
        term_signal_ch: Arc<AtomicBool>,
    ) -> Self {
        Self {
            connection,
            ch_period_bias_pct,
            amqp_channel,
            amqp_settings,
            ch_max_flush_ms,
            term_signal_ch,
        }
    }

    pub fn make_inserter_task<
        T: Row + ClickhouseDoootTrait + Serialize + DeserializeOwned + Send + 'static,
    >(
        &self,
    ) -> Result<tokio::task::JoinHandle<()>> {
        let def = T::default();
        let dooot_name = def.get_dooot_name();
        let table_name = def.get_table_name().context("table name not found")?;
        let queue_name = def.get_queue_name().context("queue name not found")?;
        let consumer_tag = format!("inserter-{}", queue_name);
        let mut flush_ms = def.get_flush_ms();
        let max_flush_ms = self.ch_max_flush_ms;
        let max_inserter_rows = def.get_inserter_max_rows();
        if flush_ms > max_flush_ms {
            log::warn!(
                "flush_ms {} is greater than max_flush_ms {} for dooot {}. Setting flush_ms to max_flush_ms",
                flush_ms,
                max_flush_ms,
                dooot_name
            );
            flush_ms = max_flush_ms;
        }

        let inserter = self
            .connection
            .inserter::<T>(table_name)?
            .with_max_rows(max_inserter_rows)
            .with_period(Some(Duration::from_millis(flush_ms)))
            .with_period_bias(self.ch_period_bias_pct);

        let mut wrapped_inserter = WrappedInserter {
            inserter,
            amqp_messages: Vec::with_capacity(25000),
            consumer_tag: consumer_tag.clone(),
        };

        let UserRabbitQueueInfo {
            debug_user,
            ch_dlx_name,
            ch_exchange_name,
            auto_delete,
            ..
        } = self.amqp_settings;

        let ch_exchange_name = ch_exchange_name.clone();

        let full_queue_name = if let Some(debug_user) = debug_user {
            format!("ch.{queue_name}.debug.{debug_user}")
        } else {
            format!("ch.{queue_name}")
        };

        let dlq_name = format!("{full_queue_name}.dead-letter");

        let is_debug = debug_user.is_some();
        let auto_delete = *auto_delete;
        let dlx_name = ch_dlx_name.clone();

        let amqp_channel = self.amqp_channel.clone();
        let term_signal_ch = self.term_signal_ch.clone();

        let inserter_task = tokio::spawn(
            #[allow(clippy::unwrap_used)]
            async move {
                let mut queue_args = FieldTable::default();
                queue_args.insert(
                    "x-dead-letter-exchange".into(),
                    AMQPValue::LongString(dlx_name.clone().into()),
                );
                // Declare the queue
                log::info!("Declaring queue: {full_queue_name}");
                amqp_channel
                    .queue_declare(
                        &full_queue_name,
                        QueueDeclareOptions {
                            durable: !is_debug,
                            auto_delete,
                            exclusive: auto_delete,
                            ..Default::default()
                        },
                        queue_args,
                    )
                    .await
                    .context(format!("Failed to declare queue {full_queue_name}"))
                    .unwrap();

                // Declare the DLQ
                log::info!("Declaring DLQ: {dlq_name}");
                amqp_channel
                    .queue_declare(
                        &dlq_name,
                        QueueDeclareOptions {
                            durable: !is_debug,
                            auto_delete,
                            exclusive: auto_delete,
                            ..Default::default()
                        },
                        FieldTable::default(),
                    )
                    .await
                    .context(format!("Failed to declare DLQ {dlq_name}"))
                    .unwrap();

                // Bind the DLQ to the DLX
                log::info!("Binding DLQ to DLX: {dlq_name} -> {dlx_name}");
                amqp_channel
                    .queue_bind(
                        &dlq_name,
                        &dlx_name,
                        dooot_name,
                        QueueBindOptions::default(),
                        FieldTable::default(),
                    )
                    .await
                    .context(format!("Failed to bind DLQ {dlq_name} to DLX {dlx_name}"))
                    .unwrap();

                // Bind queue to CH_EXCHANGE
                log::info!("Binding queue to CH_EXCHANGE: {full_queue_name} -> {ch_exchange_name}");
                amqp_channel
                    .queue_bind(
                        &full_queue_name,
                        &ch_exchange_name,
                        dooot_name,
                        QueueBindOptions::default(),
                        FieldTable::default(),
                    )
                    .await
                    .context(format!(
                        "Failed to bind queue {full_queue_name} to CH_EXCHANGE {ch_exchange_name}"
                    ))
                    .unwrap();

                // Create consumer
                log::info!("Creating consumer: {consumer_tag}");
                let mut consumer = amqp_channel
                    .basic_consume(
                        &full_queue_name,
                        &consumer_tag,
                        BasicConsumeOptions::default(),
                        FieldTable::default(),
                    )
                    .await
                    .context(format!("Failed to create consumer {consumer_tag}"))
                    .unwrap();

                log::info!("Starting {dooot_name} consumer");

                loop {
                    // Check if the termination signal is set
                    if term_signal_ch.load(std::sync::atomic::Ordering::Relaxed) {
                        log::info!("Termination signal received, stopping {dooot_name} consumer");
                        break;
                    }

                    let time_left = wrapped_inserter
                        .inserter
                        .time_left()
                        .unwrap_or_else(|| Duration::from_millis(flush_ms));

                    // consumer.next() is cancel-safe to use with tokio::time::timeout
                    let delivery = match timeout(time_left, consumer.next()).await {
                        Ok(d) => d,
                        Err(_) => {
                            // Went past the time limit, flush the inserter, to not cause a timeout error
                            wrapped_inserter.commit_and_flush().await;

                            continue;
                        }
                    };

                    let Some(delivery) = delivery else {
                        // StreamExt Iterator finished
                        break;
                    };

                    match delivery {
                        Ok(delivery) => {
                            let parsed_dooots: Result<Vec<T>, _> = delivery
                                .data
                                .split(|b| *b == b'\n')
                                .map(serde_json::from_slice)
                                .collect();

                            match parsed_dooots {
                                Ok(pd) => {
                                    wrapped_inserter
                                        .push_message(delivery, pd)
                                        .context(format!(
                                            "Failed to push message into WrappedInserter for {}",
                                            consumer_tag
                                        ))
                                        .unwrap();
                                    wrapped_inserter.commit_and_flush().await;
                                }
                                Err(e) => {
                                    log::error!("error parsing dooots: {:?}", e);
                                    delivery
                                        .nack(BasicNackOptions::default())
                                        .await
                                        .context(format!(
                                            "Could not nack message in {}",
                                            consumer_tag
                                        ))
                                        .unwrap();
                                }
                            }
                        }
                        Err(e) => {
                            panic!("{consumer_tag} - Error getting delivery {e}");
                        }
                    }
                }
            },
        );

        Ok(inserter_task)
    }
}

struct WrappedInserter<T: Row + Serialize> {
    pub inserter: Inserter<T>,
    pub amqp_messages: Vec<Delivery>,
    pub consumer_tag: String,
}

impl<T: Row + Serialize> WrappedInserter<T> {
    #[allow(clippy::unwrap_used)]
    /// Commit the inserter and flush the messages if the inserter has written any rows
    /// Retries up to 5 times with exponential backoff on failure
    async fn commit_and_flush(&mut self) {
        const MAX_RETRIES: u32 = 5;
        let mut retry_count = 0;

        loop {
            match self.inserter.commit().await {
                Ok(qtys) => {
                    if qtys == Quantities::ZERO {
                        // No rows written, nothing to commit/ack
                        return;
                    }

                    log::debug!("Acking {} messages", self.amqp_messages.len());
                    for delivery in self.amqp_messages.drain(..) {
                        delivery
                            .ack(BasicAckOptions::default())
                            .await
                            .unwrap_or_else(|_| panic!("Could not ack in {}", self.consumer_tag));
                    }
                    return; // Success, exit the retry loop
                }
                Err(e) => {
                    retry_count += 1;

                    if retry_count > MAX_RETRIES {
                        log::error!(
                            "Failed to commit dooots for {} after {} retries: {:?}. Nacking {} messages",
                            self.consumer_tag,
                            MAX_RETRIES,
                            e,
                            self.amqp_messages.len()
                        );
                        for delivery in self.amqp_messages.drain(..) {
                            delivery
                                .nack(BasicNackOptions::default())
                                .await
                                .unwrap_or_else(|_| {
                                    panic!("Could not nack in {}", self.consumer_tag)
                                });
                        }
                        return;
                    }

                    let delay_ms = 2_u64.pow(retry_count - 1) * 100; // Exponential backoff: 100ms, 200ms, 400ms, 800ms, 1600ms
                    log::warn!(
                        "Commit failed for {} (attempt {}/{}): {:?}. Retrying in {}ms...",
                        self.consumer_tag,
                        retry_count,
                        MAX_RETRIES,
                        e,
                        delay_ms
                    );

                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }

    pub fn push_message(&mut self, message: Delivery, dooots: Vec<T>) -> Result<()> {
        for dooot in dooots.into_iter() {
            self.inserter.write(&dooot)?;
        }
        self.amqp_messages.push(message);

        Ok(())
    }
}
