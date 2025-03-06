use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq)]
pub enum Error {
    #[error("Connection timeout")]
    ConnectionTimeout,
    #[error("Message decoding operation failed")]
    Decoding,
    #[error("Message encoding operation failed")]
    Encoding,
    #[error("IO operation failed")]
    Io,
}

pub type Result<T> = error_stack::Result<T, Error>;
