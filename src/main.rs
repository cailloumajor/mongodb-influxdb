use std::sync::Arc;

use anyhow::Context as _;
use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use futures_util::StreamExt;
use humantime::Duration;
use signal_hook::consts::TERM_SIGNALS;
use signal_hook::low_level::signal_name;
use signal_hook_tokio::Signals;
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument};
use tracing_log::LogTracer;

use mongodb_influxdb::HEALTH_SOCKET_PATH;

mod channel;
mod health;
mod influxdb;
mod line_protocol;
mod mongodb;

#[derive(Parser)]
struct Args {
    /// Scraping interval
    #[arg(env, long, default_value = "1m")]
    interval: Duration,

    #[command(flatten)]
    mongodb: mongodb::Config,

    #[command(flatten)]
    influxdb: influxdb::Config,

    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,
}

#[instrument(skip_all)]
async fn handle_signals(signals: Signals, shutdown_token: CancellationToken) {
    let mut signals_stream = signals.map(|signal| signal_name(signal).unwrap_or("unknown"));
    info!(status = "started");
    while let Some(signal) = signals_stream.next().await {
        info!(msg = "received signal", reaction = "shutting down", signal);
        shutdown_token.cancel();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(args.verbose.tracing_level())
        .init();

    LogTracer::init_with_filter(args.verbose.log_level_filter())?;

    let influxdb_client = Arc::new(influxdb::Client::new(&args.influxdb));
    let (data_points_tx, data_points_task) = influxdb_client.clone().handle_data_points();
    let (influxdb_health_tx, influxdb_health_task) = influxdb_client.handle_health();

    let shutdown_token = CancellationToken::new();

    let mongodb_collection = mongodb::Collection::create(&args.mongodb).await?;
    let mongodb_collection = Arc::new(mongodb_collection);
    let scrape_task = mongodb_collection.clone().periodic_scrape(
        args.interval.into(),
        data_points_tx,
        shutdown_token.clone(),
    );
    let (mongodb_health_tx, mongodb_health_task) = mongodb_collection.handle_health();

    let health_senders = vec![
        ("influxdb", influxdb_health_tx),
        ("mongodb", mongodb_health_tx),
    ];
    let health_listen_task =
        health::listen(HEALTH_SOCKET_PATH, health_senders, shutdown_token.clone())
            .context("error creating health listening task")?;

    let signals = Signals::new(TERM_SIGNALS).context("error registering termination signals")?;
    let signals_handle = signals.handle();
    let signals_task = tokio::spawn(handle_signals(signals, shutdown_token));

    tokio::try_join!(scrape_task, health_listen_task)
        .context("error joining scrape and/or health listen tasks")?;

    signals_handle.close();

    tokio::try_join!(
        signals_task,
        data_points_task,
        influxdb_health_task,
        mongodb_health_task
    )
    .context("error joining tasks")?;

    Ok(())
}
