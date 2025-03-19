use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Visualization entity not found")]
    EntityNotFound,
    #[error("Async worker error while attempting to send instruction to visualization")]
    AsyncWorker,
    #[error("Network error while sending instructions to visualization")]
    Network,
    #[error("No active visualization")]
    NoActiveVisualization,
    #[error("Process error")]
    Process,
}

pub type Result<T> = error_stack::Result<T, Error>;
