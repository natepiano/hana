use crate::{Error, Result};
use error_stack::{Report, ResultExt};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const MAX_ATTEMPTS: u8 = 15;
const RETRY_DELAY: Duration = Duration::from_millis(500);

pub struct VisualizationProcess {
    child: std::process::Child,
    path: PathBuf,
}

impl VisualizationProcess {
    pub fn new(path: PathBuf) -> Result<Self> {
        Command::new(&path)
            .spawn()
            .map_err(Error::Io)
            .attach_printable(format!("Failed to launch visualization: {path:?}"))
            .map(|child| VisualizationProcess { child, path })
    }

    pub fn connect(&self) -> Result<TcpStream> {
        // Try to connect with retries
        let mut attempts = 0;
        let stream = loop {
            match TcpStream::connect("127.0.0.1:3001") {
                Ok(stream) => break stream,
                Err(_) => {
                    attempts += 1;
                    if attempts >= MAX_ATTEMPTS {
                        return Err(Report::new(Error::ConnectionTimeout).attach_printable(
                            format!("Failed to connect after {attempts} attempts"),
                        ));
                    }
                    println!("Connection attempt {} failed, retrying...", attempts);
                    std::thread::sleep(RETRY_DELAY);
                }
            }
        };

        Ok(stream)
    }

    pub fn wait(mut self, timeout: Duration) -> Result<()> {
        use std::thread;

        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            match self.child.try_wait().map_err(Error::Io)? {
                Some(_status) => return Ok(()),
                None => thread::sleep(Duration::from_millis(100)),
            }
        }

        // If we get here, we've timed out
        self.child
            .kill()
            .map_err(Error::Io)
            .attach_printable("Failed to kill visualization process after timeout")?;

        Err(Report::new(Error::ConnectionTimeout)
            .attach_printable("Visualization process failed to shutdown in time"))
    }
}

impl Drop for VisualizationProcess {
    fn drop(&mut self) {
        if let Err(e) = self.child.kill() {
            // Convert to error-stack Report and log with context
            let error = error_stack::Report::new(Error::Io(e))
                .attach_printable("Failed to send kill signal to visualization process")
                .attach_printable(format!("path: {:?}", self.path));
            // Log the full error chain
            eprintln!("{:?}", error);
        }
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn test_spawn_error() {
        let result = Command::new("non_existent_executable")
            .spawn()
            .map_err(Error::Io)
            .attach_printable("Failed to launch visualization");

        let err = result.expect_err("Expected spawn to fail");

        // Verify error type
        assert!(matches!(err.current_context(), Error::Io(_)));

        // Verify error message chain
        let error_string = format!("{err:?}");
        assert!(
            error_string.contains("Failed to launch visualization"),
            "Missing expected error message"
        );
        assert!(
            error_string.contains("No such file or directory"),
            "Missing underlying OS error"
        );
        println!("Error string: {}", error_string);
    }
}
