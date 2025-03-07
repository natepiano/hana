use std::fmt;
use std::os::fd::RawFd;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::os::unix::net::UnixStream as StdUnixStream;

use error_stack::ResultExt;
use tokio::net::UnixStream as TokioUnixStream;

use crate::prelude::*;
use crate::transport::{Transport, TransportConnector, TransportListener};

/// Transport implementation using Unix socket pairs
pub struct SocketPairTransport {
    stream: TokioUnixStream,
}

impl SocketPairTransport {
    pub fn new(stream: TokioUnixStream) -> Self {
        Self { stream }
    }

    /// Create a socket pair returning the parent and child transports
    pub fn create_pair() -> Result<(Self, Self)> {
        // Create a standard Unix socket pair
        let (stream1, stream2) = StdUnixStream::pair()
            .change_context(Error::Io)
            .attach_printable("Failed to create Unix socket pair")?;

        // Convert to tokio Unix streams
        let stream1 = TokioUnixStream::from_std(stream1)
            .change_context(Error::Io)
            .attach_printable("Failed to convert standard Unix stream to tokio Unix stream")?;

        let stream2 = TokioUnixStream::from_std(stream2)
            .change_context(Error::Io)
            .attach_printable("Failed to convert standard Unix stream to tokio Unix stream")?;

        Ok((Self::new(stream1), Self::new(stream2)))
    }

    /// Get the raw file descriptor for this transport
    /// This is used to pass the descriptor to child processes
    pub fn as_raw_fd(&self) -> RawFd {
        self.stream.as_raw_fd()
    }

    /// Create a transport from a raw file descriptor
    /// # Safety
    /// The file descriptor must be valid and refer to a Unix socket
    pub unsafe fn from_raw_fd(fd: RawFd) -> Result<Self> {
        // First create a standard Unix stream from the raw fd
       unsafe  { let std_stream = StdUnixStream::from_raw_fd(fd); 

        // Then convert to a tokio Unix stream
        let tokio_stream = TokioUnixStream::from_std(std_stream)
            .change_context(Error::Io)
            .attach_printable("Failed to convert std UnixStream to tokio UnixStream")?;

        Ok(Self::new(tokio_stream)) }
    }
}

impl Transport for SocketPairTransport {}

impl fmt::Debug for SocketPairTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SocketPairTransport")
            .field("fd", &self.stream.as_raw_fd())
            .finish()
    }
}

/// Connector for socket pairs
pub struct SocketPairConnector;

impl TransportConnector for SocketPairConnector {
    type Transport = SocketPairTransport;

    async fn connect(&self) -> Result<Self::Transport> {
        Err(error_stack::Report::new(Error::Io).attach_printable(
            "SocketPairConnector can't be used directly. Use SocketPairTransport::create_pair()",
        ))
    }
}

/// Listener for socket pairs
pub struct SocketPairListener;

impl TransportListener for SocketPairListener {
    type Transport = SocketPairTransport;

    async fn accept(&self) -> Result<Self::Transport> {
        Err(error_stack::Report::new(Error::Io).attach_printable(
            "SocketPairListener can't be used directly. Use SocketPairTransport::create_pair()",
        ))
    }
}

crate::impl_async_io!(SocketPairTransport, stream);

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn test_socket_pair_transport() -> Result<()> {
        // Create a socket pair
        let (mut parent, mut child) = SocketPairTransport::create_pair()?;

        // Test bidirectional communication
        parent
            .write_all(b"hello from parent")
            .await
            .change_context(Error::Io)?;

        let mut buf = [0u8; 17];
        let n = child.read(&mut buf).await.change_context(Error::Io)?;

        assert_eq!(&buf[..n], b"hello from parent");

        child
            .write_all(b"hello from child")
            .await
            .change_context(Error::Io)?;

        let mut buf = [0u8; 16];
        let n = parent.read(&mut buf).await.change_context(Error::Io)?;

        assert_eq!(&buf[..n], b"hello from child");

        Ok(())
    }

    #[tokio::test]
    async fn test_socket_pair_connector_error() {
        let connector = SocketPairConnector;
        let result = connector.connect().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_socket_pair_listener_error() {
        let listener = SocketPairListener;
        let result = listener.accept().await;
        assert!(result.is_err());
    }
}
