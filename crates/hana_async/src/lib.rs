//! async runtime - covering networking, process management,
//! and channels to bridge between sync and async code
use std::future::Future;
use std::sync::Arc;

use bevy::prelude::*;
use error_stack::{Result, ResultExt};
use flume::{Receiver, Sender};
use tokio::runtime::Runtime;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Runtime channel closed")]
    ChannelClosed,

    #[error("Tokio runtime creation failed")]
    RuntimeCreationFailed,
}

/// Plugin that adds async runtime support to a Bevy app
pub struct AsyncRuntimePlugin;

impl Plugin for AsyncRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, init_async_runtime);
    }
}

/// Resource that provides access to a Tokio runtime
#[derive(Resource)]
pub struct AsyncRuntime {
    runtime: Arc<Runtime>,
}

/// used to initialize the tokio async runtime and add it as a resource
/// exits the app with an error code if the runtime creation fails
/// currently this is the only app exit we are aware of
fn init_async_runtime(mut commands: Commands, mut exit: EventWriter<AppExit>) {
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

    /// Get a reference to the Tokio runtime
    pub fn runtime(&self) -> &Arc<Runtime> {
        &self.runtime
    }

    /// Spawn a background task
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let rt = Arc::clone(&self.runtime);
        rt.spawn(future);
    }

    /// Create a channel pair for async communication
    pub fn create_channel<T>(&self) -> (Sender<T>, Receiver<T>)
    where
        T: Send + 'static,
    {
        flume::unbounded()
    }

    /// Helper method to create our bridge between the async worker and the main bevy ECS system
    /// thread(s)
    pub fn create_worker<Cmd, Msg, F, Fut>(
        &self,
        process_fn: F,
    ) -> (CommandSender<Cmd>, MessageReceiver<Msg>)
    where
        Cmd: Send + Clone + std::fmt::Debug + 'static,
        Msg: Send + 'static,
        F: Fn(Cmd) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future<Output = Vec<Msg>> + Send + 'static,
    {
        let (cmd_tx, cmd_rx) = flume::unbounded();
        let (msg_tx, msg_rx) = flume::unbounded();

        let runtime = Arc::clone(&self.runtime);

        // Spawn the worker thread directly here instead of calling a separate function
        std::thread::spawn(move || {
            runtime.block_on(async {
                while let Ok(command) = cmd_rx.recv_async().await {
                    let message_sender = msg_tx.clone();
                    let process = process_fn.clone();

                    runtime.spawn(async move {
                        let results = process(command).await;
                        for event in results {
                            let _ = message_sender.send(event);
                        }
                    });
                }
            });
        });

        // give the other side of the communication channels
        // back to the bevy side so they can send commands to the worker
        // and receive messages from the worker
        (CommandSender(cmd_tx), MessageReceiver(msg_rx))
    }

    /// Run a closure in a blocking context
    pub fn block_on<F, T>(&self, future: F) -> T
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.runtime.block_on(future)
    }
}

/// Typed wrapper for command senders
#[derive(Clone)]
pub struct CommandSender<T>(Sender<T>);

impl<T> CommandSender<T>
where
    T: Send + Clone + std::fmt::Debug + 'static,
{
    pub fn send(&self, command: T) -> std::result::Result<(), flume::SendError<T>> {
        self.0.send(command)
    }
}

/// Typed wrapper for event receivers
pub struct MessageReceiver<T>(Receiver<T>);

impl<T> MessageReceiver<T>
where
    T: Send + 'static,
{
    pub fn try_recv(&self) -> Option<T> {
        self.0.try_recv().ok()
    }

    pub fn receiver(&self) -> &Receiver<T> {
        &self.0
    }
}
