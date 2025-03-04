//! Provider trait for transport implementations
//!
//! The TransportProvider trait allows for compile-time selection of different
//! transport implementations while maintaining a consistent interface.

use std::fmt::Debug;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::prelude::*;

pub trait Transport: AsyncRead + AsyncWrite + Unpin + Debug {
    // No additional methods required for the initial implementation
}

/// A provider of transport implementations
///
/// This trait serves as an abstraction layer for different transport types.
/// It associates the appropriate transport, connector, and listener types,
/// and provides factory methods to create them.
///
/// Implementations of this trait are typically stateless and used at compile time
/// to select which transport implementation to use.
#[allow(async_fn_in_trait)]
pub trait TransportProvider {
    /// The concrete transport type used by this provider
    type Transport: Transport;

    /// The connector used to initiate connections with this transport
    type Connector: TransportConnector<Transport = Self::Transport>;

    /// The listener used to accept incoming connections with this transport
    type Listener: TransportListener<Transport = Self::Transport>;

    /// Get a default connector for this transport
    fn connector() -> Result<Self::Connector>;

    /// Get a default listener for this transport
    async fn listener() -> Result<Self::Listener>;
}

#[allow(async_fn_in_trait)]
pub trait TransportListener {
    type Transport: Transport;

    /// Listen for and accept an incoming connection
    async fn accept(&self) -> Result<Self::Transport>;
}

#[allow(async_fn_in_trait)]
pub trait TransportConnector {
    type Transport: Transport;

    /// Connect to a target
    async fn connect(&self) -> Result<Self::Transport>;
}
