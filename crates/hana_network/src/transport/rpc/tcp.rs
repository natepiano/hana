use crate::prelude::*;
use crate::transport::provider::*;
use crate::transport::support::*;
use error_stack::ResultExt;
use std::fmt;
use tokio::net::TcpListener as TokioTcpListener;
use tokio::net::TcpStream;
use tracing::debug; // Added import for debug macro

const DEFAULT_IP_PORT: &str = "127.0.0.1:3001";

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
}

impl TcpConnector {
    pub fn new(addr: impl Into<String>) -> Self {
        Self { addr: addr.into() }
    }

    pub fn default() -> Self {
        Self::new(DEFAULT_IP_PORT)
    }
}

impl TransportConnector for TcpConnector {
    type Transport = TcpTransport;

    async fn connect(&self) -> Result<Self::Transport> {
        debug!("Connecting via TCP to {}", self.addr);

        let addr = self.addr.clone();

        let stream = connect_with_retry(
            || {
                let addr = addr.clone();
                async move { TcpStream::connect(&addr).await }
            },
            RetryConfig::default(),
            &self.addr,
        )
        .await?;

        Ok(TcpTransport::new(stream))
    }
}

crate::impl_async_io!(TcpTransport, stream);
