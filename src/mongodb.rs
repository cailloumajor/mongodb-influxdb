use std::time::{Duration, UNIX_EPOCH};

use anyhow::Context as _;
use clap::Args;
use futures_util::{future, StreamExt, TryStreamExt};
use mongodb::bson::{bson, doc, Document};
use mongodb::options::{ClientOptions, EstimatedDocumentCountOptions, FindOptions};
use mongodb::Client;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{self, MissedTickBehavior};
use tokio_stream::wrappers::IntervalStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, info_span, instrument, warn, Instrument as _};

use crate::channel::roundtrip_channel;
use crate::health::HealthChannel;
use crate::line_protocol::{DataPoint, DataPointCreateError};

const APP_NAME: &str = concat!(env!("CARGO_PKG_NAME"), " (", env!("CARGO_PKG_VERSION"), ")");

#[derive(Args)]
#[group(skip)]
pub(crate) struct Config {
    /// URI of MongoDB server
    #[arg(env, long, default_value = "mongodb://mongodb")]
    mongodb_uri: String,

    /// MongoDB database
    #[arg(env, long)]
    mongodb_database: String,

    /// MongoDB collection
    #[arg(env, long)]
    mongodb_collection: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DataDocument {
    #[serde(rename = "_id")]
    pub(crate) id: String,
    pub(crate) val: Document,
    updated_since: u64,
}

#[derive(Clone)]
pub(crate) struct Collection(mongodb::Collection<DataDocument>);

impl Collection {
    #[instrument(name = "mongodb_collection_create", skip_all)]
    pub(crate) async fn create(config: &Config) -> anyhow::Result<Self> {
        let mut options = ClientOptions::parse(&config.mongodb_uri)
            .await
            .context("error parsing connection string URI")?;
        options.app_name = String::from(APP_NAME).into();
        options.server_selection_timeout = Duration::from_secs(2).into();
        let client = Client::with_options(options).context("error creating the client")?;
        let collection = client
            .database(&config.mongodb_database)
            .collection(&config.mongodb_collection);

        info!(status = "success");
        Ok(Self(collection))
    }

    pub(crate) fn periodic_scrape(
        &self,
        period: Duration,
        data_points_tx: mpsc::Sender<Vec<DataPoint>>,
        shutdown_token: CancellationToken,
    ) -> JoinHandle<()> {
        let mut interval = time::interval(period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut interval_stream = IntervalStream::new(interval)
            .take_until(shutdown_token.cancelled_owned())
            .boxed();
        let tick_interval = period.as_millis() as u64;
        let projection = doc! {
            "updatedSince": {
                "$dateDiff": {
                    "startDate": "$updatedAt",
                    "endDate": "$$NOW",
                    "unit": "millisecond",
                },
            },
            "val": true,
        };
        let options = FindOptions::builder().projection(projection).build();
        let cloned_self = self.clone();

        tokio::spawn(
            async move {
                info!(status = "started");

                while interval_stream.next().await.is_some() {
                    let cursor = match cloned_self.0.find(None, options.clone()).await {
                        Ok(cursor) => cursor,
                        Err(err) => {
                            error!(kind="find in collection", %err);
                            continue;
                        }
                    };
                    let filtered_cursor = cursor.try_filter(|document| {
                        let fresh = document.updated_since < tick_interval;
                        if !fresh {
                            warn!(kind = "outdated data", document.id);
                        }
                        future::ready(fresh)
                    });
                    let docs = match filtered_cursor.try_collect::<Vec<_>>().await {
                        Ok(docs) => docs,
                        Err(err) => {
                            error!(kind="collecting documents", %err);
                            continue;
                        }
                    };

                    let measurement = cloned_self.0.namespace().to_string();
                    let timestamp = UNIX_EPOCH
                        .elapsed()
                        .expect("system time is before Unix epoch")
                        .as_secs();
                    let data_points: Vec<_> = match docs
                        .into_iter()
                        .map(|doc| DataPoint::create(doc, measurement.clone(), timestamp))
                        .collect()
                    {
                        Ok(vec) => vec,
                        Err(DataPointCreateError { doc_id, field, msg }) => {
                            error!(during = "DataPoint::new", doc_id, field, msg);
                            continue;
                        }
                    };
                    if let Err(err) = data_points_tx.try_send(data_points) {
                        error!(during="sending data points", %err);
                    }
                }

                info!(status = "terminating");
            }
            .instrument(info_span!("mongodb_periodic_scrape")),
        )
    }

    pub(crate) fn handle_health(&self) -> (HealthChannel, JoinHandle<()>) {
        let (tx, mut rx) = roundtrip_channel(1);
        let options = EstimatedDocumentCountOptions::builder()
            .max_time(Duration::from_secs(2))
            .comment(bson!("healthcheck"))
            .build();
        let cloned_self = self.clone();

        let task = tokio::spawn(
            async move {
                info!(status = "started");

                while let Some((_, reply_tx)) = rx.recv().await {
                    debug!(msg = "request received");
                    let outcome = match cloned_self
                        .0
                        .estimated_document_count(options.clone())
                        .await
                    {
                        Ok(_) => true,
                        Err(err) => {
                            error!(kind = "estimated document count", %err);
                            false
                        }
                    };
                    if reply_tx.send(outcome).is_err() {
                        error!(kind = "reply channel sending");
                    }
                }

                info!(status = "terminating");
            }
            .instrument(info_span!("mongodb_health_handler")),
        );

        (tx, task)
    }
}
