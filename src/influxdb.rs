use std::io::Write;
use std::sync::Arc;

use clap::Args;
use flate2::write::GzEncoder;
use reqwest::{Client as HttpClient, header};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::{JoinHandle, spawn_blocking};
use tracing::{Instrument, debug, error, info, info_span, instrument};
use url::Url;

use crate::channel::roundtrip_channel;
use crate::health::HealthChannel;
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
    bucket: Arc<str>,
    org: Arc<str>,
    auth_header: Arc<str>,
    http_client: HttpClient,
}

impl Client {
    pub(crate) fn new(config: &Config) -> Self {
        let url = config.influxdb_url.clone();
        let bucket = Arc::from(config.influxdb_bucket.as_str());
        let org = Arc::from(config.influxdb_org.as_str());
        let auth_header = Arc::from(format!("Token {}", config.influxdb_api_token).as_str());
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

        let body = spawn_blocking(move || {
            let mut encoder = GzEncoder::new(Vec::new(), Default::default());
            encoder.write_all(line_protocol.as_bytes())?;
            encoder.finish()
        })
        .await
        .map_err(|err| {
            error!(when = "joining compression task", %err);
        })?
        .map_err(|err| {
            error!(when = "compressing body", %err);
        })?;

        let resp = self
            .http_client
            .post(url)
            .header(header::AUTHORIZATION, self.auth_header.as_ref())
            .header(header::CONTENT_ENCODING, "gzip")
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(body)
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

    pub(crate) fn handle_health(&self) -> (HealthChannel, JoinHandle<()>) {
        let (tx, mut rx) = roundtrip_channel(1);
        let cloned_self = self.clone();

        let task = tokio::spawn(
            async move {
                info!(status = "started");

                while let Some((_, reply_tx)) = rx.recv().await {
                    debug!(msg = "request received");
                    let outcome = cloned_self.write(String::new()).await.is_ok();
                    if reply_tx.send(outcome).is_err() {
                        error!(kind = "reply channel sending");
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
                    .match_header("Content-Encoding", "gzip")
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
                // Generated under linux, using `printf "some_line_protocol" | gzip | xxd -i`,
                // and replacing the `OS` byte (see RFC1952, section 2.3.1) with 255 (unknown).
                let expected_body = vec![
                    0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x2b, 0xce, 0xcf,
                    0x4d, 0x8d, 0xcf, 0xc9, 0xcc, 0x4b, 0x8d, 0x2f, 0x28, 0xca, 0x2f, 0xc9, 0x4f,
                    0xce, 0xcf, 0x01, 0x00, 0xd5, 0x03, 0x68, 0xd5, 0x12, 0x00, 0x00, 0x00,
                ];
                let mut server = Server::new_async().await;
                let mock = server
                    .mock("POST", "/api/v2/write")
                    .match_query(Matcher::AllOf(vec![
                        Matcher::UrlEncoded("bucket".into(), "somebucket".into()),
                        Matcher::UrlEncoded("org".into(), "someorg".into()),
                        Matcher::UrlEncoded("precision".into(), "s".into()),
                    ]))
                    .match_header("Authorization", "Token sometoken")
                    .match_header("Content-Encoding", "gzip")
                    .match_header("Content-Type", "text/plain; charset=utf-8")
                    .match_body(expected_body)
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
