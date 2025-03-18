use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Command failed to send")]
    CommandFailed,
    #[error("Visualization entity not found")]
    EntityNotFound,
    #[error("Network error")]
    Network,
    #[error("No active visualization")]
    NoActiveVisualization,
    #[error("Process error")]
    Process,
}

pub type Result<T> = error_stack::Result<T, Error>;
