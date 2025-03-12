use std::path::PathBuf;
use std::time::Duration;

use error_stack::ResultExt;
use hana_process::{Error, Process};

const TEST_LOG_FILTER: &str = "warn,hana=warn";

#[allow(clippy::expect_used)]
#[tokio::test]
async fn test_spawn_error() {
    let result = tokio::process::Command::new("non_existent_executable")
        .spawn()
        .change_context(Error::Io)
        .attach_printable("Failed to launch visualization");

    let err = result.expect_err("Expected spawn to fail");

    // Verify error type
    assert!(matches!(err.current_context(), Error::Io));
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

#[allow(clippy::unwrap_used)]
#[tokio::test]
async fn test_io_error_simulation() {
    use std::path::PathBuf;

    // Attempt to run a process that definitely doesn't exist
    let nonexistent_path = PathBuf::from("/path/that/definitely/does/not/exist");
    let result = Process::run(nonexistent_path, "info").await;

    assert!(result.is_err());
    let err = result.unwrap_err();

    // Print the full error report for inspection
    println!("Full error report:\n{:?}", err);

    // Print just the context part
    println!("Error context: {:?}", err.current_context());

    // Check that it's the expected error type
    assert!(matches!(err.current_context(), Error::Io));
}
