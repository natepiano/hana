use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Visualization error")]
    Visualization,
}

pub type Result<T> = error_stack::Result<T, Error>;
