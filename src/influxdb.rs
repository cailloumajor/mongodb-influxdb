use arcstr::ArcStr;
use clap::Args;
use reqwest::{header, Client as HttpClient};
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, info_span, instrument, Instrument};
use url::Url;

use crate::line_protocol::DataPoint;

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

#[derive(Clone)]
pub(crate) struct Client {
    url: Url,
    bucket: ArcStr,
    org: ArcStr,
    auth_header: ArcStr,
    http_client: HttpClient,
}

impl Client {
    pub(crate) fn new(config: &Config) -> Self {
        let url = config.influxdb_url.clone();
        let bucket = ArcStr::from(&config.influxdb_bucket);
        let org = ArcStr::from(&config.influxdb_org);
        let auth_header = ArcStr::from(format!("Token {}", config.influxdb_api_token));
        let http_client = HttpClient::new();

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

        let resp = self
            .http_client
            .post(url)
            .header(header::AUTHORIZATION, self.auth_header.as_str())
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(line_protocol)
            .send()
            .await
            .map_err(|err| {
                error!(kind = "request sending", %err);
            })?;

        let status_code = resp.status();
        if !status_code.is_success() {
            let message = resp
                .json()
                .await
                .map(|WriteResponse { message }| message)
                .unwrap_or_default();
            error!(kind = "response status", %status_code, message);
            return Err(());
        }

        Ok(())
    }

    pub(crate) fn handle_data_points(&self) -> (mpsc::Sender<Vec<DataPoint>>, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<Vec<DataPoint>>(1);
        let cloned_self = self.clone();

        let task = tokio::spawn(
            async move {
                info!(status = "started");

                while let Some(data_points) = rx.recv().await {
                    let line_protocol = data_points
                        .into_iter()
                        .map(|m| m.to_string())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let _ = cloned_self.write(line_protocol).await;
                }

                info!(status = "terminating");
            }
            .instrument(info_span!("influxdb_data_points_handler")),
        );

        (tx, task)
    }

    pub(crate) fn handle_health(&self) -> (mpsc::Sender<oneshot::Sender<bool>>, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<oneshot::Sender<bool>>(1);
        let cloned_self = self.clone();

        let task = tokio::spawn(
            async move {
                info!(status = "started");

                while let Some(outcome_tx) = rx.recv().await {
                    debug!(msg = "request received");
                    let outcome = cloned_self.write(String::new()).await.is_ok();
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

#[cfg(test)]
mod tests {
    use super::*;

    mod client {
        use super::*;

        mod write {
            use mockito::{Matcher, Server};

            use super::*;

            #[tokio::test]
            async fn request_send_failure() {
                let server = Server::new_async().await;
                let config = Config {
                    influxdb_url: server.url().parse().unwrap(),
                    influxdb_api_token: "\0".to_string(),
                    influxdb_org: "someorg".to_string(),
                    influxdb_bucket: "somebucket".to_string(),
                };
                let client = Client::new(&config);
                let result = client.write("some_line_protocol".to_string()).await;
                assert!(result.is_err());
            }

            #[tokio::test]
            async fn bad_status_code() {
                let mut server = Server::new_async().await;
                let mock = server
                    .mock("POST", "/api/v2/write")
                    .match_query(Matcher::AllOf(vec![
                        Matcher::UrlEncoded("bucket".into(), "somebucket".into()),
                        Matcher::UrlEncoded("org".into(), "someorg".into()),
                        Matcher::UrlEncoded("precision".into(), "s".into()),
                    ]))
                    .match_header("Authorization", "Token sometoken")
                    .match_header("Content-Type", "text/plain; charset=utf-8")
                    .with_status(500)
                    .create_async()
                    .await;
                let config = Config {
                    influxdb_url: server.url().parse().unwrap(),
                    influxdb_api_token: "sometoken".to_string(),
                    influxdb_org: "someorg".to_string(),
                    influxdb_bucket: "somebucket".to_string(),
                };
                let client = Client::new(&config);
                let result = client.write("some_line_protocol".to_string()).await;
                mock.assert_async().await;
                assert!(result.is_err());
            }

            #[tokio::test]
            async fn success() {
                let mut server = Server::new_async().await;
                let mock = server
                    .mock("POST", "/api/v2/write")
                    .match_query(Matcher::AllOf(vec![
                        Matcher::UrlEncoded("bucket".into(), "somebucket".into()),
                        Matcher::UrlEncoded("org".into(), "someorg".into()),
                        Matcher::UrlEncoded("precision".into(), "s".into()),
                    ]))
                    .match_header("Authorization", "Token sometoken")
                    .match_header("Content-Type", "text/plain; charset=utf-8")
                    .create_async()
                    .await;
                let config = Config {
                    influxdb_url: server.url().parse().unwrap(),
                    influxdb_api_token: "sometoken".to_string(),
                    influxdb_org: "someorg".to_string(),
                    influxdb_bucket: "somebucket".to_string(),
                };
                let client = Client::new(&config);
                let result = client.write("some_line_protocol".to_string()).await;
                mock.assert_async().await;
                assert!(result.is_ok());
            }
        }
    }
}
