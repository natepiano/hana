use std::{path::PathBuf, time::Duration};

use error_stack::ResultExt;
use hana_process::{Error, Process};

const TEST_LOG_FILTER: &str = "warn,hana=warn";

#[tokio::test]
async fn test_spawn_error() {
    let result = tokio::process::Command::new("non_existent_executable")
        .spawn()
        .map_err(|e| Error::Io { source: e })
        .attach_printable("Failed to launch visualization");

    let err = result.expect_err("Expected spawn to fail");

    // Verify error type
    assert!(matches!(err.current_context(), Error::Io { source: _ }));
}

#[tokio::test]
async fn test_ensure_shutdown() -> Result<(), Box<dyn std::error::Error>> {
    // Use Cargo's provided env var to determine the path of the built helper.
    let helper_path = PathBuf::from(env!("CARGO_BIN_EXE_hana_helper"));

    let visualization = Process::run(helper_path, TEST_LOG_FILTER).await?;

    // Use a short timeout so that ensure_shutdown will trigger killing.
    let timeout = Duration::from_millis(1);

    let result = visualization.ensure_shutdown(timeout).await;

    assert!(
        result.is_err(),
        "should error because hana_helper is a loop that never exits gracefully"
    );

    if let Err(err) = result {
        assert!(
            matches!(err.current_context(), Error::NotResponding),
            "Expected error to be Error::NotResponding, got: Error::{:?}",
            err.current_context()
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_is_running() -> Result<(), Box<dyn std::error::Error>> {
    // Use Cargo's provided env var to determine the path of the built helper.
    let helper_path = PathBuf::from(env!("CARGO_BIN_EXE_hana_helper"));

    // Spawn the process
    let mut process = Process::run(helper_path, TEST_LOG_FILTER).await?;

    // Check that it's running
    let running = process.is_running().await?;
    assert!(running, "Process should be running initially");

    // Kill the process and handle io::Error conversion
    process.child.kill().await?;

    // Wait for the process to exit and handle io::Error conversion
    process.child.wait().await?;

    // Now we can be sure the process has exited
    let running = process.is_running().await?;
    assert!(
        !running,
        "Process should not be running after kill and wait"
    );

    Ok(())
}
