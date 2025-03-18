use std::future::Future;

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

    pub fn receiver(&self) -> &Receiver<T> {
        &self.0
    }
}

pub struct Worker<Cmd, Msg> {
    command_sender:   CommandSender<Cmd>,
    message_receiver: MessageReceiver<Msg>,
}

impl<Cmd, Msg> Worker<Cmd, Msg>
where
    Cmd: Send + Clone + std::fmt::Debug + 'static,
    Msg: Send + 'static,
{
    /// Create a new worker with the given process function
    pub fn new<F, Fut>(async_runtime: &crate::AsyncRuntime, process_fn: F) -> Self
    where
        F: Fn(Cmd) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future<Output = Vec<Msg>> + Send + 'static,
    {
        let (command_sender, message_receiver) = async_runtime.create_worker(process_fn);
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

    /// Get the command sender
    pub fn command_sender(&self) -> &CommandSender<Cmd> {
        &self.command_sender
    }

    /// Get the message receiver
    pub fn message_receiver(&self) -> &MessageReceiver<Msg> {
        &self.message_receiver
    }
}
