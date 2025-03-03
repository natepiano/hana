use crate::prelude::*;
use crate::transport::provider::*;
use error_stack::{Report, ResultExt};
use std::fmt;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use tracing::debug;

const CONNECTION_MAX_ATTEMPTS: u8 = 15;
const CONNECTION_RETRY_DELAY: Duration = Duration::from_millis(200);
const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\hana-ipc";

pub struct IpcTransport {
    is_server: bool,
    client: Option<NamedPipeClient>,
    server: Option<NamedPipeServer>,
}

impl IpcTransport {
    pub fn new_client(client: NamedPipeClient) -> Self {
        Self {
            is_server: false,
            client: Some(client),
            server: None,
        }
    }

    pub fn new_server(server: NamedPipeServer) -> Self {
        Self {
            is_server: true,
            client: None,
            server: Some(server),
        }
    }
}

impl Transport for IpcTransport {}

impl fmt::Debug for IpcTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IpcTransport")
            .field("is_server", &self.is_server)
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
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&self.pipe_name)
            .change_context(Error::Io)
            .attach_printable_lazy(|| {
                format!("Failed to create named pipe server at {}", self.pipe_name)
            })?;

        // Wait for a client connection
        server
            .connect()
            .await
            .change_context(Error::Io)
            .attach_printable("Failed to connect named pipe server to client")?;

        Ok(IpcTransport::new_server(server))
    }
}

pub struct IpcConnector {
    pipe_name: String,
    max_attempts: u8,
    retry_delay: Duration,
}

impl IpcConnector {
    pub fn new(pipe_name: String) -> Self {
        Self {
            pipe_name,
            max_attempts: CONNECTION_MAX_ATTEMPTS,
            retry_delay: CONNECTION_RETRY_DELAY,
        }
    }

    pub fn default() -> Result<Self> {
        Ok(Self::new(DEFAULT_PIPE_NAME.to_string()))
    }
}

impl TransportConnector for IpcConnector {
    type Transport = IpcTransport;

    async fn connect(&self) -> Result<Self::Transport> {
        debug!("Connecting via named pipes to {:?}", &self.pipe_name);

        let mut attempts = 0;
        let client = loop {
            match ClientOptions::new().open(&self.pipe_name) {
                Ok(client) => break client,
                Err(e) => {
                    attempts += 1;
                    if attempts >= self.max_attempts {
                        return Err(Report::new(Error::ConnectionTimeout)
                                            .attach_printable(format!(
                                                "Failed to connect to named pipe after {attempts} attempts. Pipe name: {}",
                                                self.pipe_name
                                            )));
                    }
                    debug!("Connection attempt {} failed: {}, retrying...", attempts, e);
                    tokio::time::sleep(self.retry_delay).await;
                }
            }
        };

        Ok(IpcTransport::new_client(client))
    }
}

// Implement AsyncRead by delegating to the inner NamedPipe
impl AsyncRead for IpcTransport {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if self.is_server {
            std::pin::Pin::new(self.server.as_mut().unwrap()).poll_read(cx, buf)
        } else {
            std::pin::Pin::new(self.client.as_mut().unwrap()).poll_read(cx, buf)
        }
    }
}

// Implement AsyncWrite by delegating to the inner NamedPipe
impl AsyncWrite for IpcTransport {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        if self.is_server {
            std::pin::Pin::new(self.server.as_mut().unwrap()).poll_write(cx, buf)
        } else {
            std::pin::Pin::new(self.client.as_mut().unwrap()).poll_write(cx, buf)
        }
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if self.is_server {
            std::pin::Pin::new(self.server.as_mut().unwrap()).poll_flush(cx)
        } else {
            std::pin::Pin::new(self.client.as_mut().unwrap()).poll_flush(cx)
        }
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if self.is_server {
            std::pin::Pin::new(self.server.as_mut().unwrap()).poll_shutdown(cx)
        } else {
            std::pin::Pin::new(self.client.as_mut().unwrap()).poll_shutdown(cx)
        }
    }
}
