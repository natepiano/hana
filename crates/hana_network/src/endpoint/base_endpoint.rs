use std::fmt::Debug;

use error_stack::{Report, ResultExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    message::{HanaMessage, Receiver, Sender},
    prelude::*,
    role::Role,
    transport::Transport,
};

/// A network endpoint in the Hana system using the generic transport abstraction
pub struct Endpoint<R: Role, T: Transport> {
    role:      std::marker::PhantomData<R>,
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
    /// R: Sender<M> - a Role is a Sender of a particular kind of HanaMessage
    pub async fn send<M>(&mut self, message: &M) -> Result<()>
    where
        M: HanaMessage + Debug,
        R: Sender<M>,
    {
        let message_bytes = bincode::serialize(message)
            .change_context(Error::Serialization)
            .attach_printable_lazy(|| format!("failed to serialize message: '{message:?}'"))
            .attach_printable_lazy(|| format!("transport: {:?}", self.transport))?;

        let len_prefix = message_bytes.len() as u32;

        self.transport
            .write_all(&len_prefix.to_le_bytes())
            .await
            .change_context(Error::Io)
            .attach_printable_lazy(|| {
                format!("Failed to write length prefix: '{len_prefix}' to message: '{message:?}'")
            })
            .attach_printable_lazy(|| format!("transport: {:?}", self.transport))?;

        self.transport
            .write_all(&message_bytes)
            .await
            .change_context(Error::Io)
            .attach_printable_lazy(|| {
                format!(
                    "Failed to write {} bytes of message data for message: '{message:?}'",
                    message_bytes.len(),
                )
            })
            .attach_printable_lazy(|| format!("transport: {:?}", self.transport))?;

        Ok(())
    }

    /// Receive a message (only available if this role implements Receiver for the message type)
    /// R: Receiver<M> - a Role is a Receiver of a particular kind of HanaMessage
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
                    })
                    .attach_printable_lazy(|| format!("transport: {:?}", self.transport))?;

                let message = bincode::deserialize(&buffer)
                    .change_context(Error::Serialization)
                    .attach_printable_lazy(|| {
                        format!("Failed to deserialize {} bytes into message", buffer.len())
                    })
                    .attach_printable_lazy(|| format!("transport: {:?}", self.transport))?;

                Ok(Some(message))
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(Report::new(Error::Io)
                .attach_printable("Failed to read length prefix for message")
                .attach_printable(e)),
        }
    }
}

impl<R: Role, T: Transport> Endpoint<R, T> {
    // This is only available when compiling tests
    #[cfg(test)]
    pub(crate) fn transport(&self) -> &T {
        &self.transport
    }
}

#[cfg(test)]
mod tests_transport {
    use super::*;
    use crate::{message::Instruction, transport::mock::MockTransport};

    // Mock role for testing
    struct MockRole;
    impl Role for MockRole {}
    impl Sender<Instruction> for MockRole {}
    impl Receiver<Instruction> for MockRole {}

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
