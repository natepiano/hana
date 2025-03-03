#[cfg(not(target_os = "macos"))]
use crate::error::Result;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use self::macos::activate_parent_window;

#[cfg(not(target_os = "macos"))]
pub fn activate_parent_window() -> Result<()> {
    tracing::info!("editor Window activation not implemented on this platform");
    Ok(())
}
