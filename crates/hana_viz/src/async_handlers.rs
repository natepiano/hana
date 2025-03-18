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
    // Create shared state for the worker
    let visualizations = Arc::new(Mutex::new(Visualizations::new()));

    // Create the worker using the new pattern
    // currently returning a vec because within handle_send_instruction we check if it was a shutdown message
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
                    Ok(()) => vec![AsyncOutcome::Started { entity }],
                    Err(err) => vec![AsyncOutcome::Error { entity, error: err }],
                },
                AsyncInstruction::SendInstructions {
                    entity,
                    instruction,
                } => {
                    let instruction_clone = instruction.clone();
                    let was_shutdown = matches!(instruction, Instruction::Shutdown);

                    match handle_send_instruction(visualizations.clone(), entity, instruction).await
                    {
                        Ok(()) => {
                            let mut events = vec![AsyncOutcome::InstructionSent {
                                entity,
                                instruction: instruction_clone,
                            }];

                            if was_shutdown {
                                events.push(AsyncOutcome::Shutdown { entity });
                            }

                            events
                        }
                        Err(err) => vec![AsyncOutcome::Error { entity, error: err }],
                    }
                }
                AsyncInstruction::Terminate { entity, timeout } => {
                    match handle_terminate(visualizations.clone(), entity, timeout).await {
                        Ok(()) => vec![AsyncOutcome::Shutdown { entity }],
                        Err(err) => vec![AsyncOutcome::Error { entity, error: err }],
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
pub async fn handle_terminate(
    visualizations: Arc<Mutex<Visualizations>>,
    entity: Entity,
    timeout: Duration,
) -> Result<()> {
    // Take the specific visualization out to avoid lock issues
    let viz_option = {
        let mut visualizations_guard = visualizations.lock().await;
        visualizations_guard.active_visualizations.remove(&entity)
    };

    if let Some((process, _)) = viz_option {
        debug!(
            "Terminating visualization process for entity {:?} with timeout: {:?}",
            entity, timeout
        );

        // Since we don't have a direct terminate method with timeout,
        // we'll just drop the process which should clean up
        // In a real implementation, this would need proper process termination logic
        drop(process);
        Ok(())
    } else {
        debug!(
            "No active visualization to terminate for entity {:?}",
            entity
        );
        // Still return OK if there's no process to terminate
        Ok(())
    }
}

/// Handle sending an instruction to the visualization
pub async fn handle_send_instruction(
    visualizations: Arc<Mutex<Visualizations>>,
    entity: Entity,
    instruction: Instruction,
) -> Result<()> {
    let was_shutdown = matches!(instruction, Instruction::Shutdown);

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

        // If it was a shutdown instruction, remove the visualization
        if was_shutdown {
            visualizations_guard.active_visualizations.remove(&entity);
        }

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
