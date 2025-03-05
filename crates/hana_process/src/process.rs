use std::{path::PathBuf, time::Duration};

use error_stack::{Report, ResultExt};
use tokio::{process::Command, time::timeout};
use tracing::debug;

use crate::{prelude::*, process_control::ProcessControl};

pub struct Process<P: ProcessControl> {
    pub child: P,
    path:      PathBuf,
}

// Provide a concrete implementation for the common case
impl Process<tokio::process::Child> {
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
            .attach_printable(format!("Command failed: {command:?}"))
            .map(|child| Process { child, path })
    }
}

impl<P: ProcessControl> Process<P> {
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
                    .attach_printable(format!("timeout: {} ms", shutdown_timeout.as_millis()))
                    .attach_printable(format!("path: {:?}", self.path));

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
            Err(e) => Err(
                Report::new(Error::Io(e)).change_context(Error::ProcessCheckFailed {
                    path: self.path.clone(),
                }),
            ),
        }
    }
}

#[cfg(test)]
#[tokio::test]
async fn test_is_running_error() {
    use std::{io, path::PathBuf};

    use crate::{process::Process, support::MockProcessControl};

    // Create a mock with a specific error using direct initialization
    let mock_error = io::Error::new(io::ErrorKind::Other, "Test error");
    let mock = MockProcessControl {
        error: Some(mock_error),
    };

    let test_path = PathBuf::from("/test/path");

    let mut process = Process {
        child: mock,
        path:  test_path.clone(),
    };

    let result = process.is_running().await;
    assert!(result.is_err());

    // Check the error type
    if let Err(report) = result {
        assert!(
            matches!(report.current_context(), Error::ProcessCheckFailed { path } if *path == test_path),
            "Expected ProcessCheckFailed with path {:?}, got {:?}",
            test_path,
            report.current_context()
        );
    }
}
