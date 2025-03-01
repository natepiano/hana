use std::fmt;
use std::time::Duration;

use error_stack::Report;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tracing::debug;

use crate::prelude::*;
use crate::transport::Transport; // Added import for debug macro

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

    /// Connect to a TCP address with retry logic
    ///
    /// This mimics the behavior of the original `connect()` function in lib.rs
    pub async fn connect(addr: &str) -> Result<Self> {
        let mut attempts = 0;
        let stream = loop {
            match TcpStream::connect(addr).await {
                Ok(stream) => break stream,
                Err(_) => {
                    attempts += 1;
                    if attempts >= CONNECTION_MAX_ATTEMPTS {
                        return Err(Report::new(Error::ConnectionTimeout).attach_printable(
                            format!("Failed to connect after {attempts} attempts"),
                        ));
                    }
                    debug!("Connection attempt {} failed, retrying...", attempts);
                    tokio::time::sleep(CONNECTION_RETRY_DELAY).await;
                }
            }
        };

        Ok(Self::new(stream))
    }

    /// Connect to the default visualization address (equivalent to the original connect())
    pub async fn connect_default() -> error_stack::Result<Self, Error> {
        Self::connect("127.0.0.1:3001").await
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
