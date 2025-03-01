use std::fmt;
use std::time::Duration;

use super::{TransportConnector, TransportListener};
use crate::prelude::*;
use crate::transport::Transport;
use error_stack::{Report, ResultExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener as TokioTcpListener;
use tokio::net::TcpStream;
use tracing::debug; // Added import for debug macro

const CONNECTION_MAX_ATTEMPTS: u8 = 15;
const CONNECTION_RETRY_DELAY: Duration = Duration::from_millis(200);

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
        Self::bind("127.0.0.1:3001").await
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
        Self::new("127.0.0.1:3001")
    }
}

impl TransportConnector for TcpConnector {
    type Transport = TcpTransport;

    async fn connect(&self) -> Result<Self::Transport> {
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

// Implement AsyncRead by delegating to the inner TcpStream
impl AsyncRead for TcpTransport {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

// Implement AsyncWrite by delegating to the inner TcpStream
impl AsyncWrite for TcpTransport {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
