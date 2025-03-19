//! Main plugin for hana_viz

use bevy::prelude::*;

use crate::{
    SendInstructionEvent, ShutdownVisualizationEvent, StartVisualizationEvent, async_handlers,
    event_systems, observers, visualizations::PendingConnections,
};

/// Main plugin for visualization management
pub struct HanaVizPlugin;

impl Plugin for HanaVizPlugin {
    fn build(&self, app: &mut App) {
        // Setup runtime resources
        app.init_resource::<PendingConnections>()
            .add_systems(Startup, async_handlers::setup_visualization_worker);

        // Register events
        app.add_event::<StartVisualizationEvent>()
            .add_event::<ShutdownVisualizationEvent>()
            .add_event::<SendInstructionEvent>();

        // Add systems
        app.add_systems(
            Update,
            (
                event_systems::process_async_outcomes,
                event_systems::handle_start_visualization_event,
                event_systems::handle_shutdown_visualization_event,
                event_systems::handle_send_instruction_event,
            ),
        );

        // Add observers
        app.add_observer(observers::on_visualization_added);
        app.add_observer(observers::on_visualization_removed);
    }
}
