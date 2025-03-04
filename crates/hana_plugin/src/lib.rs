//! exposes the HanaPlugin for use in bevy based visualizations
//! The `HanaPlugin` is a Bevy plugin for hana's remote control of your
//! visualization.
//!
//! # Example
//!
//! ```rust,ignore
//! # use hana_plugin::HanaPlugin;
//! # use bevy::prelude::*;
//!
//! App::new()
//!     .add_plugins(HanaPlugin)
//!     // other app setup code
//!     .run();
//! ```
//!
//! Issues
//! - we need to handle disconnects such as in try_recv getting an error in hande_instructions
mod error;
mod instruction_receiver;
mod plugin;
mod prelude;

use instruction_receiver::InstructionReceiver;
pub use plugin::{HanaEvent, HanaPlugin};
pub use prelude::*;
