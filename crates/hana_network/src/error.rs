use bincode::Error as BincodeError;
use std::io::Error as IoError;
// don't be confused - this is just for the derive macro
use thiserror::Error;

// when using Result from this crate, the Error type is hana-network::Error
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    // - hana-network errors

    // - External errors
    #[error("IO error: {0}")]
    Io(#[from] IoError),
    #[error("Serialization error: {0}")]
    Serialization(#[from] BincodeError),
}
