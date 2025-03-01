#[cfg(debug_assertions)]
pub mod debug; // used to return focus to the editor after process completes
mod error;
mod prelude;

pub use crate::prelude::*;
use error_stack::{Report, ResultExt};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tracing::{debug, error};

const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(100);

pub struct Process {
    child: Option<std::process::Child>, // Changed to Option
    path: PathBuf,
}

impl Process {
    pub fn run(path: PathBuf, log_filter: impl Into<String>) -> Result<Self> {
        let log_filter = log_filter.into();
        let mut command = Command::new(&path);
        command.env("RUST_LOG", &log_filter);

        command
            .spawn()
            .map_err(Error::Io)
            .attach_printable(format!("Failed to launch visualization: {path:?}"))
            .map(|child| Process {
                child: Some(child),
                path,
            })
    }

    pub fn ensure_shutdown(mut self, timeout: Duration) -> Result<()> {
        use std::thread;

        let start = std::time::Instant::now();

        if let Some(mut child) = self.child.take() {
            // Try graceful shutdown first
            while start.elapsed() < timeout {
                match child.try_wait().map_err(Error::Io)? {
                    Some(_status) => return Ok(()),
                    None => thread::sleep(SHUTDOWN_TIMEOUT),
                }
            }

            debug!(
                "exceeded graceful shutdown timeout: {}",
                start.elapsed().as_millis()
            );

            // If we get here, we've timed out - attempt to kill
            child
                .kill()
                .map_err(Error::Io)
                .attach_printable("failed to kill visualization process after timeout")?;

            // Process didn't respond to graceful shutdown
            Err(Report::new(Error::NotResponding)
                .attach_printable("visualization process not responding"))
        } else {
            // Process was already shutdown
            Ok(())
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            if let Err(e) = child.kill() {
                let error = Report::new(Error::Io(e))
                    .attach_printable("Failed to send kill signal to visualization process")
                    .attach_printable(format!("path: {:?}", self.path));
                error!("{:?}", error);
            }
        }
    }
}
