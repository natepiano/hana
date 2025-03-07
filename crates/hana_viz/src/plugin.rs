//! Main plugin for hana_viz
use bevy::prelude::*;
use hana_async::AsyncRuntime;

use crate::entity::*;
use crate::observers::*;
use crate::runtime::*;
use crate::systems::*;

/// Main plugin for visualization management
pub struct HanaVizPlugin;

impl Plugin for HanaVizPlugin {
    fn build(&self, app: &mut App) {
        // Setup runtime resources (using the on_startup pattern)
        app.add_systems(Startup, setup_viz_runtime);

        // Register events
        app.add_event::<StartVisualization>()
            .add_event::<ShutdownVisualization>()
            .add_event::<SendInstruction>();

        // Add systems when they're created
        app.add_systems(
            Update,
            (
                process_outcomes_from_runtime,
                handle_start_visualization_requests,
                handle_shutdown_visualization_requests,
                handle_send_instruction_requests,
            ),
        );

        // Add observers
        app.add_observer(on_visualization_start);
        app.add_observer(on_visualization_connected);
        app.add_observer(on_visualization_disconnected);
        app.add_observer(on_process_terminated);
        app.add_observer(on_visualization_shutdown_complete);
    }
}

/// System to initialize the visualization runtime
fn setup_viz_runtime(mut commands: Commands, async_runtime: Res<AsyncRuntime>) {
    let (cmd_sender, event_receiver) = setup_visualization_runtime(&async_runtime);
    commands.insert_resource(cmd_sender);
    commands.insert_resource(event_receiver);
}
