/// Abstract UNIX domain socket for health messages passing.
pub const HEALTH_SOCKET_PATH: &str = concat!("\0", env!("CARGO_PKG_NAME"), "-health");
