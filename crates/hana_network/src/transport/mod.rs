//! transport layers provided by hana_network

pub mod tcp;
use std::fmt::Debug;

pub use tcp::TcpTransport;
use tokio::io::{AsyncRead, AsyncWrite};

pub trait Transport: AsyncRead + AsyncWrite + Unpin + Debug {
    // No additional methods required for the initial implementation
}
