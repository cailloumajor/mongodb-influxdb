use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use actix::prelude::*;
use anyhow::Context as _;
use clap::Args;
use futures_util::{future, FutureExt, TryStreamExt};
use mongodb::bson::{bson, doc, Document};
use mongodb::options::{ClientOptions, EstimatedDocumentCountOptions, FindOptions};
use mongodb::{Client, Collection};
use serde::Deserialize;
use tracing::{error, info, info_span, instrument, warn, Instrument as _};

use crate::health::{HealthPing, HealthResult};
use crate::influxdb::DataPoints;
use crate::line_protocol::{DataPoint, DataPointCreateError};

type ModelCollection = Collection<DataDocument>;

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
    pub data: Document,
    pub source_timestamps: Document,
    updated_since: u64,
}

#[instrument(skip_all)]
pub(crate) async fn create_collection(config: &Config) -> anyhow::Result<ModelCollection> {
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
    Ok(collection)
}

pub(crate) struct MongoDBActor {
    pub collection: ModelCollection,
    pub tick_interval: Duration,
    pub tags_age: Arc<Vec<String>>,
    pub data_points_recipient: Recipient<DataPoints>,
}

impl Actor for MongoDBActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(self.tick_interval, |_this, ctx| {
            ctx.notify(Tick);
        });
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct Tick;

impl Handler<Tick> for MongoDBActor {
    type Result = ResponseFuture<()>;

    fn handle(&mut self, _msg: Tick, _ctx: &mut Self::Context) -> Self::Result {
        let collection = self.collection.clone();
        let tick_interval = self.tick_interval.as_millis() as u64;
        let tags_age = Arc::clone(&self.tags_age);
        let recipient = self.data_points_recipient.clone();

        async move {
            let projection = doc! {
                "updatedSince": {
                    "$dateDiff": {
                        "startDate": "$updatedAt",
                        "endDate": "$$NOW",
                        "unit": "millisecond",
                    },
                },
                "data": true,
                "sourceTimestamps": true,
            };
            let options = FindOptions::builder().projection(projection).build();
            let cursor = match collection.find(None, options).await {
                Ok(cursor) => cursor,
                Err(err) => {
                    error!(kind="find in collection", %err);
                    return;
                }
            };
            let filtered_cursor = cursor.try_filter(|document| {
                let fresh = document.updated_since < tick_interval;
                if !fresh {
                    warn!(kind = "outdated data", document.id);
                }
                future::ready(fresh)
            });
            let docs: Vec<_> = match filtered_cursor.try_collect().await {
                Ok(docs) => docs,
                Err(err) => {
                    error!(kind="collecting documents", %err);
                    return;
                }
            };

            let measurement = collection.namespace().to_string();
            let timestamp = UNIX_EPOCH
                .elapsed()
                .expect("system time is before Unix epoch")
                .as_secs();
            let data_points: Vec<_> = match docs
                .into_iter()
                .map(|doc| DataPoint::create(doc, measurement.clone(), &tags_age, timestamp))
                .collect()
            {
                Ok(vec) => vec,
                Err(DataPointCreateError { doc_id, field, msg }) => {
                    error!(during = "DataPoint::new", doc_id, field, msg);
                    return;
                }
            };
            if let Err(err) = recipient.try_send(DataPoints(data_points)) {
                error!(during="sending data points", %err);
            }
        }
        .instrument(info_span!("tick_handler"))
        .boxed()
    }
}

impl Handler<HealthPing> for MongoDBActor {
    type Result = ResponseFuture<HealthResult>;

    fn handle(&mut self, _msg: HealthPing, ctx: &mut Self::Context) -> Self::Result {
        let collection = self.collection.clone();
        let state = ctx.state();
        let options = EstimatedDocumentCountOptions::builder()
            .max_time(Duration::from_secs(2))
            .comment(bson!("healthcheck"))
            .build();

        async move {
            if state != ActorState::Running {
                return Err(format!("actor is in `{:?} state`", state));
            }

            if let Err(err) = collection.estimated_document_count(options).await {
                error!(kind="estimated document count", %err);
                return Err("estimated document count query error".into());
            }

            Ok(())
        }
        .instrument(info_span!("mongodb_health_handler"))
        .boxed()
    }
}
