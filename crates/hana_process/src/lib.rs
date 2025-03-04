mod error;
mod prelude;
mod process;
mod process_control;
mod support;

pub use crate::prelude::*;
// Process has a generic type parameter so we can mock it in tests
// however for the public interface, we can just export it as Process
// type aliases are fun
pub type Process = process::Process<tokio::process::Child>;
