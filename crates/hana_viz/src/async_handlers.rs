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
    // Create the hashmap of visualizations that we will interact with for networking and process management
    let visualizations = Arc::new(Mutex::new(Visualizations::new()));

    // Create the worker using the new pattern
    // currently returning a  because within handle_send_instruction we check if it was a shutdown message
    // and we add an extra AsyncOutcome::Shutdown that goes along with the AsyncOutcome::Instruction
    let worker = hana_async::Worker::new(&async_runtime, move |instruction: AsyncInstruction| {
        let visualizations = Arc::clone(&visualizations);
        async move {
            match instruction {
                AsyncInstruction::Start {
                    entity,
                    path,
                    env_filter,
                } => match handle_start(visualizations.clone(), entity, path, env_filter).await {
                    Ok(()) => AsyncOutcome::Started { entity },
                    Err(err) => AsyncOutcome::Error { entity, error: err },
                },
                AsyncInstruction::SendInstruction {
                    entity,
                    instruction,
                } => {
                    let instruction_clone = instruction.clone();

                    match handle_send_instruction(visualizations.clone(), entity, instruction).await
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
        "Attempting to terminate visualization for entity {:?} with timeout: {:?}",
        entity, timeout
    );

    // Start time for tracking timeout
    let start_time = std::time::Instant::now();

    // First, try to wait for graceful shutdown
    loop {
        // Check if timeout elapsed
        if start_time.elapsed() >= timeout {
            debug!(
                "Timeout reached for entity {:?}, forcing termination",
                entity
            );
            break;
        }

        // Check process status
        let mut visualizations_guard = visualizations.lock().await;
        if let Some(entry) = visualizations_guard.active_visualizations.get_mut(&entity) {
            let (process, _) = entry;
            match process.is_running().await {
                Ok(false) => {
                    // Process has already exited gracefully
                    debug!("Process for entity {:?} has gracefully shutdown", entity);
                    // Remove it from active_visualizations and return success
                    visualizations_guard.active_visualizations.remove(&entity);
                    return Ok(());
                }
                Ok(true) => {
                    // Still running, release lock and wait a bit
                    drop(visualizations_guard);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                Err(e) => {
                    // Error checking status, log and break to force termination
                    debug!("Error checking process status: {:?}", e);
                    break;
                }
            }
        } else {
            // Process already removed
            debug!("No active visualization found for entity {:?}", entity);
            return Ok(());
        }
    }

    // If we reach here, we need to force termination
    // Take the visualization out of the map
    let viz_option = {
        let mut visualizations_guard = visualizations.lock().await;
        visualizations_guard.active_visualizations.remove(&entity)
    };

    if let Some((process, _)) = viz_option {
        debug!("Forcefully terminating process for entity {:?}", entity);
        // The process will be dropped here, which should clean up resources
        drop(process);
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
            "Sending instruction to entity {:?}: {:?}",
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

        debug!("Instruction sent successfully to entity {:?}", entity);

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
