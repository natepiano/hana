//! our specific async worker which wraps a hana_async::Worker and
//! specifically has it work with AsyncInstruction and AsyncOutcome
//! it then delegates the send and the try_receive methods to the underlying Worker
use bevy::prelude::*;
use error_stack::Report;
use hana_async::Worker;

use crate::async_messages::{AsyncInstruction, AsyncOutcome};
use crate::error::{Error, Result};

/// Resource that manages the visualization worker
#[derive(Resource)]
pub struct VisualizationWorker(pub Worker<AsyncInstruction, AsyncOutcome>);

impl VisualizationWorker {
    /// Send a command to the visualization worker
    pub fn send(&self, command: impl Into<AsyncInstruction>) -> Result<()> {
        let command = command.into();

        self.0
            .send_command(command)
            .map_err(|_| Report::new(Error::CommandFailed))
    }

    /// Try to receive a message from the visualization worker
    pub fn try_receive(&self) -> Option<AsyncOutcome> {
        self.0.try_receive()
    }
}
