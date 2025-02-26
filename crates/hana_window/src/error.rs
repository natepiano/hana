use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to capture parent process")]
    ParentCapture,
    #[error("Failed to activate window")]
    WindowActivation,
    #[error("IO error")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = error_stack::Result<T, Error>;
