use std::time::Duration;

use bevy::prelude::*;
use hana_network::Instruction;

use crate::entity::*;
use crate::runtime::{RuntimeOutcomeMessage, RuntimeTask, RuntimeTaskSender};

/// Handles StartVisualization events by creating or updating visualization entities
pub fn handle_start_visualization_requests(
    mut commands: Commands,
    mut start_events: EventReader<StartVisualization>,
    unstarted: Query<&Visualization, With<Unstarted>>,
    cmd_sender: Res<RuntimeTaskSender>,
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

                    // Send command to async worker
                    let _ = cmd_sender.0.send(RuntimeTask::Start {
                        entity,
                        path: viz.path.clone(),
                        env_filter: viz.env_filter.clone(),
                    });
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

                    info!("Starting new visualization: {}", name);

                    // Send command to async worker via flume channel
                    let _ = cmd_sender.0.send(RuntimeTask::Start {
                        entity,
                        path: path.clone(),
                        env_filter,
                    });
                }
            }
        }
    }
}

/// Handles ShutdownVisualization events
pub fn handle_shutdown_visualization_requests(
    mut commands: Commands,
    mut shutdown_events: EventReader<ShutdownVisualization>,
    connected: Query<Entity, With<Connected>>,
    cmd_sender: Res<RuntimeTaskSender>,
) {
    for event in shutdown_events.read() {
        if connected.get(event.entity).is_ok() {
            info!("Shutting down visualization: {:?}", event.entity);

            // Update entity state
            commands
                .entity(event.entity)
                .remove::<Connected>()
                .insert(ShuttingDown);

            // Send command to async worker
            let _ = cmd_sender.0.send(RuntimeTask::Send {
                entity: event.entity,
                instruction: Instruction::Shutdown,
            });

            // Set a timeout to force terminate if needed
            let _ = cmd_sender.0.send(RuntimeTask::Terminate {
                entity: event.entity,
                timeout: Duration::from_millis(event.timeout_ms),
            });
        }
    }
}

/// Handles SendInstruction events
pub fn handle_send_instruction_requests(
    mut instruction_events: EventReader<SendInstruction>,
    connected: Query<Entity, With<Connected>>,
    cmd_sender: Res<RuntimeTaskSender>,
) {
    for event in instruction_events.read() {
        if connected.get(event.entity).is_ok() {
            info!(
                "Sending instruction to visualization: {:?}",
                event.instruction
            );

            // Send command to async worker
            let _ = cmd_sender.0.send(RuntimeTask::Send {
                entity: event.entity,
                instruction: event.instruction.clone(),
            });
        }
    }
}

// Process events coming backfrom the visualization runtime
pub fn process_outcomes_from_runtime(
    mut commands: Commands,
    messages: Res<crate::runtime::RuntimeMessageReceiver>,
) {
    // Try to receive all pending messages
    while let Some(event) = messages.0.try_recv() {
        match event {
            RuntimeOutcomeMessage::Started { entity } => {
                info!("Visualization started successfully: {:?}", entity);

                // Update entity state
                commands
                    .entity(entity)
                    .remove::<Starting>()
                    .insert(Connected);
            }
            RuntimeOutcomeMessage::InstructionSent {
                entity,
                instruction,
            } => {
                debug!("Instruction sent: {:?} to {:?}", instruction, entity);
                // Could update UI or other state here if needed
            }
            RuntimeOutcomeMessage::Shutdown { entity } => {
                info!("Visualization shut down: {:?}", entity);

                commands
                    .entity(entity)
                    .remove::<Connected>()
                    .remove::<ShuttingDown>()
                    .insert(Unstarted);
            }
            RuntimeOutcomeMessage::Error { entity, error } => {
                // Change from report to error
                error!("Visualization error for {:?}: {:?}", entity, error);

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
