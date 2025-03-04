// we only need this to make it easy to mock for testing
#[allow(async_fn_in_trait)]
pub trait ProcessControl {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>>;
    async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus>;
    async fn kill(&mut self) -> std::io::Result<()>;
}

// Implement the trait for tokio::process::Child
impl ProcessControl for tokio::process::Child {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.try_wait()
    }

    async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.wait().await
    }

    async fn kill(&mut self) -> std::io::Result<()> {
        self.kill().await
    }
}
