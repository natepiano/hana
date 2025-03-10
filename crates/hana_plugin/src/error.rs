use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Can't start network runtime")]
    CantStartNetworkRuntime,
    #[error("Network error")]
    Network,
    #[error("I/O error")]
    Io,
    #[error("Channel error")]
    Channel,
}

pub type Result<T> = error_stack::Result<T, Error>;
