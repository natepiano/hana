use std::fmt;

use error_stack::{Report, ResultExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use tracing::debug;

use crate::prelude::*;
use crate::transport::provider::*;
use crate::transport::support::*;

const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\hana-ipc";

pub struct IpcTransport {
    pipe: PipeVariant,
}

impl IpcTransport {
    pub fn new_client(client: NamedPipeClient) -> Self {
        Self {
            pipe: PipeVariant::Client(client),
        }
    }

    pub fn new_server(server: NamedPipeServer) -> Self {
        Self {
            pipe: PipeVariant::Server(server),
        }
    }
}

impl Transport for IpcTransport {}

impl fmt::Debug for IpcTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let variant_name = match &self.pipe {
            PipeVariant::Client(_) => "Client",
            PipeVariant::Server(_) => "Server",
        };
        f.debug_struct("IpcTransport")
            .field("variant", &variant_name)
            .finish()
    }
}

pub struct IpcListener {
    pipe_name: String,
}

impl IpcListener {
    pub async fn create() -> Result<Self> {
        Self::with_name(DEFAULT_PIPE_NAME.to_string())
    }

    pub fn with_name(pipe_name: String) -> Result<Self> {
        Ok(Self { pipe_name })
    }
}

impl TransportListener for IpcListener {
    type Transport = IpcTransport;

    async fn accept(&self) -> Result<Self::Transport> {
        debug!("Creating named pipe server at {}", self.pipe_name);

        // Create a server instance
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&self.pipe_name)
            .change_context(Error::Io)
            .attach_printable_lazy(|| {
                format!("Failed to create named pipe server at {}", self.pipe_name)
            })?;

        debug!(
            "Waiting for client connection on named pipe {}",
            self.pipe_name
        );

        // Wait for a client connection
        server
            .connect()
            .await
            .change_context(Error::Io)
            .attach_printable("Failed to connect named pipe server to client")?;

        debug!("Client connected to named pipe {}", self.pipe_name);

        Ok(IpcTransport::new_server(server))
    }
}

pub struct IpcConnector {
    pipe_name: String,
}

impl IpcConnector {
    pub fn new(pipe_name: String) -> Self {
        Self { pipe_name }
    }

    pub fn default() -> Result<Self> {
        Ok(Self::new(DEFAULT_PIPE_NAME.to_string()))
    }
}

impl TransportConnector for IpcConnector {
    type Transport = IpcTransport;

    async fn connect(&self) -> Result<Self::Transport> {
        debug!("Connecting via named pipes to {}", &self.pipe_name);

        let pipe_name = self.pipe_name.clone();

        let client = connect_with_retry(
            || {
                let pipe_name = pipe_name.clone();
                async move {
                    // ClientOptions::open is not async, but we can wrap it
                    match ClientOptions::new().open(&pipe_name) {
                        Ok(client) => {
                            debug!("Successfully connected to named pipe {}", pipe_name);
                            Ok(client)
                        }
                        Err(e) => {
                            debug!("Failed to connect to named pipe {}: {:?}", pipe_name, e);
                            Err(e)
                        }
                    }
                }
            },
            RetryConfig {
                max_attempts: 30,                                   // Increase retry attempts
                retry_delay: std::time::Duration::from_millis(100), // Shorter delay
            },
            &self.pipe_name,
        )
        .await?;

        Ok(IpcTransport::new_client(client))
    }
}

/// Represents the internal pipe variant - either client or server
enum PipeVariant {
    Client(NamedPipeClient),
    Server(NamedPipeServer),
}

impl AsyncRead for PipeVariant {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            PipeVariant::Client(client) => std::pin::Pin::new(client).poll_read(cx, buf),
            PipeVariant::Server(server) => std::pin::Pin::new(server).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for PipeVariant {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            PipeVariant::Client(client) => std::pin::Pin::new(client).poll_write(cx, buf),
            PipeVariant::Server(server) => std::pin::Pin::new(server).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            PipeVariant::Client(client) => std::pin::Pin::new(client).poll_flush(cx),
            PipeVariant::Server(server) => std::pin::Pin::new(server).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            PipeVariant::Client(client) => std::pin::Pin::new(client).poll_shutdown(cx),
            PipeVariant::Server(server) => std::pin::Pin::new(server).poll_shutdown(cx),
        }
    }
}

// Implement AsyncRead by delegating to the inner pipe variant
impl AsyncRead for IpcTransport {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.pipe).poll_read(cx, buf)
    }
}

// Implement AsyncWrite by delegating to the inner pipe variant
impl AsyncWrite for IpcTransport {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.pipe).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.pipe).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.pipe).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests_ipc {
    use std::error::Error as StdError;
    use tokio::task;

    use super::{IpcConnector, IpcListener};
    use crate::transport::support::test_ipc_transport;

    // Add this function to set up logging for tests
    fn init_test_logging() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();
    }

    #[tokio::test]
    async fn test_windows_named_pipe_transport() -> Result<(), Box<dyn StdError + Send + Sync>> {
        // Initialize logging
        init_test_logging();

        println!("Starting Windows IPC test");

        // Create a unique pipe name for this test
        let pipe_name = format!(r"\\.\pipe\hana-ipc-test-{}", std::process::id());

        // Create listener
        let listener = IpcListener::with_name(pipe_name.clone()).map_err(|e| format!("{e}"))?;
        let connector = IpcConnector::new(pipe_name);

        // Run the test
        test_ipc_transport(listener, connector).await
    }
}
