use crate::message::{HanaMessage, Receiver, Sender};
use crate::prelude::*;
use error_stack::{Report, ResultExt};
use std::fmt::Debug;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Represents the role of a Hana network endpoint
pub trait Role {}

/// Controller role - manages and controls visualizations
pub struct HanaApp;
impl Role for HanaApp {}

/// Visualization role - receives and responds to control messages
pub struct Visualization;
impl Role for Visualization {}

// Add this at the top of the file alongside existing imports
use crate::transport::Transport;

// Add this new struct that works with our Transport abstraction
/// A network endpoint in the Hana system using the generic transport abstraction
pub struct Endpoint<R: Role, T: Transport> {
    role: std::marker::PhantomData<R>,
    transport: T,
}

impl<R: Role, T: Transport> Endpoint<R, T> {
    pub fn new(transport: T) -> Self {
        Self {
            role: std::marker::PhantomData,
            transport,
        }
    }

    /// Send a message (only available if this role implements Sender for the message type)
    pub async fn send<M>(&mut self, message: &M) -> Result<()>
    where
        M: HanaMessage + Debug,
        R: Sender<M>,
    {
        let message_bytes = bincode::serialize(message)
            .change_context(Error::Serialization)
            .attach_printable_lazy(|| format!("failed to serialize message: {:?}", message))?;

        let len_prefix = message_bytes.len() as u32;

        self.transport
            .write_all(&len_prefix.to_le_bytes())
            .await
            .change_context(Error::Io)
            .attach_printable("Failed to write length prefix")?;

        self.transport
            .write_all(&message_bytes)
            .await
            .change_context(Error::Io)
            .attach_printable_lazy(|| {
                format!(
                    "Failed to write {} bytes of message data",
                    message_bytes.len()
                )
            })?;

        Ok(())
    }

    /// Receive a message (only available if this role implements Receiver for the message type)
    pub async fn receive<M: HanaMessage>(&mut self) -> Result<Option<M>>
    where
        R: Receiver<M>,
    {
        let mut len_bytes = [0u8; 4];
        match self.transport.read_exact(&mut len_bytes).await {
            Ok(_) => {
                let len = u32::from_le_bytes(len_bytes) as usize;
                let mut buffer = vec![0u8; len];

                self.transport
                    .read_exact(&mut buffer)
                    .await
                    .change_context(Error::Io)
                    .attach_printable_lazy(|| {
                        format!("Failed to read {} bytes of message data", len)
                    })?;

                let message = bincode::deserialize(&buffer)
                    .change_context(Error::Serialization)
                    .attach_printable_lazy(|| {
                        format!("Failed to deserialize {} bytes into message", buffer.len())
                    })?;

                Ok(Some(message))
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(Report::new(Error::Io)
                .attach_printable("Failed to read length prefix")
                .attach_printable(e)),
        }
    }
}

// Role-specific constructors for the new transport-based endpoint
impl Endpoint<HanaApp, crate::transport::TcpTransport> {
    pub async fn connect_to_visualization() -> Result<Self> {
        let transport = crate::transport::tcp::TcpTransport::connect_default()
            .await
            .change_context(Error::ConnectionTimeout)
            .attach_printable("Failed to connect to visualization process")?;

        Ok(Self::new(transport))
    }
}

impl Endpoint<Visualization, crate::transport::TcpTransport> {
    pub async fn accept_controller(listener: &mut TcpListener) -> Result<Self> {
        let (stream, _) = listener
            .accept()
            .await
            .change_context(Error::Io)
            .attach_printable("Failed to accept connection from controller")?;

        Ok(Self::new(crate::transport::TcpTransport::new(stream)))
    }
}

#[cfg(test)]
mod tests_transport {
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

    use super::*;
    use crate::message::Instruction;
    use crate::transport::Transport;

    // Mock role for testing
    struct MockRole;
    impl Role for MockRole {}
    impl Sender<Instruction> for MockRole {}
    impl Receiver<Instruction> for MockRole {}

    // Mock transport implementation for testing
    struct MockTransport {
        read_data: Vec<u8>,
        write_data: Vec<u8>,
        read_position: usize,
        write_error_after: Option<usize>,
        read_error_kind: Option<std::io::ErrorKind>,
    }

    impl MockTransport {
        fn new(read_data: Vec<u8>) -> Self {
            Self {
                read_data,
                write_data: Vec::new(),
                read_position: 0,
                write_error_after: None,
                read_error_kind: None,
            }
        }

        fn with_write_error(read_data: Vec<u8>, error_after: usize) -> Self {
            Self {
                read_data,
                write_data: Vec::new(),
                read_position: 0,
                write_error_after: Some(error_after),
                read_error_kind: None,
            }
        }

        fn with_read_error(error_kind: std::io::ErrorKind) -> Self {
            Self {
                read_data: vec![],
                write_data: Vec::new(),
                read_position: 0,
                write_error_after: None,
                read_error_kind: Some(error_kind),
            }
        }
    }

    // Implement Debug for MockTransport
    impl Debug for MockTransport {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

    #[tokio::test]
    async fn test_transport_send_message_success() {
        let mock = MockTransport::new(vec![]);
        let mut endpoint = Endpoint::<MockRole, MockTransport>::new(mock);

        let instruction = Instruction::Ping;
        endpoint.send(&instruction).await.unwrap();

        let written = &endpoint.transport.write_data;
        assert!(!written.is_empty());

        // Verify format: length prefix + serialized data
        let len_bytes = &written[0..4];
        let len = u32::from_le_bytes(len_bytes.try_into().unwrap());
        assert_eq!(written.len(), (len as usize) + 4);
    }

    #[tokio::test]
    async fn test_transport_send_message_io_error() {
        let mock = MockTransport::with_write_error(vec![], 0);
        let mut endpoint = Endpoint::<MockRole, MockTransport>::new(mock);

        let instruction = Instruction::Ping;
        let result = endpoint.send(&instruction).await;
        assert!(matches!(result, Err(e) if *e.current_context() == Error::Io));
    }

    #[tokio::test]
    async fn test_transport_receive_message_success() {
        // Create test data
        let instruction = Instruction::Ping;
        let msg_bytes = bincode::serialize(&instruction).unwrap();
        let len = msg_bytes.len() as u32;
        let mut data = len.to_le_bytes().to_vec();
        data.extend(msg_bytes);

        let mock = MockTransport::new(data);
        let mut endpoint = Endpoint::<MockRole, MockTransport>::new(mock);

        let result: Option<Instruction> = endpoint.receive().await.unwrap();
        assert_eq!(result, Some(Instruction::Ping));
    }

    #[tokio::test]
    async fn test_transport_receive_message_io_error() {
        let mock = MockTransport::with_read_error(std::io::ErrorKind::Other);
        let mut endpoint = Endpoint::<MockRole, MockTransport>::new(mock);

        let result = endpoint.receive::<Instruction>().await;
        assert!(matches!(result, Err(e) if *e.current_context() == Error::Io));
    }

    #[tokio::test]
    async fn test_transport_receive_message_eof() {
        let mock = MockTransport::new(vec![]);
        let mut endpoint = Endpoint::<MockRole, MockTransport>::new(mock);

        let result: Option<Instruction> = endpoint.receive().await.unwrap();
        assert_eq!(result, None);
    }
}
