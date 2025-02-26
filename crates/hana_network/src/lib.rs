mod error;

use std::time::Duration;

use error_stack::{Report, ResultExt};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::debug;

pub use crate::error::{Error, Result};

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

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Instruction {
    Ping,
    Shutdown,
}

pub async fn send_instruction<W>(stream: &mut W, command: &Instruction) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let command_bytes = bincode::serialize(command)
        .change_context(Error::Serialization)
        .attach_printable_lazy(|| format!("Failed to serialize command: {:?}", command))?;

    let len_prefix = command_bytes.len() as u32;

    stream
        .write_all(&len_prefix.to_le_bytes())
        .await
        .change_context(Error::Io)
        .attach_printable("Failed to write length prefix")?;

    stream
        .write_all(&command_bytes)
        .await
        .change_context(Error::Io)
        .attach_printable_lazy(|| {
            format!(
                "Failed to write {} bytes of command data",
                command_bytes.len()
            )
        })?;

    Ok(())
}

pub async fn receive_instruction<R>(stream: &mut R) -> Result<Option<Instruction>>
where
    R: AsyncReadExt + Unpin,
{
    let mut len_bytes = [0u8; 4];
    match stream.read_exact(&mut len_bytes).await {
        Ok(_) => {
            let len = u32::from_le_bytes(len_bytes) as usize;
            let mut buffer = vec![0u8; len];

            stream
                .read_exact(&mut buffer)
                .await
                .change_context(Error::Io)
                .attach_printable_lazy(|| {
                    format!("Failed to read {} bytes of command data", len)
                })?;

            let command = bincode::deserialize(&buffer)
                .change_context(Error::Serialization)
                .attach_printable_lazy(|| {
                    format!("Failed to deserialize {} bytes into Command", buffer.len())
                })?;

            Ok(Some(command))
        }
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
        Err(e) => Err(Report::new(Error::Io)
            .attach_printable("Failed to read length prefix")
            .attach_printable(e)),
    }
}

#[cfg(test)]
mod write_tests {

    use super::*;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::AsyncWrite;

    struct MockAsyncWriter {
        written: Vec<u8>,
        error_after: Option<usize>,
    }

    impl AsyncWrite for MockAsyncWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            if let Some(error_after) = self.error_after {
                if self.written.len() >= error_after {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Mock error",
                    )));
                }
            }

            self.written.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn test_write_command_success() {
        let mut mock = Box::pin(MockAsyncWriter {
            written: Vec::new(),
            error_after: None,
        });

        let command = Instruction::Ping;
        send_instruction(&mut mock, &command).await.unwrap();
        assert!(!mock.as_ref().written.is_empty());
    }

    #[tokio::test]
    async fn test_write_command_io_error() {
        let mut mock = Box::pin(MockAsyncWriter {
            written: Vec::new(),
            error_after: Some(0),
        });

        let command = Instruction::Ping;
        let result = send_instruction(&mut mock, &command).await;
        assert!(matches!(result, Err(ref e) if *e.current_context() == Error::Io));
    }

    #[tokio::test]
    async fn test_write_command_correct_format() {
        let mut mock = Box::pin(MockAsyncWriter {
            written: Vec::new(),
            error_after: None,
        });

        let command = Instruction::Ping;
        send_instruction(&mut mock, &command).await.unwrap();

        let written = &mock.as_ref().written;

        // First 4 bytes should be length prefix
        let len_bytes = &written[0..4];
        let len = u32::from_le_bytes(len_bytes.try_into().unwrap());

        // Remaining bytes should be serialized command
        let command_bytes = &written[4..];
        assert_eq!(command_bytes.len(), len as usize);

        // Should deserialize back to original command
        let deserialized: Instruction = bincode::deserialize(command_bytes).unwrap();
        assert!(matches!(deserialized, Instruction::Ping));
    }
}

#[cfg(test)]
mod read_tests {
    use super::*;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::AsyncRead;
    use tokio::io::ReadBuf;

    #[tokio::test]
    async fn test_read_command_success() {
        // Create a valid command and serialize it
        let command = Instruction::Ping;
        let command_bytes = bincode::serialize(&command).unwrap();

        // Create mock data with proper length prefix
        let len = command_bytes.len() as u32;
        let mut data = len.to_le_bytes().to_vec();
        data.extend(command_bytes);

        let mock = MockAsyncReader {
            data,
            position: 0,
            error_kind: None,
        };

        let mut stream = Box::pin(mock);
        let result = receive_instruction(&mut stream).await.unwrap();
        assert_eq!(result, Some(Instruction::Ping));
    }

    // Mock async reader for testing EOF and errors
    struct MockAsyncReader {
        data: Vec<u8>,
        position: usize,
        error_kind: Option<std::io::ErrorKind>,
    }

    impl AsyncRead for MockAsyncReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            if let Some(error_kind) = self.error_kind {
                return Poll::Ready(Err(std::io::Error::new(error_kind, "Mock error")));
            }

            if self.position >= self.data.len() {
                return Poll::Ready(Ok(())); // EOF
            }

            let n = std::cmp::min(buf.remaining(), self.data.len() - self.position);
            buf.put_slice(&self.data[self.position..self.position + n]);
            self.position += n;
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn test_read_command_unexpected_eof() {
        let mock = MockAsyncReader {
            data: vec![4, 0, 0, 0], // Length prefix only
            position: 0,
            error_kind: None,
        };

        let mut stream = Box::pin(mock);
        let result = receive_instruction(&mut stream).await;
        assert!(matches!(result, Err(ref e) if *e.current_context() == Error::Io));
    }

    #[tokio::test]
    async fn test_read_command_io_error() {
        let mock = MockAsyncReader {
            data: vec![],
            position: 0,
            error_kind: Some(std::io::ErrorKind::Other),
        };

        let mut stream = Box::pin(mock);
        let result = receive_instruction(&mut stream).await;
        assert!(matches!(result, Err(ref e) if *e.current_context() == Error::Io));
    }

    #[tokio::test]
    async fn test_read_command_deserialization_error() {
        let mock = MockAsyncReader {
            data: vec![
                4, 0, 0, 0, // Length prefix (4 bytes)
                0, 1, 2, 3, // Invalid command data
            ],
            position: 0,
            error_kind: None,
        };

        let mut stream = Box::pin(mock);
        let result = receive_instruction(&mut stream).await;
        assert!(matches!(result, Err(ref e) if *e.current_context() == Error::Serialization));
    }
}
