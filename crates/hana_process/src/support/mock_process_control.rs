use std::io;

use crate::process_control::ProcessControl;

// A simple mock implementation of ProcessControl for testing
pub struct MockProcessControl {
    pub error: Option<io::Error>,
}

impl ProcessControl for MockProcessControl {
    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        Err(self
            .error
            .take()
            .unwrap_or_else(|| io::Error::new(io::ErrorKind::Other, "Mock error")))
    }

    async fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        unimplemented!("Not needed for this test")
    }

    async fn kill(&mut self) -> io::Result<()> {
        unimplemented!("Not needed for this test")
    }
}
