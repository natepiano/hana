mod error;

use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use error_stack::{Report, ResultExt};

pub use crate::error::{Error, Result};

const TCP_ADDR: &str = "127.0.0.1:3001";
const CONNECTION_MAX_ATTEMPTS: u8 = 15;
const CONNECTION_RETRY_DELAY: Duration = Duration::from_millis(200);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(100);

pub struct Process {
    child: std::process::Child,
    path:  PathBuf,
}

impl Process {
    pub fn run(path: PathBuf) -> Result<Self> {
        Command::new(&path)
            .spawn()
            .map_err(Error::Io)
            .attach_printable(format!("Failed to launch visualization: {path:?}"))
            .map(|child| Process { child, path })
    }

    pub fn connect(&self) -> Result<TcpStream> {
        // Try to connect - with retries
        let mut attempts = 0;
        let stream = loop {
            match TcpStream::connect(TCP_ADDR) {
                Ok(stream) => break stream,
                Err(_) => {
                    attempts += 1;
                    if attempts >= CONNECTION_MAX_ATTEMPTS {
                        return Err(Report::new(Error::ConnectionTimeout).attach_printable(
                            format!("Failed to connect after {attempts} attempts"),
                        ));
                    }
                    println!("Connection attempt {} failed, retrying...", attempts);
                    std::thread::sleep(CONNECTION_RETRY_DELAY);
                }
            }
        };

        Ok(stream)
    }

    pub fn ensure_shutdown(mut self, timeout: Duration) -> Result<()> {
        use std::thread;

        let start = std::time::Instant::now();

        //child.try_wait() will return Some(..) if the child process has exited
        // otherwise we keep trying until we reach the timeout
        while start.elapsed() < timeout {
            match self.child.try_wait().map_err(Error::Io)? {
                Some(_status) => return Ok(()),
                None => thread::sleep(SHUTDOWN_TIMEOUT),
            }
        }

        // If we get here, we've timed out
        // kill throws io::Error so we wrap it in our own because that's what we do
        // however it's almost completely unlikely that this will fail
        // this will generally just kill and move on
        self.child
            .kill()
            .map_err(|e| Error::Io(e))
            .attach_printable("Failed to kill visualization process after timeout")?;

        // if we've made it this far, we throw the NotResponding error to indicate that we were
        // unable to kill it
        Err(Report::new(Error::NotResponding)
            .attach_printable("Visualization process not responding"))
    }
}

impl Drop for Process {
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
