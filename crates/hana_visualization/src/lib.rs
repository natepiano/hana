mod error;
mod prelude;

use std::marker::PhantomData;
use std::path::PathBuf;
use std::time::Duration;

use error_stack::ResultExt;
use hana_network::{Endpoint, HanaApp, Instruction, TcpTransport};
use hana_process::Process;

use crate::prelude::*;

/// Marker type for visualizations that have not yet connected their network.
pub struct Unstarted;

/// Marker type for visualizations that are currently connecting their network.
pub struct Started;

/// Marker type for visualizations that have successfully connected.
pub struct Connected;

/// The Visualization type represents a remote visualization process along with its
/// TcpStream for its network connection. The State generic parameter enforces that
/// only valid operations are available at a given stage.
pub struct Visualization<State> {
    process: Process,
    // In the Unstarted state, there is no connection.
    // In the Connected state, we hold the TcpStream.
    stream: StreamState<State>,
    _state: PhantomData<State>,
}

pub enum StreamState<State> {
    Unconnected(PhantomData<State>),
    Connected(Endpoint<HanaApp, TcpTransport>),
}

impl Visualization<Unstarted> {
    /// Create a new unstarted visualization
    pub fn start(path: PathBuf, env_filter: impl Into<String>) -> Result<Visualization<Started>> {
        let process = Process::run(path, env_filter.into())
            .change_context(Error::Process)
            .attach_printable("Failed to start visualization process")?;

        Ok(Visualization {
            process,
            stream: StreamState::Unconnected(PhantomData),
            _state: PhantomData,
        })
    }
}

impl Visualization<Started> {
    /// Connect to the visualization process
    pub async fn connect(self) -> Result<Visualization<Connected>> {
        // Use the transport approach internally
        let transport = TcpTransport::connect_default()
            .await
            .change_context(Error::Process)
            .attach_printable("Failed to connect to visualization process")?;

        let endpoint = Endpoint::new(transport);

        Ok(Visualization {
            process: self.process,
            stream: StreamState::Connected(endpoint),
            _state: PhantomData,
        })
    }
}

impl Visualization<Connected> {
    /// Send a command to the connected visualization
    async fn send_instruction(&mut self, instruction: &Instruction) -> Result<()> {
        let StreamState::Connected(endpoint) = &mut self.stream else {
            panic!("Type system ensures Visualization<Connected> must have StreamState::Connected");
        };

        endpoint
            .send(instruction)
            .await
            .change_context(Error::Network)
            .attach_printable_lazy(|| format!("Failed to send instruction: {:?}", instruction))
    }

    pub async fn ping(&mut self) -> Result<()> {
        self.send_instruction(&Instruction::Ping).await
    }

    /// Shutdown the visualization gracefully
    pub async fn shutdown(mut self, timeout: Duration) -> Result<()> {
        // Send shutdown command
        self.send_instruction(&Instruction::Shutdown).await?;

        // Ensure process terminates
        self.process
            .ensure_shutdown(timeout)
            .change_context(Error::Process)
            .attach_printable("failed to ensure shutdown of visualization process")?;

        Ok(())
    }
}
