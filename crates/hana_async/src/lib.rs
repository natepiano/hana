//! async runtime - covering networking, process management,
//! and channels to bridge between sync and async code
mod error;
mod runtime;
mod worker;

use bevy::prelude::*;
pub use runtime::AsyncRuntime;
pub use worker::AsyncWorker;

/// Plugin that adds async runtime support to a Bevy app
pub struct AsyncRuntimePlugin;

impl Plugin for AsyncRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, runtime::init_async_runtime);
    }
}
