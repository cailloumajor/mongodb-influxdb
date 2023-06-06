use std::path::Path;

use anyhow::Context as _;
use futures_util::future;
use futures_util::stream::{AbortHandle, Abortable, FuturesUnordered};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::UnixListenerStream;
use tracing::{debug, error, info, info_span, instrument, Instrument};

pub(crate) type HealthResponder = oneshot::Sender<bool>;

#[instrument(skip_all)]
async fn health_roundtrip(
    subsystem: &str,
    health_tx: &mpsc::Sender<HealthResponder>,
) -> Result<(), String> {
    let (tx, rx) = oneshot::channel();

    health_tx.try_send(tx).map_err(|err| {
        error!(during = "sending health request", subsystem, %err);
        format!("error sending health request to `{subsystem}` component")
    })?;

    let success = rx.await.map_err(|err| {
        error!(kind = "outcome channel receiving", subsystem, %err);
        format!("error receiving health outcome from `{subsystem}` component")
    })?;

    if success {
        Ok(())
    } else {
        Err(format!("component `{subsystem}` is unhealthy"))
    }
}

pub(crate) fn listen(
    socket_path: impl AsRef<Path>,
    health_senders: Vec<(&'static str, mpsc::Sender<HealthResponder>)>,
) -> anyhow::Result<(AbortHandle, JoinHandle<()>)> {
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    let listener = UnixListener::bind(socket_path).context("error binding socket")?;
    let unix_streams = UnixListenerStream::new(listener).filter_map(|item| {
        future::ready(match item {
            Ok(unix_stream) => Some(unix_stream),
            Err(err) => {
                error!(during = "unix listener accept", %err);
                None
            }
        })
    });
    let mut unix_streams = Abortable::new(unix_streams, abort_registration);

    let task = tokio::spawn(
        async move {
            info!(msg = "listening");

            while let Some(mut stream) = unix_streams.next().await {
                debug!(msg = "got connection");

                let errors = health_senders
                    .iter()
                    .map(|(subsystem, health_tx)| health_roundtrip(subsystem, health_tx))
                    .collect::<FuturesUnordered<_>>()
                    .filter_map(|result| {
                        future::ready(match result {
                            Ok(_) => None,
                            Err(err) => Some(err),
                        })
                    })
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
    );

    Ok((abort_handle, task))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod health_roundtrip {
        use super::*;

        #[tokio::test]
        async fn request_sending_error() {
            let (tx, _) = mpsc::channel(1);
            assert!(health_roundtrip("test", &tx).await.is_err());
        }

        #[tokio::test]
        async fn outcome_channel_receiving_error() {
            let (tx, mut rx) = mpsc::channel(1);
            tokio::spawn(async move {
                let _ = rx.recv().await.unwrap();
            });
            assert!(health_roundtrip("test", &tx).await.is_err());
        }

        #[tokio::test]
        async fn unhealthy_subsystem() {
            let (tx, mut rx) = mpsc::channel::<HealthResponder>(1);
            tokio::spawn(async move {
                let outcome_tx = rx.recv().await.unwrap();
                outcome_tx.send(false).unwrap();
            });
            assert!(health_roundtrip("test", &tx).await.is_err());
        }

        #[tokio::test]
        async fn success() {
            let (tx, mut rx) = mpsc::channel::<HealthResponder>(1);
            tokio::spawn(async move {
                let outcome_tx = rx.recv().await.unwrap();
                outcome_tx.send(true).unwrap();
            });
            assert!(health_roundtrip("test", &tx).await.is_ok());
        }
    }

    mod listen {
        use tokio::io::AsyncReadExt;
        use tokio::net::UnixStream;

        use super::*;

        fn health_task(outcome: bool) -> mpsc::Sender<HealthResponder> {
            let (tx, mut rx) = mpsc::channel::<HealthResponder>(1);
            tokio::spawn(async move {
                while let Some(outcome_tx) = rx.recv().await {
                    outcome_tx.send(outcome).unwrap();
                }
            });
            tx
        }

        #[tokio::test]
        async fn bind_error() {
            let mut socket_path = String::new();
            for _ in 0..150 {
                socket_path.push('\0');
            }
            let senders = Vec::new();

            assert!(listen(socket_path, senders).is_err());
        }

        #[tokio::test]
        async fn unhealthy() {
            let senders = vec![
                ("first", health_task(true)),
                ("second", health_task(false)),
                ("third", health_task(false)),
            ];
            listen("\0unhealthy_test", senders).unwrap();
            let mut unix_stream = UnixStream::connect("\0unhealthy_test").await.unwrap();
            let mut health_status = String::new();
            unix_stream
                .read_to_string(&mut health_status)
                .await
                .unwrap();
            assert!(health_status.contains("component `second` is unhealthy"));
            assert!(health_status.contains("component `third` is unhealthy"));
        }

        #[tokio::test]
        async fn healthy() {
            let senders = vec![("first", health_task(true)), ("second", health_task(true))];
            listen("\0healthy_test", senders).unwrap();
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
