use anyhow::{Context, Result};
use lapin::{
    options::{ExchangeBindOptions, ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions},
    types::{AMQPValue, FieldTable},
    Connection, ExchangeKind,
};

#[derive(Clone, Debug)]
pub struct UserRabbitQueueInfo {
    pub exchange_name: String,
    pub msg_exchange_options: ExchangeDeclareOptions,
    pub dooot_exchange_options: ExchangeDeclareOptions,
    pub msg_dlx_options: ExchangeDeclareOptions,
    pub dooot_dlx_options: ExchangeDeclareOptions,
    pub incoming_queue_name: String,
    pub dooot_queue_name: String,
    pub dlq_name: String,
    pub dooot_dlq_name: String,
    pub queue_options: QueueDeclareOptions,
    pub ch_dlx_name: String,
    pub ch_exchange_name: String,
    pub debug_user: Option<String>,
    pub auto_delete: bool,
}

pub async fn connect_to_rabbit(connection_string: &str) -> Result<Connection> {
    let regex = regex::Regex::new(r"(amqps?:.+:)(.+)(@.*)")?;
    let redacted_con_string = regex.replace_all(connection_string, "${1}redacted${3}");
    log::debug!("connection string: {redacted_con_string}");

    amqp_connect(connection_string, "ingestooor-rabbit-factory").await
}

pub async fn amqp_connect(addr: impl AsRef<str>, name: &'static str) -> Result<lapin::Connection> {
    lapin::Connection::connect(
        addr.as_ref(),
        lapin::ConnectionProperties::default()
            .with_connection_name(
                format!(
                    "{}@{}",
                    name,
                    hostname::get()
                        .context("Failed to get system hostname")?
                        .into_string()
                        .map_err(|_| anyhow::anyhow!("Failed to parse system hostname"))?,
                )
                .into(),
            )
            .with_executor(tokio_executor_trait::Tokio::current())
            .with_reactor(tokio_reactor_trait::Tokio),
    )
    .await
    .context("Failed to connect to the AMQP server")
}

pub fn get_queue_info(
    prefix: String,
    mut exchange_name: String,
    debug_user: Option<String>,
    debug_exchange: bool,
    auto_delete_queues_setting: Option<bool>,
) -> UserRabbitQueueInfo {
    let mut dooot_queue_name = format!("{prefix}.dooot");
    let mut incoming_queue_name = format!("{prefix}.incoming");
    let mut dlq_name = format!("{incoming_queue_name}.dead-letter");
    let mut dooot_dlq_name = format!("{dooot_queue_name}.dead-letter");
    let mut ch_exchange_name = "ch.dooot".to_string();
    let mut ch_dlx_name = format!("{ch_exchange_name}.dead-letter");
    let is_debug = debug_user.is_some();

    if let Some(ref username) = debug_user {
        // Add debug username to queue names
        incoming_queue_name.push_str(&format!(".debug.{username}"));
        dlq_name.push_str(&format!(".debug.{username}"));
        dooot_queue_name.push_str(&format!(".debug.{username}"));
        dooot_dlq_name.push_str(&format!(".debug.{username}"));
        ch_exchange_name.push_str(&format!(".debug.{username}"));
        ch_dlx_name.push_str(&format!(".debug.{username}"));

        if debug_exchange {
            exchange_name.push_str(&format!(".debug.{username}"));
        }
    }

    // Auto-delete by default, but only if debug is enabled
    let defaulted_auto_delete_setting = auto_delete_queues_setting.unwrap_or(true);
    let auto_delete = defaulted_auto_delete_setting && is_debug;

    let queue_options = QueueDeclareOptions {
        durable: !is_debug,
        auto_delete,
        exclusive: auto_delete,
        ..Default::default()
    };

    let dooot_exchange_options = ExchangeDeclareOptions {
        durable: !is_debug,
        auto_delete,
        ..Default::default()
    };

    let msg_exchange_options = ExchangeDeclareOptions {
        durable: !debug_exchange,
        // Only delete this exchange if explicitly a debug exchange, and if the setting allows it
        auto_delete: debug_exchange && defaulted_auto_delete_setting,
        ..Default::default()
    };

    let msg_dlx_options = ExchangeDeclareOptions {
        durable: !debug_exchange,
        auto_delete,
        ..Default::default()
    };

    let dooot_dlx_options = ExchangeDeclareOptions {
        durable: !is_debug,
        auto_delete,
        ..Default::default()
    };

    UserRabbitQueueInfo {
        exchange_name,
        msg_exchange_options,
        dooot_exchange_options,
        msg_dlx_options,
        dooot_dlx_options,
        incoming_queue_name,
        dooot_queue_name,
        dlq_name,
        dooot_dlq_name,
        queue_options,
        ch_dlx_name,
        ch_exchange_name,
        debug_user,
        auto_delete,
    }
}

