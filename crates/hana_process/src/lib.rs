mod error;
mod process;
mod process_control;
mod support;

mod prelude {
    pub use crate::error::{Error, Result};
}

pub use crate::prelude::*;
pub use crate::process::RunningState;

// Process has a generic type parameter so we can mock it in tests
// however for the public interface, we can just export it as Process
// type aliases are fun
pub type Process = process::Process<tokio::process::Child>;
