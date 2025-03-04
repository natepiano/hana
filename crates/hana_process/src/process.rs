use std::path::PathBuf;
use std::time::Duration;

use error_stack::{Report, ResultExt};
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, warn};

use crate::prelude::*;

pub struct Process {
    child: tokio::process::Child,
    path:  PathBuf,
}

impl Process {
    pub async fn run(path: PathBuf, log_filter: impl Into<String>) -> Result<Self> {
        debug!("Attempting to spawn process at path: {:?}", path);
        debug!("Path exists: {}", path.exists());

        let log_filter = log_filter.into();
        let mut command = Command::new(&path);
        command.env("RUST_LOG", &log_filter);
        command.kill_on_drop(true); // takes care of cleanup for us

        command
            .spawn()
            .map_err(Error::Io)
            .attach_printable(format!("Failed to launch visualization: {path:?}"))
            .map(|child| Process { child, path })
    }

    pub async fn ensure_shutdown(mut self, shutdown_timeout: Duration) -> Result<()> {
        // More elegant timeout handling with tokio
        match timeout(shutdown_timeout, self.child.wait()).await {
            Ok(result) => {
                // Process exited within timeout
                result.map_err(Error::Io)?;
                Ok(())
            }
            Err(_) => {
                // Timeout occurred
                let report = Report::new(Error::NotResponding)
                    .attach_printable("visualization process not responding")
                    .attach_printable(format!("timeout: {} ms", shutdown_timeout.as_millis()))
                    .attach_printable(format!("path: {:?}", self.path));

                warn!("exceeded shutdown timeout: {}", report);

                // Kill the process
                if let Err(kill_err) = self.child.kill().await {
                    // Add kill error information to the existing report
                    return Err(report
                        .attach_printable(format!("additionally, kill() failed: {}", kill_err)));
                }

                // Return the timeout error with kill successful
                Err(report)
            }
        }
    }

    // clippy doesn't want names with beginning with is_* to take &mut
    // because is questions imply immutability
    // try_wait() requires &mut self so let's just override it
    #[allow(clippy::wrong_self_convention)]
    pub async fn is_running(&mut self) -> Result<bool> {
        match self.child.try_wait() {
            Ok(Some(_)) => Ok(false), // Process has exited
            Ok(None) => Ok(true),     // Process is still running
            Err(e) => {
                Err(Report::new(Error::Io(e))
                    .attach_printable("Failed to check if process is running"))
            }
        }
    }
}
