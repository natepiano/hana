//! transport layers provided by hana_network

#[cfg(test)]
pub mod mock;
pub mod tcp;

use crate::prelude::*;
use std::fmt::Debug;
use tokio::io::{AsyncRead, AsyncWrite};

pub trait Transport: AsyncRead + AsyncWrite + Unpin + Debug {
    // No additional methods required for the initial implementation
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
