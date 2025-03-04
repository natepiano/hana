use std::fmt;
use std::path::{Path, PathBuf};

use error_stack::ResultExt;
use tokio::net::{UnixListener as TokioUnixListener, UnixStream as TokioUnixStream};
use tracing::debug;

use crate::prelude::*;
use crate::transport::provider::*;
use crate::transport::support::*;

const DEFAULT_SOCKET_PATH: &str = "/tmp/hana-ipc.sock";

pub struct IpcTransport {
    stream: TokioUnixStream,
}

impl IpcTransport {
    pub fn new(stream: TokioUnixStream) -> Self {
        Self { stream }
    }
}

impl Transport for IpcTransport {}

impl fmt::Debug for IpcTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IpcTransport")
            .field("peer_addr", &self.stream.peer_addr().ok())
            .finish()
    }
}

pub struct IpcListener {
    listener: TokioUnixListener,
    path:     PathBuf,
}

impl IpcListener {
    pub async fn create() -> Result<Self> {
        Self::bind(DEFAULT_SOCKET_PATH).await
    }

    pub async fn bind<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        debug!("Attempting to bind IPC socket at {:?}", path);

        // Remove socket file if it exists
        if path.exists() {
            debug!("Found existing socket file at {:?}, removing it", path);
            std::fs::remove_file(path)
                .change_context(Error::Io)
                .attach_printable_lazy(|| {
                    format!("Failed to remove existing socket file at {:?}", path)
                })?;
        }

        debug!("Binding Unix socket at {:?}", path);
        let listener = TokioUnixListener::bind(path)
            .change_context(Error::Io)
            .attach_printable_lazy(|| format!("Failed to bind Unix socket at {:?}", path))?;

        debug!("Successfully bound Unix socket at {:?}", path);

        Ok(Self {
            listener,
            path: path.to_path_buf(),
        })
    }
}

impl TransportListener for IpcListener {
    type Transport = IpcTransport;

    async fn accept(&self) -> Result<Self::Transport> {
        debug!("accept called on unix IpcListener");
        let (stream, _) = self
            .listener
            .accept()
            .await
            .change_context(Error::Io)
            .attach_printable("Failed to accept connection on Unix socket")?;

        Ok(IpcTransport::new(stream))
    }
}

impl Drop for IpcListener {
    fn drop(&mut self) {
        // Clean up the socket file when the listener is dropped
        if self.path.exists() {
            debug!("Cleaning up Unix socket at {:?}", self.path);
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub struct IpcConnector {
    path: PathBuf,
}

impl IpcConnector {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn default() -> Result<Self> {
        Ok(Self::new(DEFAULT_SOCKET_PATH))
    }
}

impl TransportConnector for IpcConnector {
    type Transport = IpcTransport;

    async fn connect(&self) -> Result<Self::Transport> {
        debug!("Connecting via Unix Sockets to {:?}", &self.path);

        let path = self.path.clone();

        let stream = connect_with_retry(
            || {
                let path = path.clone();
                async move { TokioUnixStream::connect(&path).await }
            },
            RetryConfig::default(),
            &self.path,
        )
        .await?;

        Ok(IpcTransport::new(stream))
    }
}

crate::impl_async_io!(IpcTransport, stream);

#[cfg(test)]
mod tests_ipc {
    use std::error::Error as StdError;

    use tempfile::tempdir;

    use super::{IpcConnector, IpcListener};
    use crate::transport::support::test_ipc_transport;

    #[tokio::test]
    async fn test_unix_socket_transport() -> Result<(), Box<dyn StdError + Send + Sync>> {
        // Create a temporary directory for our socket file
        let temp_dir = tempdir()?;
        let socket_path = temp_dir.path().join("hana-test.sock");

        // Create listener and connector
        let listener = IpcListener::bind(&socket_path)
            .await
            .map_err(|e| format!("{e}"))?;
        let connector = IpcConnector::new(&socket_path);

        // Run the test
        test_ipc_transport(listener, connector).await
    }
}
