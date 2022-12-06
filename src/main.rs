use actix::{Actor, Arbiter};
use anyhow::{ensure, Context as _};
use clap::Parser;
use clap_verbosity_flag::{InfoLevel, LogLevel, Verbosity};
use futures_util::future::AbortHandle;
use futures_util::StreamExt;
use humantime::Duration;
use signal_hook::consts::TERM_SIGNALS;
use signal_hook::low_level::signal_name;
use signal_hook_tokio::Signals;
use tracing::{info, instrument};
use tracing_log::LogTracer;

mod health;
mod influxdb;
mod line_protocol;
mod mongodb;

#[derive(Parser)]
struct Args {
    #[arg(env, long, default_value = "1m")]
    interval: Duration,

    #[command(flatten)]
    mongodb: mongodb::Config,

    #[command(flatten)]
    influxdb: influxdb::Config,

    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,
}

fn filter_from_verbosity<T>(verbosity: &Verbosity<T>) -> tracing::level_filters::LevelFilter
where
    T: LogLevel,
{
    use tracing_log::log::LevelFilter;
    match verbosity.log_level_filter() {
        LevelFilter::Off => tracing::level_filters::LevelFilter::OFF,
        LevelFilter::Error => tracing::level_filters::LevelFilter::ERROR,
        LevelFilter::Warn => tracing::level_filters::LevelFilter::WARN,
        LevelFilter::Info => tracing::level_filters::LevelFilter::INFO,
        LevelFilter::Debug => tracing::level_filters::LevelFilter::DEBUG,
        LevelFilter::Trace => tracing::level_filters::LevelFilter::TRACE,
    }
}

#[instrument(skip_all)]
async fn handle_signals(signals: Signals, abort_handle: AbortHandle) {
    let mut signals_stream = signals.map(|signal| signal_name(signal).unwrap_or("unknown"));
    while let Some(signal) = signals_stream.next().await {
        info!(signal, msg = "received signal, finishing");
        abort_handle.abort();
    }
}

#[actix::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(filter_from_verbosity(&args.verbose))
        .init();

    LogTracer::init_with_filter(args.verbose.log_level_filter())?;

    let signals = Signals::new(TERM_SIGNALS).context("error registering termination signals")?;
    let signals_handle = signals.handle();
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    let sent = Arbiter::current().spawn(handle_signals(signals, abort_handle));
    ensure!(sent, "error spawning signals handler");

    let influxdb_client = influxdb::Client::new(&args.influxdb);
    let influxdb_addr = influxdb::InfluxDBActor::new(influxdb_client).start();

    let collection = mongodb::create_collection(&args.mongodb).await?;
    let mongodb_addr = {
        let actor = mongodb::MongoDBActor {
            collection,
            tick_interval: args.interval.into(),
            data_points_recipient: influxdb_addr.clone().recipient(),
        };
        actor.start()
    };

    let targets = [
        ("influxdb", influxdb_addr.recipient()),
        ("mongodb", mongodb_addr.recipient()),
    ];
    let health_addr = health::HealthService::new(targets).start();

    health::listen(health_addr, abort_registration).await?;

    signals_handle.close();

    Ok(())
}
