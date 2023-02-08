use std::sync::Arc;

use actix::prelude::*;
use clap::Args;
use futures_util::FutureExt;
use serde::Deserialize;
use tracing::{error, info_span, instrument, Instrument};
use trillium_tokio::TcpConnector;
use url::Url;

use crate::health::{HealthPing, HealthResult};
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
    async fn write(&self, line_protocol: String) -> bool {
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
                return false;
            }
        };

        let status_code = conn.status().unwrap();
        if !status_code.is_success() {
            error!(kind = "response status", %status_code);
        } else {
            return true;
        }

        match conn.response_json().await {
            Ok(WriteResponse { message }) => {
                error!(kind = "InfluxDB error", message);
            }
            Err(err) => {
                error!(during="response deserializing", %err);
            }
        };
        false
    }
}

pub(crate) struct InfluxDBActor {
    influxdb_client: Arc<Client>,
}

impl InfluxDBActor {
    pub(crate) fn new(client: Client) -> Self {
        Self {
            influxdb_client: Arc::new(client),
        }
    }
}

impl Actor for InfluxDBActor {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "()")]
pub(crate) struct DataPoints(pub Vec<DataPoint>);

impl Handler<DataPoints> for InfluxDBActor {
    type Result = ResponseFuture<()>;

    fn handle(&mut self, msg: DataPoints, _ctx: &mut Self::Context) -> Self::Result {
        let client = Arc::clone(&self.influxdb_client);
        let line_protocol = msg
            .0
            .into_iter()
            .map(|m| m.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        async move {
            client.write(line_protocol).await;
        }
        .instrument(info_span!("data_points_handler"))
        .boxed()
    }
}

impl Handler<HealthPing> for InfluxDBActor {
    type Result = ResponseFuture<HealthResult>;

    fn handle(&mut self, _msg: HealthPing, ctx: &mut Self::Context) -> Self::Result {
        let client = Arc::clone(&self.influxdb_client);
        let state = ctx.state();

        async move {
            if state != ActorState::Running {
                return Err(format!("actor is in `{state:?} state`"));
            }

            let success = client.write(String::new()).await;
            if !success {
                return Err("write error".into());
            }

            Ok(())
        }
        .boxed()
    }
}
