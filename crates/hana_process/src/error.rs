use thiserror::Error;

#[derive(Debug, Error)]
// pub enum Error {
//     #[error("Io error")]
//     Io(#[from] std::io::Error),
//     #[error("Not responding")]
//     NotResponding,
// }
pub enum Error {
    #[error("IO error")]
    Io(#[from] std::io::Error),
    #[error("Process not responding")]
    NotResponding,
    #[cfg(debug_assertions)]
    #[error("Failed to capture parent process")]
    ParentCapture,
    #[cfg(debug_assertions)]
    #[error("Failed to activate window")]
    WindowActivation,
}
pub type Result<T> = error_stack::Result<T, Error>;
