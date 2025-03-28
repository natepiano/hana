use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Visualization error")]
    Visualization,
}

#[derive(Debug)]
pub enum Severity {
    #[allow(dead_code)]
    Critical, // Application must terminate
    Error, // Operation failed, but app can continue
    #[allow(dead_code)]
    Warning, // Something went wrong but was handled automatically
}

//pub type Result<T> = error_stack::Result<T, Error>;
