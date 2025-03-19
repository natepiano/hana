use std::path::PathBuf;
use std::time::Duration;

use bevy::prelude::*;
use error_stack::Report;
use hana_network::Instruction;

use crate::error::Error;

// tasks that the AsyncRuntime can do
#[derive(Debug, Clone)]
pub enum AsyncInstruction {
    /// Start a visualization process and connect to it
    Start {
        entity:     Entity,
        path:       PathBuf,
        env_filter: String,
    },
    /// Send a network instruction to the running visualization
    SendInstruction {
        entity:      Entity,
        instruction: Instruction,
    },
    /// Terminate the visualization process (with optional timeout)
    /// only used when the Shutdown Instruction fails
    /// doing it this way allows sending the shutdown message for a graceful
    /// shutdown - Terminate is not graceful
    Shutdown { entity: Entity, timeout: Duration },
}

/// Messages sent from the async worker back to Bevy systems
/// Once completed, the Bevy systems can take appropriate action to update components
#[derive(Debug)]
pub enum AsyncOutcome {
    /// Visualization process was started and connected successfully
    Started { entity: Entity },
    /// An instruction was sent successfully
    InstructionSent {
        entity:      Entity,
        instruction: Instruction,
    },
    /// A visualization has shut down
    Shutdown { entity: Entity },
    /// An error occurred
    Error {
        entity: Entity,
        error:  Report<Error>,
    },
}
