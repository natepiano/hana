//! visualization control bindings (typically to keyboard input) are
//! turned into events in hana::basic_viz (currently)
//! these events are handled with these systems
//!
//! handle_* sends messages to the async worker with worker.send
//! process_worker_outcomes gets the messages back from the async worker with worker.try_receive
use std::time::Duration;

use bevy::prelude::*;
use hana_network::Instruction;

use crate::async_messages::{AsyncInstruction, AsyncOutcome};
use crate::async_worker::VisualizationWorker;
use crate::visualization::{
    SendInstructionEvent, ShutdownVisualizationEvent, StartVisualizationEvent, Visualization,
};
use crate::visualizations::{PendingConnections, VisualizationDetails};

/// Handles StartVisualization events by creating or updating visualization entities
pub fn handle_start_visualization_event(
    mut commands: Commands,
    mut start_events: EventReader<StartVisualizationEvent>,
    mut pending_connections: ResMut<PendingConnections>,
    worker: Res<VisualizationWorker>,
) {
    for event in start_events.read() {
        // Reserve an entity ID but don't populate it yet
        let entity = commands.spawn_empty().id();

        // Store details for when connection completes
        pending_connections.pending.insert(
            entity,
            VisualizationDetails {
                path: event.path.clone(),
                name: event.name.clone(),
                env_filter: event.env_filter.clone(),
            },
        );

        info!(
            "StartVisualizationEvent: {} (path: {:?})",
            event.name, event.path
        );

        // Send command to worker
        if let Err(e) = worker.send(AsyncInstruction::Start {
            entity,
            path: event.path.clone(),
            env_filter: event.env_filter.clone(),
        }) {
            error!("Failed to send start command: {:?}", e);
            commands.entity(entity).despawn();
            pending_connections.pending.remove(&entity);
        }
    }
}

/// Handles ShutdownVisualization events
pub fn handle_shutdown_visualization_event(
    mut shutdown_events: EventReader<ShutdownVisualizationEvent>,
    visualizations: Query<&Visualization>,
    worker: Res<VisualizationWorker>,
) {
    for event in shutdown_events.read() {
        // Check if the entity exists and has a Visualization component
        if visualizations.get(event.entity).is_ok() {
            info!("ShutdownVisualizationEvent: entity {:?}", event.entity);

            // First send shutdown instruction to worker for graceful shutdown
            if let Err(e) = worker.send(AsyncInstruction::SendInstruction {
                entity: event.entity,
                instruction: Instruction::Shutdown,
            }) {
                error!("Failed to send shutdown instruction: {:?}", e);
            }

            // Always follow up with a terminate command that will wait for graceful shutdown
            // and force terminate only if needed
            if let Err(e) = worker.send(AsyncInstruction::Shutdown {
                entity: event.entity,
                timeout: Duration::from_millis(event.timeout_ms),
            }) {
                error!("Failed to send terminate command: {:?}", e);
            }
        }
    }
}

/// Handles SendInstruction events
pub fn handle_send_instruction_event(
    mut instruction_events: EventReader<SendInstructionEvent>,
    visualizations: Query<&Visualization>,
    worker: Res<VisualizationWorker>,
) {
    for event in instruction_events.read() {
        // Check if the entity exists and has a Visualization component
        if visualizations.get(event.entity).is_ok() {
            info!("SendInstructionEvent: {:?}", event.instruction);

            // Send command to worker
            if let Err(e) = worker.send(AsyncInstruction::SendInstruction {
                entity: event.entity,
                instruction: event.instruction.clone(),
            }) {
                error!("Failed to send instruction: {:?}", e);
            }
        }
    }
}

/// process_async_outcome handles the ECS side of things
/// it receives all the AsyncOutcomes and does things like
/// taking the pending entity and adding a Visualization to it for real so
/// that now it can be used in an ECS system
pub fn process_async_outcomes(
    mut commands: Commands,
    mut pending_connections: ResMut<PendingConnections>,
    worker: Res<VisualizationWorker>,
) {
    // Try to receive all pending messages
    while let Some(outcome) = worker.try_receive() {
        match outcome {
            AsyncOutcome::Started { entity } => {
                // remove it from Pending and populate it with its visualization
                if let Some(details) = pending_connections.pending.remove(&entity) {
                    info!(
                        "AsyncOutcome::Started: {} (entity: {:?})",
                        details.name, entity
                    );

                    commands.entity(entity).insert(Visualization {
                        path: details.path,
                        name: details.name,
                        env_filter: details.env_filter,
                    });
                }
            }
            AsyncOutcome::InstructionSent {
                entity,
                instruction,
            } => {
                // currently we don't do anyting but log that this happened
                // possibly in the future there would be some use for this
                debug!(
                    "AsyncOutcome::InstructionSent to visualization: {:?} (entity: {:?})",
                    instruction, entity
                );
            }
            AsyncOutcome::Shutdown { entity } => {
                info!(
                    "AsyncOutcome::Shutdown: entity {:?} - despawning entity",
                    entity
                );

                // back here in ECS world, with the process shutdown, we need to despawn the visualization to clean
                // everything up
                commands.entity(entity).despawn();
            }
            AsyncOutcome::Error { entity, error } => {
                error!("AsyncOutcome::Error: {:?} (entity: {:?})", error, entity);

                // Add to failed list if it was a pending connection
                if let Some(details) = pending_connections.pending.remove(&entity) {
                    pending_connections
                        .failed
                        .push((details, format!("{:?}", error)));
                }

                // Clean up the entity
                commands.entity(entity).despawn();
            }
        }
    }
}
