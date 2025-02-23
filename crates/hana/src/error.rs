use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error")]
    Io(#[from] std::io::Error),
    #[error("Network error")]
    Network,
    #[error("Process error")]
    Process,
}

pub type Result<T> = error_stack::Result<T, Error>;
