#[cfg(debug_assertions)]
mod error;

#[cfg(debug_assertions)]
mod macos;

#[cfg(debug_assertions)]
pub use crate::error::{Error, Result};
#[cfg(debug_assertions)]
use error_stack::ResultExt;

#[cfg(debug_assertions)]
pub fn activate_parent_window() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::activate_parent_window().attach_printable("Failed to activate parent window")
    }

    #[cfg(not(target_os = "macos"))]
    {
        info!("Window activation not implemented on this platform");
        Ok(())
    }
}
