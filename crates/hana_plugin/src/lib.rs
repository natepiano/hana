//! exposes the VisualizationControl plugin for use in bevy based visualizations
mod error;
mod instruction_receiver;
mod plugin;
mod prelude;
use instruction_receiver::InstructionReceiver;
/// The `HanaPlugin` is a Bevy plugin for hana's remote control of your
/// visualization.
///
/// # Example
///
/// ```rust
/// # use hana_plugin::HanaPlugin;
/// # use bevy::prelude::*;
///
/// App::new()
///     .add_plugins(HanaPlugin)
///     // other app setup code
///     .run();
/// ```
pub use plugin::{HanaEvent, HanaPlugin};
pub use prelude::*;
