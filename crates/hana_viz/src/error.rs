use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Network error")]
    Network,
    #[error("Process error")]
    Process,
    #[error("No active visualization")]
    NoActiveVisualization,
    #[error("Visualization entity not found")]
    EntityNotFound,
}

pub type Result<T> = error_stack::Result<T, Error>;
