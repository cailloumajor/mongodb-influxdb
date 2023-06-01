use anyhow::anyhow;
use tokio::io::AsyncReadExt;
use tokio::net::UnixStream;

use mongodb_influxdb::HEALTH_SOCKET_PATH;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut unix_stream = UnixStream::connect(HEALTH_SOCKET_PATH).await?;
    let mut health_status = String::new();
    unix_stream.read_to_string(&mut health_status).await?;
    let health_status = health_status.trim();
    if health_status == "OK" {
        Ok(())
    } else {
        Err(anyhow!(String::from(health_status)))
    }
}
