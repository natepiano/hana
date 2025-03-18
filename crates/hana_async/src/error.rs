use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Runtime channel closed")]
    ChannelClosed,

    #[error("Tokio runtime creation failed")]
    RuntimeCreationFailed,
}
