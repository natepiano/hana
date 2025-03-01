use super::base_endpoint::Endpoint;
use crate::prelude::*;
use crate::role::Role;
use crate::role::{HanaRole, VisualizationRole};
use crate::transport::tcp::{TcpConnector, TcpListener, TcpTransport};
use crate::transport::*;
use std::ops::{Deref, DerefMut};

/// A generic endpoint that can be specialized for different roles in the Hana system
pub struct RoleBasedEndpoint<R: Role, T: Transport>(Endpoint<R, T>);

/// An endpoint for a Hana controller to connect to and control visualizations
pub type HanaEndpoint<T = TcpTransport> = RoleBasedEndpoint<HanaRole, T>;

/// An endpoint for a visualization to accept connections from Hana controllers
pub type VisualizationEndpoint<T = TcpTransport> = RoleBasedEndpoint<VisualizationRole, T>;

impl<R: Role, T: Transport> RoleBasedEndpoint<R, T> {
    /// Create a new role-based endpoint with the specified transport
    pub fn new(transport: T) -> Self {
        Self(Endpoint::new(transport))
    }
}

// Implement Deref to delegate to the inner Endpoint
impl<R: Role, T: Transport> Deref for RoleBasedEndpoint<R, T> {
    type Target = Endpoint<R, T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// Implement DerefMut to delegate to the inner Endpoint
impl<R: Role, T: Transport> DerefMut for RoleBasedEndpoint<R, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// Role-specific implementation for HanaRole
impl RoleBasedEndpoint<HanaRole, TcpTransport> {
    /// Connect to a visualization at the default address
    pub async fn connect_to_visualization() -> Result<Self> {
        let connector = TcpConnector::default();
        let transport = connector.connect().await?;
        Ok(Self::new(transport))
    }
}

// Role-specific implementation for VisualizationRole
impl RoleBasedEndpoint<VisualizationRole, TcpTransport> {
    /// Listen on the default port and accept a connection
    pub async fn listen_for_hana() -> Result<Self> {
        let listener = TcpListener::bind_default().await?;
        let transport = listener.accept().await?;
        Ok(Self::new(transport))
    }
}

#[cfg(test)]
mod tests_endpoint {
    use super::*;
    use crate::message::Instruction;
    use crate::transport::mock::MockTransport;

    #[tokio::test]
    async fn test_hana_endpoint_send_message() {
        let mock = MockTransport::new(vec![]);
        let mut endpoint = HanaEndpoint::new(mock);

        let instruction = Instruction::Ping;
        endpoint.send(&instruction).await.unwrap();

        let written = &endpoint.0.transport().write_data;
        assert!(!written.is_empty());
    }

    #[tokio::test]
    async fn test_visualization_endpoint_receive_message() {
        // Create test data
        let instruction = Instruction::Ping;
        let msg_bytes = bincode::serialize(&instruction).unwrap();
        let len = msg_bytes.len() as u32;
        let mut data = len.to_le_bytes().to_vec();
        data.extend(msg_bytes);

        let mock = MockTransport::new(data);
        let mut endpoint = VisualizationEndpoint::new(mock);

        let result: Option<Instruction> = endpoint.receive().await.unwrap();
        assert_eq!(result, Some(Instruction::Ping));
    }
}
