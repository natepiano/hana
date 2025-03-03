use crate::prelude::*;
use crate::transport::provider::*;
use error_stack::{Report, ResultExt};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::net::UnixListener as TokioUnixListener;
use tokio::net::UnixStream as TokioUnixStream;
use tracing::{debug, warn};

const CONNECTION_MAX_ATTEMPTS: u8 = 15;
const CONNECTION_RETRY_DELAY: Duration = Duration::from_millis(200);
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
    path: PathBuf,
}

impl IpcListener {
    pub async fn create() -> Result<Self> {
        Self::bind(DEFAULT_SOCKET_PATH).await
    }

    pub async fn bind<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Remove socket file if it exists
        if path.exists() {
            std::fs::remove_file(path)
                .change_context(Error::Io)
                .attach_printable_lazy(|| {
                    format!("Failed to remove existing socket file at {:?}", path)
                })?;
        }

        let listener = TokioUnixListener::bind(path)
            .change_context(Error::Io)
            .attach_printable_lazy(|| format!("Failed to bind Unix socket at {:?}", path))?;

        Ok(Self {
            listener,
            path: path.to_path_buf(),
        })
    }
}

impl TransportListener for IpcListener {
    type Transport = IpcTransport;

    async fn accept(&self) -> Result<Self::Transport> {
        warn!("accept called on unix IpcListener");
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
            println!("Cleaning up Unix socket at {:?}", self.path);
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub struct IpcConnector {
    path: PathBuf,
    max_attempts: u8,
    retry_delay: Duration,
}

impl IpcConnector {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            max_attempts: CONNECTION_MAX_ATTEMPTS,
            retry_delay: CONNECTION_RETRY_DELAY,
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

        let mut attempts = 0;
        let stream = loop {
            match TokioUnixStream::connect(&self.path).await {
                Ok(stream) => break stream,
                Err(_) => {
                    attempts += 1;
                    if attempts >= self.max_attempts {
                        return Err(Report::new(Error::ConnectionTimeout).attach_printable(
                            format!("Failed to connect after {attempts} attempts"),
                        ));
                    }
                    debug!("Connection attempt {} failed, retrying...", attempts);
                    tokio::time::sleep(self.retry_delay).await;
                }
            }
        };

        Ok(IpcTransport::new(stream))
    }
}

crate::impl_async_io!(IpcTransport, stream);
