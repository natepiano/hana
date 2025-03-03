use crate::prelude::*;
use crate::transport::provider::*;
use error_stack::{Report, ResultExt};
use std::fmt;
use std::time::Duration;
use tokio::net::TcpListener as TokioTcpListener;
use tokio::net::TcpStream;
use tracing::debug; // Added import for debug macro

const CONNECTION_MAX_ATTEMPTS: u8 = 15;
const CONNECTION_RETRY_DELAY: Duration = Duration::from_millis(200);
const DEFAULT_IP_PORT: &str = "127.0.0.1:3001";

pub struct TcpProvider;

impl TransportProvider for TcpProvider {
    type Transport = TcpTransport;
    type Connector = TcpConnector;
    type Listener = TcpListener;

    fn connector() -> Result<Self::Connector> {
        Ok(TcpConnector::default())
    }

    async fn listener() -> Result<Self::Listener> {
        TcpListener::bind_default().await
    }
}

/// A TCP-based transport implementation
pub struct TcpTransport {
    stream: TcpStream,
}

impl TcpTransport {
    /// Create a new TCP transport from an existing TcpStream
    pub fn new(stream: TcpStream) -> Self {
        Self { stream }
    }
}

impl Transport for TcpTransport {}

impl fmt::Debug for TcpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcpTransport")
            .field("peer_addr", &self.stream.peer_addr().ok())
            .finish()
    }
}

pub struct TcpListener {
    listener: TokioTcpListener,
}

impl TcpListener {
    pub async fn bind(addr: &str) -> Result<Self> {
        let listener = TokioTcpListener::bind(addr)
            .await
            .change_context(Error::Io)
            .attach_printable(format!("Failed to bind to {}", addr))?;

        Ok(Self { listener })
    }

    pub async fn bind_default() -> Result<Self> {
        Self::bind(DEFAULT_IP_PORT).await
    }
}

impl TransportListener for TcpListener {
    type Transport = TcpTransport;

    async fn accept(&self) -> Result<Self::Transport> {
        let (stream, _) = self
            .listener
            .accept()
            .await
            .change_context(Error::Io)
            .attach_printable("Failed to accept connection")?;

        Ok(TcpTransport::new(stream))
    }
}

// TCP connector implementation
pub struct TcpConnector {
    addr: String,
    max_attempts: u8,
    retry_delay: Duration,
}

impl TcpConnector {
    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            max_attempts: CONNECTION_MAX_ATTEMPTS,
            retry_delay: CONNECTION_RETRY_DELAY,
        }
    }

    pub fn default() -> Self {
        Self::new(DEFAULT_IP_PORT)
    }
}

impl TransportConnector for TcpConnector {
    type Transport = TcpTransport;

    async fn connect(&self) -> Result<Self::Transport> {
        debug!("Connecting via TCP to {}", self.addr);
        let mut attempts = 0;
        let stream = loop {
            match TcpStream::connect(&self.addr).await {
                Ok(stream) => break stream,
                Err(_) => {
                    attempts += 1;
                    if attempts >= self.max_attempts {
                        return Err(Report::new(Error::ConnectionTimeout).attach_printable(
                            format!("Failed to connect after {attempts} attempts"),
                        ));
                    }
                    debug!("Connection attempt {} failed, retrying...", attempts);
                    tokio::time::sleep(self.retry_delay).await;
                }
            }
        };

        Ok(TcpTransport::new(stream))
    }
}

use crate::impl_async_io_for_field;
impl_async_io_for_field!(TcpTransport, stream);
