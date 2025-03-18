//! visualization control bindings (typically to keyboard input) are
//! turned into events in hana::basic_viz (currently)
//! these events are handled with these systems
//! the intent is that we can't have a Visualization in a state where the message
//! will be "incorrect" so we have the various states for a visualization as defined
//! in entity.rs
use std::time::Duration;

use bevy::prelude::*;
use hana_network::Instruction;

use crate::async_messages::{AsyncInstruction, AsyncOutcome};
use crate::async_worker::VisualizationWorker;
use crate::visualization::{
    Connected, Disconnected, SendInstructionEvent, ShutdownVisualizationEvent, ShuttingDown,
    StartVisualizationEvent, Starting, Unstarted, Visualization,
};

/// Handles StartVisualization events by creating or updating visualization entities
pub fn handle_start_visualization_event(
    mut commands: Commands,
    mut start_events: EventReader<StartVisualizationEvent>,
    unstarted: Query<&Visualization, With<Unstarted>>,
    worker: Res<VisualizationWorker>,
) {
    for event in start_events.read() {
        match event.entity {
            // Update existing visualization
            Some(entity) => {
                if let Ok(viz) = unstarted.get(entity) {
                    info!("Starting existing visualization: {}", viz.name);

                    // Update entity state
                    commands
                        .entity(entity)
                        .remove::<Unstarted>()
                        .insert(Starting);

                    // Send command to worker
                    if let Err(e) = worker.send(AsyncInstruction::Start {
                        entity,
                        path: viz.path.clone(),
                        env_filter: viz.env_filter.clone(),
                    }) {
                        error!("Failed to send start command: {:?}", e);
                    }
                }
            }
            // Create new visualization
            None => {
                if let Some(path) = &event.path {
                    let name = event.name.clone().unwrap_or_else(|| {
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Unnamed Visualization")
                            .to_string()
                    });

                    let env_filter = event.env_filter.clone().unwrap_or_else(|| {
                        std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string())
                    });

                    let visualization = Visualization {
                        path: path.clone(),
                        name: name.clone(),
                        env_filter: env_filter.clone(),
                    };

                    // Spawn entity first
                    let entity = commands.spawn((visualization, Starting)).id();

                    info!(
                        "StartVisualizationEvent received for: {} - sending AsyncInstruction::Start",
                        name
                    );

                    // Send command to worker
                    if let Err(e) = worker.send(AsyncInstruction::Start {
                        entity,
                        path: path.clone(),
                        env_filter,
                    }) {
                        error!(
                            "Failed to send start command for new visualization: {:?}",
                            e
                        );
                    }
                }
            }
        }
    }
}

/// Handles ShutdownVisualization events
pub fn handle_shutdown_visualization_event(
    mut commands: Commands,
    mut shutdown_events: EventReader<ShutdownVisualizationEvent>,
    connected: Query<Entity, With<Connected>>,
    worker: Res<VisualizationWorker>,
) {
    for event in shutdown_events.read() {
        if connected.get(event.entity).is_ok() {
            info!("Shutting down visualization: {:?}", event.entity);

            // Update entity state
            commands
                .entity(event.entity)
                .remove::<Connected>()
                .insert(ShuttingDown);

            // Send command to worker
            if let Err(e) = worker.send(AsyncInstruction::SendInstructions {
                entity: event.entity,
                instruction: Instruction::Shutdown,
            }) {
                error!("Failed to send shutdown instruction: {:?}", e);
            }

            // Set a timeout to force terminate if needed
            if let Err(e) = worker.send(AsyncInstruction::Terminate {
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
    connected: Query<Entity, With<Connected>>,
    worker: Res<VisualizationWorker>,
) {
    for event in instruction_events.read() {
        if connected.get(event.entity).is_ok() {
            info!(
                "Sending instruction to visualization: {:?}",
                event.instruction
            );

            // Send command to worker
            if let Err(e) = worker.send(AsyncInstruction::SendInstructions {
                entity: event.entity,
                instruction: event.instruction.clone(),
            }) {
                error!("Failed to send instruction: {:?}", e);
            }
        }
    }
}

/// process_worker_outcome moves our entity states along
/// which makes it easy for observers to track where we're at and take action
/// action that doesn't have to be taken here as this basically just handles
/// the component state changes
pub fn process_worker_outcomes(mut commands: Commands, worker: Res<VisualizationWorker>) {
    // Try to receive all pending messages
    while let Some(event) = worker.try_receive() {
        match event {
            AsyncOutcome::Started { entity } => {
                info!(
                    "AsyncOutcome::Started received from hana_async::Worker for entity: {:?} updating marker component to Connected",
                    entity
                );

                // Update entity state - just mark as Connected
                commands
                    .entity(entity)
                    .remove::<Starting>()
                    .insert(Connected);
            }
            AsyncOutcome::InstructionSent {
                entity,
                instruction,
            } => {
                debug!(
                    "AsyncOutcome::InstructionSent received from hana_async::Worker: {:?} for entity {:?}",
                    instruction, entity
                );
                // Could update UI or other state here if needed
            }
            AsyncOutcome::Shutdown { entity } => {
                info!(
                    "AsyncOutcome::Shutdown received from hana_async::Worker for entity: {:?} - moving to Unstarted state",
                    entity
                );

                commands
                    .entity(entity)
                    .remove::<Connected>()
                    .remove::<ShuttingDown>()
                    .insert(Unstarted);
            }
            AsyncOutcome::Error { entity, error } => {
                // Change from report to error
                error!(
                    "AsyncOutcome::Error received from hana_async::Worker for entity: {:?} error: {:?} moving to Disconnected state",
                    entity, error
                );

                commands
                    .entity(entity)
                    .remove::<Starting>()
                    .remove::<Connected>()
                    .insert(Disconnected {
                        error: Some(format!("{:?}", error)),
                    });
            }
        }
    }
}
