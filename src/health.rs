use std::path::Path;

use anyhow::Context as _;
use futures_util::StreamExt;
use futures_util::future;
use futures_util::stream::FuturesUnordered;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::UnixListenerStream;
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, debug, error, info, info_span};

use crate::channel::RoundtripSender;

pub(crate) type HealthChannel = RoundtripSender<(), bool>;

pub(crate) fn listen(
    socket_path: impl AsRef<Path>,
    health_channels: Vec<(&'static str, HealthChannel)>,
    shutdown_token: CancellationToken,
) -> anyhow::Result<JoinHandle<()>> {
    let listener = UnixListener::bind(socket_path).context("error binding socket")?;
    let mut unix_streams = UnixListenerStream::new(listener)
        .filter_map(|item| {
            future::ready(match item {
                Ok(unix_stream) => Some(unix_stream),
                Err(err) => {
                    error!(during = "unix listener accept", %err);
                    None
                }
            })
        })
        .take_until(shutdown_token.cancelled_owned())
        .boxed();

    Ok(tokio::spawn(
        async move {
            info!(msg = "listening");

            while let Some(mut stream) = unix_streams.next().await {
                debug!(msg = "got connection");

                let errors = health_channels
                    .iter()
                    .map(async |(subsystem, health_channel)| {
                        match health_channel.roundtrip(()).await {
                            Ok(true) => None,
                            Ok(false) => Some(format!("component `{subsystem}` is unhealthy")),
                            Err(err) => {
                                error!(during = "health channel roundtrip", subsystem, %err);
                                Some(format!(
                                    "component `{subsystem}`: health channel roundtrip error"
                                ))
                            }
                        }
                    })
                    .collect::<FuturesUnordered<_>>()
                    .filter_map(future::ready)
                    .collect::<Vec<_>>()
                    .await;
                let message = if errors.is_empty() {
                    "OK".to_string()
                } else {
                    errors.join(" & ")
                };

                if let Err(err) = stream.write_all(message.as_bytes()).await {
                    error!(kind="write to stream", %err);
                }
                if let Err(err) = stream.shutdown().await {
                    error!(kind="stream shutdown", %err);
                }
            }

            info!(msg = "terminating");
        }
        .instrument(info_span!("health_listen")),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod listen {
        use tokio::io::AsyncReadExt;
        use tokio::net::UnixStream;
        use tokio::sync::oneshot;

        use crate::channel::roundtrip_channel;

        use super::*;

        fn health_task(outcome: Result<bool, ()>) -> HealthChannel {
            let (tx, mut rx) = roundtrip_channel(1);
            let cloned_tx = tx.clone();
            tokio::spawn(async move {
                let outcome = match outcome {
                    Ok(healthy) => healthy,
                    Err(_) => {
                        let (reply_tx, _) = oneshot::channel();
                        cloned_tx.send((), reply_tx).await;
                        return;
                    }
                };
                while let Some((_, reply_tx)) = rx.recv().await {
                    reply_tx.send(outcome).unwrap();
                }
            });
            tx
        }

        #[tokio::test]
        async fn bind_error() {
            let socket_path: String = ['\0'; 150].iter().collect();

            assert!(listen(socket_path, vec![], CancellationToken::new()).is_err());
        }

        #[tokio::test]
        async fn unhealthy() {
            let senders = vec![
                ("first", health_task(Ok(true))),
                ("second", health_task(Ok(false))),
                ("third", health_task(Err(()))),
            ];
            listen("\0unhealthy_test", senders, CancellationToken::new()).unwrap();
            let mut unix_stream = UnixStream::connect("\0unhealthy_test").await.unwrap();
            let mut health_status = String::new();
            unix_stream
                .read_to_string(&mut health_status)
                .await
                .unwrap();
            assert!(health_status.contains("component `second` is unhealthy"));
            assert!(health_status.contains("component `third`: health channel roundtrip error"));
        }

        #[tokio::test]
        async fn healthy() {
            let senders = vec![
                ("first", health_task(Ok(true))),
                ("second", health_task(Ok(true))),
            ];
            listen("\0healthy_test", senders, CancellationToken::new()).unwrap();
            let mut unix_stream = UnixStream::connect("\0healthy_test").await.unwrap();
            let mut health_status = String::new();
            unix_stream
                .read_to_string(&mut health_status)
                .await
                .unwrap();
            assert_eq!(health_status, "OK");
        }
    }
}
