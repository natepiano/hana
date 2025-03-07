use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq)]
pub enum Error {
    #[error("Connection timeout")]
    ConnectionTimeout,
    #[error("IO operation failed")]
    Io,
    #[error("Serialization operation failed")]
    Serialization,
}

pub type Result<T> = error_stack::Result<T, Error>;
