//!  Communication Flow
//! 1. **Command Flow (Bevy → Async)**:
//!    - Bevy system calls `worker.send(someCommand)`
//!    - Command is sent through channel to async context
//!    - Worker processes command via the provided callback function
//!
//! 2. **Message Flow (Async → Bevy)**:
//!    - Async operation completes and produces results
//!    - Results are sent as messages through return channel
//!    - Bevy polling system calls `worker.try_receive()` to collect messages

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use error_stack::{Report, ResultExt};
use hana_async::AsyncRuntime;
use hana_network::{HanaEndpoint, Instruction};
use hana_process::Process;
use tokio::sync::Mutex;

use crate::async_messages::{AsyncInstruction, AsyncOutcome};
use crate::async_worker::VisualizationWorker;
use crate::error::{Error, Result};
use crate::visualizations::Visualizations;

/// System to initialize the visualization worker
/// all async handlers bubble up errors that are added to a
pub fn setup_visualization_worker(mut commands: Commands, async_runtime: Res<AsyncRuntime>) {
    // Create the hashmap of visualizations that we will interact with for networking and process
    // management
    let visualizations = Arc::new(Mutex::new(Visualizations::new()));

    // Create the worker using the new pattern
    // a hana_async::Worker expects the current runtime which we've already inserted as a resource
    // and a closure handling all of the messages coming from the ECS systems that want to talk the
    // async runtime
    //
    // we create it here then we add it as a resource so it can be queried by the ECS systems in event_systems
    // and used to call back into the async runtime
    let worker =
        hana_async::AsyncWorker::new(&async_runtime, move |instruction: AsyncInstruction| {
            let visualizations = Arc::clone(&visualizations);
            async move {
                match instruction {
                    AsyncInstruction::Start {
                        entity,
                        path,
                        env_filter,
                    } => match handle_start(visualizations.clone(), entity, path, env_filter).await
                    {
                        Ok(()) => AsyncOutcome::Started { entity },
                        Err(err) => AsyncOutcome::Error { entity, error: err },
                    },
                    AsyncInstruction::SendInstruction {
                        entity,
                        instruction,
                    } => {
                        let instruction_clone = instruction.clone();

                        match handle_send_instruction(visualizations.clone(), entity, instruction)
                            .await
                        {
                            Ok(()) => AsyncOutcome::InstructionSent {
                                entity,
                                instruction: instruction_clone,
                            },
                            Err(err) => AsyncOutcome::Error { entity, error: err },
                        }
                    }
                    AsyncInstruction::Shutdown { entity, timeout } => {
                        match handle_shutdown(visualizations.clone(), entity, timeout).await {
                            Ok(()) => AsyncOutcome::Shutdown { entity },
                            Err(err) => AsyncOutcome::Error { entity, error: err },
                        }
                    }
                }
            }
        });

    // Insert the worker as a resource
    commands.insert_resource(VisualizationWorker(worker));
}

/// Handle starting a visualization
pub async fn handle_start(
    visualizations: Arc<Mutex<Visualizations>>,
    entity: Entity,
    path: PathBuf,
    env_filter: String,
) -> Result<()> {
    // Start the new process
    debug!(
        "AsyncInstruction::Start received - starting process for entity {:?}: {:?}",
        entity, path
    );

    // Use Process::run to start the process
    let process = Process::run(path.clone(), env_filter.clone())
        .await
        .change_context(Error::Process)?;

    // Try to connect to it
    debug!(
        "AsyncInstruction::Start - process started for entity {:?}, connecting...",
        entity
    );
    let endpoint = HanaEndpoint::connect_to_visualization()
        .await
        .change_context(Error::Network)?;

    debug!(
        "AsyncInstruction::Start - successfully connected to visualization for entity {:?}",
        entity
    );

    // Store in state
    let mut state_guard = visualizations.lock().await;
    state_guard
        .active_visualizations
        .insert(entity, (process, endpoint));

    Ok(())
}

/// Handle terminating a visualization process
/// give it a bit to wait for the graceful shutdown
pub async fn handle_shutdown(
    visualizations: Arc<Mutex<Visualizations>>,
    entity: Entity,
    timeout: Duration,
) -> Result<()> {
    debug!(
        "AsyncInstruction::Shutdown - attempting to terminate visualization for entity {:?} with timeout: {:?}",
        entity, timeout
    );

    // Get the process, removing it from visualizations map
    let process = {
        let mut vis_guard = visualizations.lock().await;
        match vis_guard.active_visualizations.remove(&entity) {
            Some((process, _)) => process,
            None => {
                debug!(
                    "AsyncInstruction::Shutdown - no active visualization found for entity {:?}",
                    entity
                );
                return Ok(()); // Already removed
            }
        }
    };

    // Use the existing ensure_shutdown method
    match process.ensure_shutdown(timeout).await {
        Ok(()) => {
            debug!(
                "AsyncInstruction::Shutdown - process for entity {:?} exited gracefully",
                entity
            );
        }
        Err(err) => {
            // This happens when the timeout was reached and the process was killed
            debug!(
                "AsyncInstruction::Shutdown - process for entity {:?} timed out and was forcibly terminated: {:?}",
                entity, err
            );
            // We don't propagate this error since the process was successfully killed
        }
    }

    Ok(())
}

/// Handle sending an instruction to the visualization
pub async fn handle_send_instruction(
    visualizations: Arc<Mutex<Visualizations>>,
    entity: Entity,
    instruction: Instruction,
) -> Result<()> {
    // Lock the mutex and work with the data inside the critical section
    let mut visualizations_guard = visualizations.lock().await;

    if let Some((_, endpoint)) = visualizations_guard.active_visualizations.get_mut(&entity) {
        debug!(
            "AsyncInstruction::SendInstruction to entity {:?}: {:?}",
            entity, instruction
        );

        // Send the instruction
        endpoint
            .send(&instruction)
            .await
            .change_context(Error::Network)
            .attach_printable(format!(
                "Failed to send {:?} to entity {:?}",
                instruction, entity
            ))?;

        debug!(
            "AsyncInstruction::SendInstruction - sent successfully to entity {:?}",
            entity
        );

        // Remove the cleanup for shutdown instructions - this is now handled by terminate
        // This is the key change - we're not removing the visualization immediately
        // which gives the shutdown message time to reach the process

        Ok(())
    } else {
        // Return error if no visualization found
        Err(
            Report::new(Error::NoActiveVisualization).attach_printable(format!(
                "No active visualization to send instruction to for entity {:?}",
                entity
            )),
        )
    }
}
