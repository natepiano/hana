//! library to connect to and work with hana visualizations running in a separate processes
//! the simple call flow we have right now just starts, pings and shuts down the visualization
//! but we have the framework in place to handle network communication via an async runtime
//! the worker within the runtime exposes a send and receive channel so we can send messages
//! from the synchronously executing bevy ecs to the async worker
//!
//! in turn the async worker is polled in a while loop - constantly looking to receive command /
//! instructions with each new instruction, it invokes the closure created by the async worker which
//! in turn processes the instruction - the return values from those instructions are sent back
//! where our worker can try_receive them to see if any async tasks have finished and it can
//! then do the right next thing
//!
mod async_handlers;
mod async_messages;
mod async_worker;
mod error;
mod event_systems;
mod observers;
mod visualization;
mod visualizations;

// Public exports for use by hana app
use bevy::prelude::*;
pub use error::{Error, Result};
pub use visualization::{
    SendInstructionEvent, ShutdownVisualizationEvent, StartVisualizationEvent, Visualization,
};
// probably temporary - exported for hana to do some error reporting
pub use visualizations::PendingConnections;

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
