//! library to connect to and work with hana visualizations running in a separate processes
mod async_handlers;
mod async_messages;
mod async_worker;
mod error;
mod event_systems;
pub(crate) mod observers;
mod plugin;
mod visualization;
pub(crate) mod visualizations;

// Public exports for use by hana app
pub use error::{Error, Result};
pub use plugin::HanaVizPlugin;
pub use visualization::{
    SendInstructionEvent, ShutdownVisualizationEvent, StartVisualizationEvent, Visualization,
};
pub use visualizations::PendingConnections;
