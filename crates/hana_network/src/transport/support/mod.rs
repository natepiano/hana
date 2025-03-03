//! Macros show up at the crate root so we just have to reference the mod and it will happen automatically.
pub mod connector;
mod impl_async_io;

pub use connector::*;
