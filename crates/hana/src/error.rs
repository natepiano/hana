use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Controller error")]
    Controller,
    #[cfg(debug_assertions)]
    #[error("hana_window error")]
    Window,
}

pub type Result<T> = error_stack::Result<T, Error>;
