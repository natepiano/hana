use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Io error")]
    Io,
    #[error("Process not responding")]
    NotResponding,
    #[error("Process check failed")]
    ProcessCheckFailed { path: PathBuf },
}

pub type Result<T> = error_stack::Result<T, Error>;
