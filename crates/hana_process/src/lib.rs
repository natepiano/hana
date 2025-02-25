mod error;

use error_stack::{Report, ResultExt};

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tracing::info;

pub use crate::error::{Error, Result};

const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(100);

pub struct Process {
    child: std::process::Child,
    path: PathBuf,
}

impl Process {
    pub fn run(path: PathBuf) -> Result<Self> {
        Command::new(&path)
            //  .env_remove("RUST_LOG")
            .spawn()
            .map_err(Error::Io)
            .attach_printable(format!("Failed to launch visualization: {path:?}"))
            .map(|child| Process { child, path })
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

        info!("elapsed wait to shutdown: {}", start.elapsed().as_millis());

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
