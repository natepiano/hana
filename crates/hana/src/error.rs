use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Visualization error")]
    Visualization,
    #[error("Tokio runtime channel closed, application cannot function")]
    TokioRuntimeChannelClosed,
}

#[derive(Debug)]
pub enum Severity {
    Critical, // Application must terminate
    Error,    // Operation failed, but app can continue
    #[allow(dead_code)]
    Warning, // Something went wrong but was handled automatically
}

pub type Result<T> = error_stack::Result<T, Error>;
