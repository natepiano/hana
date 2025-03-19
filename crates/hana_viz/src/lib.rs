//! library to connect to and work with hana visualizations running in a separate processes
//! the simple call flow we have right now just starts, pings and shuts down the visualization
//! but we have the framework in place to handle network communication via an async runtime
//! the worker within the runtime exposes a send and receive channel so we can send messages
//! from the synchronously executing bevy ecs to the async worker
//!
//! in turn the async worker is polled in a while loop - constantly looking to receive command / instructions
//! with each new instruction, it invokes the closure created by the async worker which in turn
//! processes the instruction - the return values from those instructions are sent back
//! where our worker can try_receive them to see if any async tasks have finished and it can
//! then do the right next thing
//!
//! both the responding to events from hana keyboard bindings and sending messages to the runtime
//! and then the receiving of messages back from the runtime are handled in event_systems.rs
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
