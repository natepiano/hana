use std::path::PathBuf;

use bevy::prelude::*;
use hana_network::Instruction;
use hana_viz::{Connected, SendInstruction, ShutdownVisualization, StartVisualization};
use tracing::info;

use crate::action::*;

/// Proof of concept plugin to control a visualization for basic functionality
pub struct BasicVizPlugin;

impl Plugin for BasicVizPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                viz_start.run_if(just_pressed(Action::Start)),
                viz_ping.run_if(just_pressed(Action::Ping)),
                viz_shutdown.run_if(just_pressed(Action::Shutdown)),
            ),
        );
    }
}

fn viz_start(mut start_events: EventWriter<StartVisualization>) {
    info!("Starting visualization via hana_viz...");

    // Create event to start visualization
    start_events.send(StartVisualization {
        entity: None, // Create a new entity
        path: Some(PathBuf::from("./target/debug/basic-visualization")),
        name: Some("Basic Visualization".to_string()),
        env_filter: Some(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string())),
        tags: vec!["basic".to_string()],
    });
}

fn viz_ping(
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

fn viz_shutdown(
    viz_query: Query<Entity, With<Connected>>,
    mut shutdown_events: EventWriter<ShutdownVisualization>,
) {
    info!("Shutting down visualization via hana_viz...");

    // Find first connected visualization
    if let Some(entity) = viz_query.iter().next() {
        // Send shutdown event
        shutdown_events.send(ShutdownVisualization {
            entity,
            timeout_ms: 5000, // 5 seconds timeout
        });
    } else {
        warn!("No connected visualization to shut down");
    }
}
