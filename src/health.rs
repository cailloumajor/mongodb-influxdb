use std::collections::HashMap;
use std::sync::Arc;

use actix::prelude::*;
use anyhow::Context as _;
use futures_util::stream::{AbortRegistration, Abortable};
use futures_util::{FutureExt, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tracing::{debug, error, info, info_span, instrument, Instrument};

use mongodb_scraper::HEALTH_SOCKET_PATH;

pub(crate) type HealthResult = Result<(), String>;

#[derive(Message)]
#[rtype(result = "HealthResult")]
pub(crate) struct HealthPing;

#[derive(Message)]
#[rtype(result = "HealthResult")]
pub(crate) struct HealthQuery;

pub(crate) struct HealthService {
    registered: Arc<HashMap<&'static str, Recipient<HealthPing>>>,
}

impl HealthService {
    pub(crate) fn new<T>(targets: T) -> Self
    where
        T: IntoIterator<Item = (&'static str, Recipient<HealthPing>)>,
    {
        let registered = targets.into_iter().collect();
        let registered = Arc::new(registered);

        Self { registered }
    }
}

impl Actor for HealthService {
    type Context = Context<Self>;
}

impl Handler<HealthQuery> for HealthService {
    type Result = ResponseFuture<HealthResult>;

    fn handle(&mut self, _msg: HealthQuery, _ctx: &mut Self::Context) -> Self::Result {
        let registered = Arc::clone(&self.registered);
        async move {
            for (name, recipient) in registered.iter() {
                let pong = match recipient.send(HealthPing).await {
                    Ok(resp) => resp,
                    Err(err) => {
                        error!(kind="fatal", %err);
                        System::current().stop();
                        unreachable!()
                    }
                };
                if let Err(err) = pong {
                    return Err(format!("component `{}` is unhealthy: {}", name, err));
                }
            }

            Ok(())
        }
        .instrument(info_span!("health_handler"))
        .boxed()
    }
}

#[instrument(skip_all, name = "health_listen")]
pub(crate) async fn listen(
    service_addr: Addr<HealthService>,
    abort_reg: AbortRegistration,
) -> anyhow::Result<()> {
    let listener =
        UnixListener::bind(HEALTH_SOCKET_PATH).context("error binding unix domain socket")?;
    let unix_streams = UnixListenerStream::new(listener);
    let mut unix_streams = Abortable::new(unix_streams, abort_reg);

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

        let health_status = service_addr.send(HealthQuery).await?;
        let message = match health_status {
            Ok(_) => String::from("OK"),
            Err(err) => err,
        };

        if let Err(err) = stream.write_all(message.as_bytes()).await {
            error!(kind="write to stream", %err);
        }
        if let Err(err) = stream.shutdown().await {
            error!(kind="stream shutdown", %err);
        }
    }
    info!(msg = "aborted");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestActor {
        pong: HealthResult,
        must_stop: bool,
    }

    impl TestActor {
        fn new(pong: HealthResult, must_stop: bool) -> Self {
            Self { pong, must_stop }
        }
    }

    impl Actor for TestActor {
        type Context = Context<Self>;

        fn started(&mut self, ctx: &mut Self::Context) {
            if self.must_stop {
                ctx.stop();
            }
        }
    }

    impl Handler<HealthPing> for TestActor {
        type Result = HealthResult;

        fn handle(&mut self, _msg: HealthPing, _ctx: &mut Self::Context) -> Self::Result {
            self.pong.clone()
        }
    }

    mod handle_health_query {
        use super::*;

        #[actix::test]
        async fn all_healthy() {
            let first_recipient = TestActor::new(Ok(()), false).start().recipient();
            let second_recipient = TestActor::new(Ok(()), false).start().recipient();
            let targets = [("first", first_recipient), ("second", second_recipient)];
            let health_addr = HealthService::new(targets).start();

            let health_result = health_addr
                .send(HealthQuery)
                .await
                .expect("unexpected mailbox error");

            assert!(health_result.is_ok());
        }

        #[actix::test]
        async fn one_unhealthy() {
            let first_recipient = TestActor::new(Err("I'm ill".into()), false)
                .start()
                .recipient();
            let second_recipient = TestActor::new(Ok(()), false).start().recipient();
            let targets = [("first", first_recipient), ("second", second_recipient)];
            let health_addr = HealthService::new(targets).start();

            let health_result = health_addr
                .send(HealthQuery)
                .await
                .expect("unexpected mailbox error");

            assert_eq!(
                health_result.unwrap_err(),
                "component `first` is unhealthy: I'm ill"
            );
        }

        #[actix::test]
        async fn mailbox_error() {
            let first_recipient = TestActor::new(Ok(()), false).start().recipient();
            let second_recipient = TestActor::new(Ok(()), true).start().recipient();
            let targets = [("first", first_recipient), ("second", second_recipient)];
            let health_addr = HealthService::new(targets).start();

            let health_result = health_addr.send(HealthQuery).await;

            assert!(health_result.is_err());
        }
    }
}
