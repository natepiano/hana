use crate::{Error, Result};
use tokio::net::{TcpListener, TcpStream};

/// Represents the role of a Hana network endpoint
pub trait Role {}

/// Controller role - manages and controls visualizations
pub struct HanaApp;
impl Role for HanaApp {}

/// Visualization role - receives and responds to control messages
pub struct Visualization;
impl Role for Visualization {}

/// A network endpoint in the Hana system
pub struct HanaEndpoint<R: Role> {
    role: std::marker::PhantomData<R>,
    stream: TcpStream,
}

impl<R: Role> HanaEndpoint<R> {
    /// Creates a new endpoint from an established connection
    pub(crate) fn new(stream: TcpStream) -> Self {
        Self {
            role: std::marker::PhantomData,
            stream,
        }
    }
}

// Specific implementation for Visualization endpoints
impl HanaEndpoint<Visualization> {
    /// Creates a new visualization endpoint by connecting to a controller
    pub async fn connect_as_visualization() -> Result<Self> {
        let stream = super::connect().await?;
        Ok(Self::new(stream))
    }
}

// Specific implementation for Controller endpoints
impl HanaEndpoint<HanaApp> {
    /// Accept a connection from a visualization
    pub async fn accept_visualization(listener: &mut TcpListener) -> Result<Self> {
        let (stream, _) = listener.accept().await.map_err(|_| Error::Io)?;
        Ok(Self::new(stream))
    }
}
