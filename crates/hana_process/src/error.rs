use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Io error")]
    Io(#[from] std::io::Error),
    #[error("Not responding")]
    NotResponding,
}

pub type Result<T> = error_stack::Result<T, Error>;
