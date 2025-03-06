use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::transport::Transport;

// Mock transport implementation for testing
pub struct MockTransport {
    pub read_data:         Vec<u8>,
    pub write_data:        Vec<u8>,
    pub read_position:     usize,
    pub write_error_after: Option<usize>,
    pub read_error_kind:   Option<std::io::ErrorKind>,
}

impl MockTransport {
    pub fn new(read_data: Vec<u8>) -> Self {
        Self {
            read_data,
            write_data: Vec::new(),
            read_position: 0,
            write_error_after: None,
            read_error_kind: None,
        }
    }

    pub fn with_write_error(read_data: Vec<u8>, error_after: usize) -> Self {
        Self {
            read_data,
            write_data: Vec::new(),
            read_position: 0,
            write_error_after: Some(error_after),
            read_error_kind: None,
        }
    }

    pub fn with_read_error(error_kind: std::io::ErrorKind) -> Self {
        Self {
            read_data:         vec![],
            write_data:        Vec::new(),
            read_position:     0,
            write_error_after: None,
            read_error_kind:   Some(error_kind),
        }
    }
}

// Implement Debug for MockTransport
impl fmt::Debug for MockTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockTransport")
            .field("read_position", &self.read_position)
            .field("read_data_len", &self.read_data.len())
            .field("write_data_len", &self.write_data.len())
            .finish()
    }
}

// Implement Transport for MockTransport
impl Transport for MockTransport {}

// Implement AsyncRead for MockTransport
impl AsyncRead for MockTransport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if let Some(error_kind) = self.read_error_kind {
            return Poll::Ready(Err(std::io::Error::new(error_kind, "Mock error")));
        }

        if self.read_position >= self.read_data.len() {
            return Poll::Ready(Ok(()));
        }

        let n = std::cmp::min(buf.remaining(), self.read_data.len() - self.read_position);
        buf.put_slice(&self.read_data[self.read_position..self.read_position + n]);
        self.read_position += n;
        Poll::Ready(Ok(()))
    }
}

// Implement AsyncWrite for MockTransport
impl AsyncWrite for MockTransport {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if let Some(error_after) = self.write_error_after {
            if self.write_data.len() >= error_after {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Mock error",
                )));
            }
        }

        self.write_data.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
