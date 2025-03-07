use std::time::Duration;

use bevy::prelude::*;
use hana_network::Instruction;

use crate::entity::*;
use crate::runtime::{VisualizationCommand, VisualizationCommandSender};

/// Handles StartVisualization events by creating or updating visualization entities
pub fn handle_start_visualization_requests(
    mut commands: Commands,
    mut start_events: EventReader<StartVisualization>,
    unstarted: Query<&Visualization, With<Unstarted>>,
    cmd_sender: Res<VisualizationCommandSender>,
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
                    let _ = cmd_sender.0.send(VisualizationCommand::Start {
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
                        path:       path.clone(),
                        name:       name.clone(),
                        env_filter: env_filter.clone(),
                        tags:       event.tags.clone(),
                    };

                    // Spawn entity first
                    let entity = commands.spawn((visualization, Starting)).id();

                    info!("Starting new visualization: {}", name);

                    // Send command to async worker
                    let _ = cmd_sender.0.send(VisualizationCommand::Start {
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
    cmd_sender: Res<VisualizationCommandSender>,
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
            let _ = cmd_sender.0.send(VisualizationCommand::Send {
                entity:      event.entity,
                instruction: Instruction::Shutdown,
            });

            // Set a timeout to force terminate if needed
            let _ = cmd_sender.0.send(VisualizationCommand::Terminate {
                entity:  event.entity,
                timeout: Duration::from_millis(event.timeout_ms),
            });
        }
    }
}

/// Handles SendInstruction events
pub fn handle_send_instruction_requests(
    mut instruction_events: EventReader<SendInstruction>,
    connected: Query<Entity, With<Connected>>,
    cmd_sender: Res<VisualizationCommandSender>,
) {
    for event in instruction_events.read() {
        if connected.get(event.entity).is_ok() {
            info!(
                "Sending instruction to visualization: {:?}",
                event.instruction
            );

            // Send command to async worker
            let _ = cmd_sender.0.send(VisualizationCommand::Send {
                entity:      event.entity,
                instruction: event.instruction.clone(),
            });
        }
    }
}

// Process events coming from the visualization runtime
pub fn process_visualization_events(
    mut commands: Commands,
    events: Res<crate::runtime::VisualizationEventReceiver>,
    mut state_events: EventWriter<VisualizationStateChanged>,
) {
    // Try to receive all pending events
    while let Some(event) = events.0.try_recv() {
        match event {
            crate::runtime::VisualizationEvent::Started { entity } => {
                info!("Visualization started successfully: {:?}", entity);

                // Update entity state
                commands
                    .entity(entity)
                    .remove::<Starting>()
                    .insert(Connected);

                state_events.send(VisualizationStateChanged {
                    entity,
                    new_state: "Connected".to_string(),
                    error: None,
                });
            }
            crate::runtime::VisualizationEvent::InstructionSent {
                entity,
                instruction,
            } => {
                debug!("Instruction sent: {:?} to {:?}", instruction, entity);
                // Could update UI or other state here if needed
            }
            crate::runtime::VisualizationEvent::Shutdown { entity } => {
                info!("Visualization shut down: {:?}", entity);

                commands
                    .entity(entity)
                    .remove::<Connected>()
                    .remove::<ShuttingDown>()
                    .insert(Unstarted);

                state_events.send(VisualizationStateChanged {
                    entity,
                    new_state: "Unstarted".to_string(),
                    error: None,
                });
            }
            crate::runtime::VisualizationEvent::Error { entity, error } => {
                // Change from report to error
                error!("Visualization error for {:?}: {:?}", entity, error);

                commands
                    .entity(entity)
                    .remove::<Starting>()
                    .remove::<Connected>()
                    .insert(Disconnected {
                        error: Some(format!("{:?}", error)),
                    });

                state_events.send(VisualizationStateChanged {
                    entity,
                    new_state: "Error".to_string(),
                    error: Some(format!("{:?}", error)),
                });
            }
        }
    }
}
