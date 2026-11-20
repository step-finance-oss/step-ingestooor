use std::{
    rc::Rc,
    sync::{atomic::AtomicBool, Arc},
};

use anyhow::Result;
use clap::Parser;
use metrics_exporter_prometheus::PrometheusBuilder;
use step_ingestooor_engine::axum::run_server::run_axum_server;
use step_ingestooor_sdk::utils::{r#async::run_future_on_new_thread, step_utils::StepUtils};

mod axum;
mod ch_cache_manager;
mod ch_cache_sql;
mod data_access;
mod hybrid_cache_manager;
mod ingestooor;
mod parser_manager;
mod rabbit_factory;
mod slot_stats;
mod ts_cache_manager;
mod ts_cache_sql;

#[derive(Parser, PartialEq, Eq, Debug)]
pub struct IngestooorArgs {
    #[clap(flatten)]
    pub amqp_params: AMQPParams,

    #[clap(long, env, parse(try_from_str), default_value = "true")]
    pub enable_parsing: bool,

    #[clap(long, env, parse(try_from_str), default_value = "true")]
    pub enable_pg_writes: bool,

    #[clap(long, env, parse(try_from_str), default_value = "true")]
    pub enable_clickhouse_writes: bool,

    #[clap(long, env)]
    pub ipfs_url: Option<String>,

    #[clap(flatten)]
    pub clickhouse_params: ClickhouseParams,
}

#[derive(Parser, PartialEq, Eq, Debug)]
pub struct AMQPParams {
    /// The address of an AMQP server to connect to
    #[clap(long, env)]
    pub rabbitmq_url: String,

    /// The queue prefix
    #[clap(long, env)]
    pub rabbitmq_queue_prefix: String,

    /// The exchange to use
    #[clap(long, env)]
    pub rabbitmq_exchange: String,

    /// Debug exchange. If true, the exchange will be created with the debug suffix
    #[clap(long, env, parse(try_from_str), default_value = "false")]
    pub debug_exchange: bool,

    /// Debug user name. If not defined then no suffix is used
    #[clap(long, env)]
    pub debug_user: Option<String>,

    /// The rabbitMQ prefetch count to use when reading from queues
    /// This loosely translates to # simultaneous messages being processed
    #[clap(long, env)]
    pub rabbitmq_prefetch: Option<u16>,

    /// Set to false to disable auto-delete queues
    #[clap(long, env)]
    pub auto_delete_queues: Option<bool>,
}

#[derive(Parser, PartialEq, Eq, Debug)]
pub struct ClickhouseParams {
    #[clap(long, env)]
    pub clickhouse_url: String,

    #[clap(long, env)]
    pub clickhouse_user: String,

    #[clap(long, env)]
    pub clickhouse_password: Option<String>,

    #[clap(long, env, default_value = "default")]
    pub clickhouse_database: String,

    /// See https://docs.rs/clickhouse/latest/clickhouse/inserter/struct.Inserter.html#method.with_period_bias
    /// Default 0.05 (5%)
    #[clap(long, env, default_value = "500")]
    pub clickhouse_period_bias_bps: u16,

    /// The maximum flush time in milliseconds
    #[clap(long, env, default_value = "25000")]
    pub clickhouse_max_flush_ms: u64,
}

impl ClickhouseParams {
    fn client(&self) -> clickhouse::Client {
        let client = clickhouse::Client::default()
            .with_url(&self.clickhouse_url)
            .with_user(&self.clickhouse_user)
            .with_database(&self.clickhouse_database);
        match &self.clickhouse_password {
            Some(password) => client.with_password(password),
            None => client,
        }
    }
}

#[cfg(test)]
mod clickhouse_tests {
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
    };

    use super::*;

    #[tokio::test]
    async fn clickhouse_requests_select_the_configured_database() -> anyhow::Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let host = listener.local_addr()?;
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await?;
            let mut socket = BufReader::new(socket);
            let mut request_line = String::new();
            socket.read_line(&mut request_line).await?;
            socket
                .get_mut()
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n\x01")
                .await?;
            Ok::<_, std::io::Error>(request_line)
        });
        let params = ClickhouseParams {
            clickhouse_url: format!("http://{host}"),
            clickhouse_user: "fixture".to_string(),
            clickhouse_password: None,
            clickhouse_database: "fixture_database".to_string(),
            clickhouse_period_bias_bps: 0,
            clickhouse_max_flush_ms: 1000,
        };
        let client = params
            .client()
            .with_compression(clickhouse::Compression::None);
        let query = async { client.query("SELECT 1").fetch_one::<u8>().await };
        let row = tokio::time::timeout(std::time::Duration::from_secs(5), query).await??;
        assert_eq!(row, 1);
        assert!(server.await??.contains("database=fixture_database"));
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();
    // From https://github.com/rustls/rustls/issues/1938#issuecomment-2099920256
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // setup metrics
    let recorder = PrometheusBuilder::new().build_recorder();
    let prometheus_handle = recorder.handle();

    metrics::set_global_recorder(Box::new(recorder)).unwrap_or_else(|e| {
        panic!("Error setting global recorder: {:?}", e);
    });

    run_future_on_new_thread(run_axum_server(prometheus_handle));

    let args = IngestooorArgs::parse();

    let amqp_params = args.amqp_params;
    let queue_info = rabbit_factory::get_queue_info(
        amqp_params.rabbitmq_queue_prefix,
        amqp_params.rabbitmq_exchange,
        amqp_params.debug_user.clone(),
        amqp_params.debug_exchange,
        amqp_params.auto_delete_queues,
    );

    //create rabbit dooot
    let rabbit_connection = rabbit_factory::connect_to_rabbit(&amqp_params.rabbitmq_url).await?;
    //build up the names of queues and such
    //assert the dooot on the rabbit server (creates exchanges, queues, and bindings)
    rabbit_factory::assert_amqp_topology(&rabbit_connection, &queue_info).await?;

    // setup step utils
    let step_utils = Rc::new(StepUtils::new(args.ipfs_url));

    // Setup clickhouse, don't connect if clickhouse_writes is false;
    let clickhouse_client = if args.enable_clickhouse_writes {
        Some(args.clickhouse_params.client())
    } else {
        None
    };

    let term_signal_incoming = Arc::new(AtomicBool::new(false));
    let term_signal_dooot = Arc::new(AtomicBool::new(false));
    let term_signal_clickhouse = Arc::new(AtomicBool::new(false));

    let ch_period_bias_pct = args.clickhouse_params.clickhouse_period_bias_bps as f64 / 10000.0;
    let ch_max_flush_ms = args.clickhouse_params.clickhouse_max_flush_ms;

    //start our tasks to do work
    let ingestooor = ingestooor::Ingestooor::new(
        rabbit_connection,
        queue_info,
        clickhouse_client,
        ch_period_bias_pct,
        ch_max_flush_ms,
        amqp_params.debug_user.clone(),
        amqp_params.rabbitmq_prefetch.unwrap_or(1),
        step_utils,
        term_signal_incoming,
        term_signal_dooot,
        term_signal_clickhouse,
    )?;

    log::info!("Enable parsing: {}", args.enable_parsing);
    log::info!("Enable db writes: {}", args.enable_pg_writes);
    log::info!(
        "Enable clickhouse writes: {}",
        args.enable_clickhouse_writes
    );

    let fut_ingestooor = ingestooor.run(args.enable_parsing, args.enable_clickhouse_writes);

    //run the ingestooor!
    fut_ingestooor.await?;

    Ok(())
}
