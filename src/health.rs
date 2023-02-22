use anyhow::Context as _;
use futures_util::future::try_join_all;
use futures_util::stream::{AbortHandle, Abortable};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::UnixListenerStream;
use tracing::{debug, error, info, info_span, Instrument};

pub(crate) fn listen(
    socket_path: &str,
    health_senders: Vec<(&'static str, mpsc::Sender<oneshot::Sender<bool>>)>,
) -> anyhow::Result<(AbortHandle, JoinHandle<()>)> {
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    let listener = UnixListener::bind(socket_path).context("error binding unix domain socket")?;
    let unix_streams = UnixListenerStream::new(listener);
    let mut unix_streams = Abortable::new(unix_streams, abort_registration);

    let task = tokio::spawn(
        async move {
            info!(msg = "listening");

            while let Some(next) = unix_streams.next().await {
                let mut stream = match next {
                    Ok(stream) => stream,
                    Err(err) => {
                        error!(kind="socket connection", %err);
                        continue;
                    }
                };
                debug!(msg = "got connection");

                let health_tasks = health_senders
                    .iter()
                    .map(|(subsystem, health_tx)| async move {
                        let (tx, rx) = oneshot::channel();

                        health_tx.try_send(tx).map_err(|err| {
                            error!(during = "sending health request", subsystem, %err);
                            format!("error sending health request to `{subsystem}` component")
                        })?;

                        rx.await.map_err(|err| {
                            error!(kind = "outcome channel receiving", subsystem, %err);
                            format!("error receiving health outcome from `{subsystem}` component")
                        })?;

                        Ok(())
                    })
                    .collect::<Vec<_>>();
                let message = match try_join_all(health_tasks).await {
                    Ok(_) => "OK".to_string(),
                    Err(err) => err,
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
