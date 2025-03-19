//! - `Worker<Cmd, Msg>`: Generic wrapper with type-safe send/receive capabilities
//! - `CommandSender<T>` and `MessageReceiver<T>`: Typed wrappers around flume channels

use std::{future::Future, sync::Arc};

use error_stack::Report;
use flume::{Receiver, Sender};

use crate::error::Error;

/// Typed wrapper for command senders
#[derive(Clone)]
pub struct CommandSender<T>(pub(crate) Sender<T>);

impl<T> CommandSender<T>
where
    T: Send + Clone + std::fmt::Debug + 'static,
{
    pub fn send(&self, command: T) -> error_stack::Result<(), Error> {
        self.0
            .send(command)
            .map_err(|_| Report::new(Error::ChannelClosed))
    }
}

/// Typed wrapper for event receivers
pub struct MessageReceiver<T>(pub(crate) Receiver<T>);

impl<T> MessageReceiver<T>
where
    T: Send + 'static,
{
    pub fn try_recv(&self) -> Option<T> {
        self.0.try_recv().ok()
    }
}

/// our generic Worker can send and receive whatever types of messages we want
/// subject to the trait bounds which aren't very limiting
pub struct Worker<Cmd, Msg> {
    command_sender: CommandSender<Cmd>,
    message_receiver: MessageReceiver<Msg>,
}

impl<Cmd, Msg> Worker<Cmd, Msg>
where
    Cmd: Send + Clone + std::fmt::Debug + 'static,
    Msg: Send + 'static,
{
    /// Creates a new Worker that processes commands asynchronously and returns messages.
    ///
    /// This is the primary way to create asynchronous workers in the application.
    /// Each worker has its own dedicated thread and processing queue.
    pub fn new<F, Fut>(async_runtime: &crate::runtime::AsyncRuntime, process_fn: F) -> Self
    where
        F: Fn(Cmd) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future<Output = Msg> + Send + 'static,
    {
        let (command_sender, message_receiver) = create_worker(async_runtime, process_fn);
        Self {
            command_sender,
            message_receiver,
        }
    }

    /// Send a command to the worker
    pub fn send_command(&self, command: Cmd) -> error_stack::Result<(), Error> {
        self.command_sender.send(command)
    }

    /// Try to receive a message from the worker (non-blocking)
    pub fn try_receive(&self) -> Option<Msg> {
        self.message_receiver.try_recv()
    }
}

/// Helper method to create our bridge between the async worker and the main bevy ECS system
/// thread(s)
fn create_worker<Cmd, Msg, F, Fut>(
    async_runtime: &crate::runtime::AsyncRuntime,
    process_fn: F,
) -> (CommandSender<Cmd>, MessageReceiver<Msg>)
where
    Cmd: Send + Clone + std::fmt::Debug + 'static,
    Msg: Send + 'static,
    F: Fn(Cmd) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Msg> + Send + 'static,
{
    let (cmd_tx, cmd_rx) = flume::unbounded();
    let (msg_tx, msg_rx) = flume::unbounded();

    let runtime = Arc::clone(&async_runtime.runtime);

    // Spawn the worker thread directly here instead of calling a separate function
    std::thread::spawn(move || {
        runtime.block_on(async {
            while let Ok(command) = cmd_rx.recv_async().await {
                let message_sender = msg_tx.clone();
                let process = process_fn.clone();

                // process the command and send all msg results back
                // in hana_viz this is processing AsyncInstruction commands and sending back
                // AsyncOutcome messages (which are a Vec of AsyncOutcome)
                runtime.spawn(async move {
                    let result = process(command).await;
                    let _ = message_sender.send(result);
                });
            }
        });
    });

    // give the other side of the communication channels
    // back to the bevy side so they can send commands to the worker
    // and receive messages from the worker
    (CommandSender(cmd_tx), MessageReceiver(msg_rx))
}
