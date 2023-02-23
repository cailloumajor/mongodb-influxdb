use std::sync::Arc;

use clap::Args;
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, info_span, instrument, Instrument};
use trillium_tokio::TcpConnector;
use url::Url;

use crate::line_protocol::DataPoint;

type HttpClient = trillium_client::Client<TcpConnector>;

#[derive(Args)]
#[group(skip)]
pub(crate) struct Config {
    /// InfluxDB root URL
    #[arg(env, long, default_value = "http://influxdb:8086")]
    influxdb_url: Url,

    /// InfluxDB API token with write-buckets permission
    #[arg(env, long)]
    influxdb_api_token: String,

    /// InfluxDB organization name or ID
    #[arg(env, long)]
    influxdb_org: String,

    /// InfluxDB bucket to write to
    #[arg(env, long)]
    influxdb_bucket: String,
}

#[derive(Deserialize)]
struct WriteResponse {
    message: String,
}

pub(crate) struct Client {
    url: Url,
    bucket: String,
    org: String,
    auth_header: String,
    http_client: HttpClient,
}

impl Client {
    pub(crate) fn new(config: &Config) -> Self {
        let url = config.influxdb_url.to_owned();
        let bucket = config.influxdb_bucket.to_owned();
        let org = config.influxdb_org.to_owned();
        let auth_header = format!("Token {}", config.influxdb_api_token);
        let http_client = HttpClient::new().with_default_pool();

        Self {
            url,
            bucket,
            org,
            auth_header,
            http_client,
        }
    }

    #[instrument(skip_all, name = "influxdb_write")]
    async fn write(&self, line_protocol: String) -> Result<(), ()> {
        let mut url = self.url.join("/api/v2/write").unwrap();
        url.query_pairs_mut()
            .append_pair("bucket", &self.bucket)
            .append_pair("org", &self.org)
            .append_pair("precision", "s");

        let mut conn = match self
            .http_client
            .post(url)
            .with_header("Authorization", self.auth_header.to_owned())
            .with_header("Content-Type", "text/plain; charset=utf-8")
            .with_body(line_protocol)
            .await
        {
            Ok(conn) => conn,
            Err(err) => {
                error!(during="request send", %err);
                return Err(());
            }
        };

        let status_code = conn.status().unwrap();
        if !status_code.is_success() {
            error!(kind = "response status", %status_code);
        } else {
            return Ok(());
        }

        match conn.response_json().await {
            Ok(WriteResponse { message }) => {
                error!(kind = "InfluxDB error", message);
            }
            Err(err) => {
                error!(during="response deserializing", %err);
            }
        };
        Err(())
    }

    pub(crate) fn handle_data_points(
        self: Arc<Self>,
    ) -> (mpsc::Sender<Vec<DataPoint>>, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<Vec<DataPoint>>(1);

        let task = tokio::spawn(
            async move {
                info!(status = "started");

                while let Some(data_points) = rx.recv().await {
                    let line_protocol = data_points
                        .into_iter()
                        .map(|m| m.to_string())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let _ = self.write(line_protocol).await;
                }

                info!(status = "terminating");
            }
            .instrument(info_span!("influxdb_data_points_handler")),
        );

        (tx, task)
    }

    pub(crate) fn handle_health(
        self: Arc<Self>,
    ) -> (mpsc::Sender<oneshot::Sender<bool>>, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<oneshot::Sender<bool>>(1);

        let task = tokio::spawn(
            async move {
                info!(status = "started");

                while let Some(outcome_tx) = rx.recv().await {
                    debug!(msg = "request received");
                    let outcome = self.write(String::new()).await.is_ok();
                    if outcome_tx.send(outcome).is_err() {
                        error!(kind = "outcome channel sending");
                    }
                }

                info!(status = "terminating");
            }
            .instrument(info_span!("influxdb_health_handler")),
        );

        (tx, task)
    }
}
