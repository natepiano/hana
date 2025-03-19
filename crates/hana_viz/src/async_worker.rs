//! our specific async worker which wraps a hana_async::Worker and
//! specifically has it work with AsyncInstruction and AsyncOutcome
//! it then delegates the send and the try_receive methods to the underlying Worker
//! we could just use worker directly but this way we have an explicit name
//! for adding as a resource to bevy
use bevy::prelude::*;
use error_stack::ResultExt;
use hana_async::AsyncWorker;

use crate::async_messages::{AsyncInstruction, AsyncOutcome};
use crate::error::{Error, Result};

/// Resource that manages the visualization worker
#[derive(Resource)]
pub struct VisualizationWorker(pub AsyncWorker<AsyncInstruction, AsyncOutcome>);

impl VisualizationWorker {
    /// Send an instruction to the visualization worker
    pub fn send_instruction(&self, instruction: impl Into<AsyncInstruction>) -> Result<()> {
        let instruction = instruction.into();

        self.0
            .send_instruction(instruction)
            .change_context(Error::AsyncWorker)
    }

    /// Try to receive a message from the visualization worker
    pub fn try_receive(&self) -> Option<AsyncOutcome> {
        self.0.try_receive()
    }
}
