use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Controller error")]
    Controller,
}

pub type Result<T> = error_stack::Result<T, Error>;
