//! - `AsyncRuntime`: Manages the Tokio multithreaded async runtime wrapped in an Arc and added to
//!   the world as a resource
use std::sync::Arc;

use bevy::prelude::*;
use error_stack::{Result, ResultExt};
use tokio::runtime::Runtime;

use crate::error::Error;

/// Resource that provides access to a Tokio runtime
#[derive(Resource)]
pub struct AsyncRuntime {
    pub(crate) runtime: Arc<Runtime>,
}

/// used to initialize the tokio async runtime and add it as a resource
/// exits the app with an error code if the runtime creation fails
/// currently this is the only app exit we are aware of
pub fn init_async_runtime(mut commands: Commands, mut exit: EventWriter<AppExit>) {
    let result = AsyncRuntime::new();
    match result {
        Ok(runtime) => commands.insert_resource(runtime),
        Err(report) => {
            error!("CRITICAL ERROR: {report:?}");
            exit.send(AppExit::from_code(hana_const::EXIT_ASYNC_RUNTIME_ERROR));
        }
    }
}

impl AsyncRuntime {
    /// Create a new AsyncRuntime with a multi-threaded Tokio runtime
    pub fn new() -> Result<Self, Error> {
        let runtime = Arc::new(Runtime::new().change_context(Error::RuntimeCreationFailed)?);
        Ok(Self { runtime })
    }
}
