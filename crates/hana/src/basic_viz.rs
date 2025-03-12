use std::path::PathBuf;

use bevy::prelude::*;
use hana_network::Instruction;
use hana_viz::{Connected, SendInstruction, ShutdownVisualization, StartVisualization};
use tracing::info;

use crate::action::*;

const VISUALIZATION_SHUTDOWN_TIMEOUT_MS: u64 = 5000;

/// Proof of concept plugin to control a visualization for basic functionality
pub struct BasicVizPlugin;

impl Plugin for BasicVizPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                start_system.run_if(just_pressed(Action::Start)),
                ping_system.run_if(just_pressed(Action::Ping)),
                shutdown_system.run_if(just_pressed(Action::Shutdown)),
            ),
        );
    }
}

fn start_system(mut start_events: EventWriter<StartVisualization>) {
    info!("Starting visualization via hana_viz...");

    // Create event to start visualization
    start_events.send(StartVisualization {
        entity:     None, // Create a new entity
        path:       Some(PathBuf::from("./target/debug/basic-visualization")),
        name:       Some("basic-visualization".to_string()),
        env_filter: Some(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string())),
    });
}

fn ping_system(
    viz_query: Query<Entity, With<Connected>>,
    mut instruction_events: EventWriter<SendInstruction>,
) {
    info!("Pinging visualization via hana_viz...");

    // Find first connected visualization
    if let Some(entity) = viz_query.iter().next() {
        // Send ping instruction
        instruction_events.send(SendInstruction {
            entity,
            instruction: Instruction::Ping,
        });
    } else {
        warn!("No connected visualization to ping");
    }
}

fn shutdown_system(
    viz_query: Query<Entity, With<Connected>>,
    mut shutdown_events: EventWriter<ShutdownVisualization>,
) {
    info!("Shutting down visualization via hana_viz...");

    // Find first connected visualization
    if let Some(entity) = viz_query.iter().next() {
        // Send shutdown event
        shutdown_events.send(ShutdownVisualization {
            entity,
            timeout_ms: VISUALIZATION_SHUTDOWN_TIMEOUT_MS, // 5 seconds timeout
        });
    } else {
        warn!("No connected visualization to shut down");
    }
}
