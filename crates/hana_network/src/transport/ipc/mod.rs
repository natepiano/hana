use crate::prelude::*;
use crate::transport::provider::*;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

// Generic interface
pub struct IpcProvider;

impl TransportProvider for IpcProvider {
    type Transport = IpcTransport;
    type Connector = IpcConnector;
    type Listener = IpcListener;

    fn connector() -> Result<Self::Connector> {
        IpcConnector::default()
    }

    async fn listener() -> Result<Self::Listener> {
        IpcListener::create().await
    }
}
