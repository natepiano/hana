use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Visualization error")]
    #[allow(dead_code)]
    Visualization,
    #[error("Tokio runtime channel closed, application cannot function")]
    TokioRuntimeChannelClosed,
}

#[allow(dead_code)]
pub type Result<T> = error_stack::Result<T, Error>;
