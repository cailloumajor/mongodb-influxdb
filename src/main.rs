use actix::{Actor, System};
use anyhow::Context as _;
use clap::Parser;
use clap_verbosity_flag::{InfoLevel, LogLevel, Verbosity};
use humantime::Duration;
use tracing_log::LogTracer;

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

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(filter_from_verbosity(&args.verbose))
        .init();

    LogTracer::init_with_filter(args.verbose.log_level_filter())?;

    let system = System::new();

    let influxdb_client = influxdb::Client::new(&args.influxdb);
    let influxdb_addr = system.block_on(async {
        let actor = influxdb::InfluxDBActor { influxdb_client };
        actor.start()
    });

    let collection = system.block_on(mongodb::create_collection(&args.mongodb))?;
    system.block_on(async {
        let actor = mongodb::MongoDBActor {
            collection,
            tick_interval: args.interval.into(),
            data_points_recipient: influxdb_addr.recipient(),
        };
        actor.start();
    });

    system.run().context("error running system")?;

    Ok(())
}
