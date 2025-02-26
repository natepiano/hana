pub mod endpoint;
mod error;
pub mod message;

pub use crate::endpoint::{Endpoint, HanaApp};
pub use crate::error::{Error, Result};
pub use crate::message::Instruction;
use error_stack::Report;
use std::time::Duration;
use tokio::net::TcpStream;
use tracing::debug;

const TCP_ADDR: &str = "127.0.0.1:3001";
const CONNECTION_MAX_ATTEMPTS: u8 = 15;
const CONNECTION_RETRY_DELAY: Duration = Duration::from_millis(200);

pub async fn connect() -> Result<TcpStream> {
    let mut attempts = 0;
    let stream = loop {
        match TcpStream::connect(TCP_ADDR).await {
            Ok(stream) => break stream,
            Err(_) => {
                attempts += 1;
                if attempts >= CONNECTION_MAX_ATTEMPTS {
                    return Err(Report::new(Error::ConnectionTimeout)
                        .attach_printable(format!("Failed to connect after {attempts} attempts")));
                }
                debug!("Connection attempt {} failed, retrying...", attempts);
                tokio::time::sleep(CONNECTION_RETRY_DELAY).await;
            }
        }
    };

    Ok(stream)
}
