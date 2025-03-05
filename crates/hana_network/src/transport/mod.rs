//! transport layers provided by hana_network
use std::fmt::Debug;

use tokio::io::{AsyncRead, AsyncWrite};

// The basic Transport trait
pub trait Transport: AsyncRead + AsyncWrite + Unpin + Debug {
    // No additional methods required for the initial implementation
}

// Define the connector and listener traits that are still needed
#[allow(async_fn_in_trait)]
pub trait TransportConnector {
    type Transport: Transport;

    /// Connect to a target
    async fn connect(&self) -> crate::prelude::Result<Self::Transport>;
}

#[allow(async_fn_in_trait)]
pub trait TransportListener {
    type Transport: Transport;

    /// Listen for and accept an incoming connection
    async fn accept(&self) -> crate::prelude::Result<Self::Transport>;
}

#[cfg(test)]
pub mod mock;
mod support;

#[allow(dead_code)] // only while we haven't implemented calls to it
pub mod socket_pair;
#[allow(dead_code)] // only while we haven't implemented calls to it
pub mod tcp;
pub mod unix; //currently in use
