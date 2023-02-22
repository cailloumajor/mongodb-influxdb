use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use anyhow::Context as _;
use clap::Args;
use futures_util::stream::{AbortHandle, Abortable};
use futures_util::{future, StreamExt, TryStreamExt};
use mongodb::bson::{bson, doc, Document};
use mongodb::options::{ClientOptions, EstimatedDocumentCountOptions, FindOptions};
use mongodb::Client;
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{self, MissedTickBehavior};
use tokio_stream::wrappers::IntervalStream;
use tracing::{error, info, info_span, instrument, warn, Instrument as _};

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
    pub id: String,
    pub val: Document,
    pub ts: Document,
    updated_since: u64,
}

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
        self: Arc<Self>,
        period: Duration,
        tags_age: Arc<Vec<String>>,
        data_points_tx: mpsc::Sender<Vec<DataPoint>>,
    ) -> (AbortHandle, JoinHandle<()>) {
        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        let mut interval = time::interval(period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut interval_stream = Abortable::new(IntervalStream::new(interval), abort_registration);
        let tick_interval = period.as_millis() as u64;

        let task = tokio::spawn(
            async move {
                info!(status = "started");

                while interval_stream.next().await.is_some() {
                    let projection = doc! {
                        "updatedSince": {
                            "$dateDiff": {
                                "startDate": "$updatedAt",
                                "endDate": "$$NOW",
                                "unit": "millisecond",
                            },
                        },
                        "val": true,
                        "ts": true,
                    };
                    let options = FindOptions::builder().projection(projection).build();
                    let cursor = match self.0.find(None, options).await {
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

                    let measurement = self.0.namespace().to_string();
                    let timestamp = UNIX_EPOCH
                        .elapsed()
                        .expect("system time is before Unix epoch")
                        .as_secs();
                    let data_points: Vec<_> = match docs
                        .into_iter()
                        .map(|doc| {
                            DataPoint::create(doc, measurement.clone(), &tags_age, timestamp)
                        })
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
        );

        (abort_handle, task)
    }

    pub(crate) fn handle_health(
        self: Arc<Self>,
    ) -> (mpsc::Sender<oneshot::Sender<bool>>, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<oneshot::Sender<bool>>(1);

        let task = tokio::spawn(
            async move {
                info!(status = "started");

                while let Some(outcome_tx) = rx.recv().await {
                    let options = EstimatedDocumentCountOptions::builder()
                        .max_time(Duration::from_secs(2))
                        .comment(bson!("healthcheck"))
                        .build();
                    let outcome = match self.0.estimated_document_count(options).await {
                        Ok(_) => true,
                        Err(err) => {
                            error!(kind = "estimated document count", %err);
                            false
                        }
                    };
                    if outcome_tx.send(outcome).is_err() {
                        error!(kind = "outcome channel sending");
                    }
                }

                info!(status = "terminating");
            }
            .instrument(info_span!("mongodb_health_handler")),
        );

        (tx, task)
    }
}
