//! basic visualization handler
//! actions from leafwing are handled and sent as events which will be read in hana_viz

use std::path::PathBuf;

use bevy::prelude::*;
use hana_network::Instruction;
use hana_viz::{
    SendInstructionEvent, ShutdownVisualizationEvent, StartVisualizationEvent, Visualization,
};
use tracing::info;

use crate::action::*;

// at some point this should be a system setting
// although i don't know if we'll ever care if timeouts are more
// than 5 seconds as it seems like a pretty good number
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

fn start_system(mut start_writer: EventWriter<StartVisualizationEvent>) {
    info!("F1 press sends StartVisualizationEvent");

    // Create event to start visualization
    start_writer.send(StartVisualizationEvent {
        path:       PathBuf::from("./target/debug/basic-visualization"),
        name:       "basic-visualization".to_string(),
        env_filter: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
    });
}

fn ping_system(
    viz_query: Query<Entity, With<Visualization>>,
    mut instruction_writer: EventWriter<SendInstructionEvent>,
) {
    info!("P press sends SendInstructionEvent with Instruction::Ping");

    for entity in viz_query.iter() {
        instruction_writer.send(SendInstructionEvent {
            entity,
            instruction: Instruction::Ping,
        });
    }
}

fn shutdown_system(
    viz_query: Query<Entity, With<Visualization>>,
    mut shutdown_writer: EventWriter<ShutdownVisualizationEvent>,
) {
    info!("F2 press sends ShutdownVisualizationEvent");

    // Find first connected visualization
    if let Some(entity) = viz_query.iter().next() {
        // Send shutdown event
        shutdown_writer.send(ShutdownVisualizationEvent {
            entity,
            timeout_ms: VISUALIZATION_SHUTDOWN_TIMEOUT_MS, // 5 seconds timeout
        });
    } else {
        warn!("No connected visualization to shut down");
    }
}