#[allow(clippy::unwrap_used)]
pub async fn assert_amqp_topology(
    rabbit: &Connection,
    queue_info: &UserRabbitQueueInfo,
) -> Result<()> {
    let UserRabbitQueueInfo {
        exchange_name,
        msg_exchange_options,
        dooot_exchange_options,
        msg_dlx_options,
        dooot_dlx_options,
        incoming_queue_name,
        dooot_queue_name,
        dlq_name,
        dooot_dlq_name,
        queue_options,
        ch_exchange_name,
        ch_dlx_name,
        ..
    } = queue_info;

    let declaration_channel = rabbit.create_channel().await.unwrap();
    let mut incoming_queue_args = FieldTable::default();

    incoming_queue_args.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(dlq_name.clone().into()),
    );

    log::info!("Declaring Incoming TX exchange: {exchange_name}");
    declaration_channel
        .exchange_declare(
            exchange_name,
            ExchangeKind::Topic,
            *msg_exchange_options,
            FieldTable::default(),
        )
        .await?;

    // Declare Incoming TX DL Exchange (DLX)
    log::info!("Declaring Incoming TX DLX: {dlq_name}");
    declaration_channel
        .exchange_declare(
            dlq_name,
            ExchangeKind::Topic,
            *msg_dlx_options,
            FieldTable::default(),
        )
        .await?;

    // Declare Incoming TX DLQ
    log::info!("Declaring Incoming TX DLQ: {dlq_name}");
    declaration_channel
        .queue_declare(dlq_name, *queue_options, FieldTable::default())
        .await?;

    // Bind Incoming TX DLX -> Incoming TX DLQ
    declaration_channel
        .queue_bind(
            dlq_name,
            dlq_name,
            "#",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    // Declare Incoming TX Queue
    log::info!("Declaring Incoming TX Queue: {incoming_queue_name}");
    declaration_channel
        .queue_declare(incoming_queue_name, *queue_options, incoming_queue_args)
        .await?;

    // Bind `mainnet.confirmed.messages` -> Incoming Queue
    log::info!(
        "Binding Incoming TX Queue: {incoming_queue_name} to defi.# on exchange {exchange_name}"
    );
    declaration_channel
        .queue_bind(
            incoming_queue_name,
            exchange_name.as_ref(),
            "defi.#",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;
    log::info!(
        "Binding Incoming TX Queue: {incoming_queue_name} to multi.# on exchange {exchange_name}"
    );
    declaration_channel
        .queue_bind(
            incoming_queue_name,
            exchange_name.as_ref(),
            "multi.#",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    //outgoing topology
    log::info!("Declaring dooot exchange: {dooot_queue_name}");
    declaration_channel
        .exchange_declare(
            dooot_queue_name,
            ExchangeKind::Topic,
            *dooot_exchange_options,
            FieldTable::default(),
        )
        .await?;

    let mut incoming_dooot_queue_args = FieldTable::default();
    incoming_dooot_queue_args.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(dooot_dlq_name.clone().into()),
    );

    // Declare CH Exchange
    log::info!("Declaring CH Exchange: {ch_exchange_name}");
    declaration_channel
        .exchange_declare(
            ch_exchange_name,
            ExchangeKind::Topic,
            *dooot_exchange_options,
            FieldTable::default(),
        )
        .await?;

    // Declare CH DLX
    log::info!("Declaring CH DLX: {ch_dlx_name}");
    declaration_channel
        .exchange_declare(
            ch_dlx_name,
            ExchangeKind::Topic,
            *dooot_dlx_options,
            FieldTable::default(),
        )
        .await?;

    // Bind ingestooor.dooot EXCHANGE -> CH_EXCHANGE
    log::info!("Binding ingestooor.dooot EXCHANGE -> CH_EXCHANGE: {dooot_queue_name} -> {ch_exchange_name}");
    declaration_channel
        .exchange_bind(
            ch_exchange_name,
            dooot_queue_name,
            "#",
            ExchangeBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    declaration_channel.close(200, "Bye").await?;

    Ok(())
}
