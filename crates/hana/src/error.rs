use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Connection timeout")]
    ConnectionTimeout,
    #[error("IO error")]
    Io(#[from] std::io::Error),
    #[error("Network error")]
    Network,
}

pub type Result<T> = error_stack::Result<T, Error>;
