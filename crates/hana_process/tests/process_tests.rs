use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use error_stack::ResultExt;
use hana_process::{Error, Process, Result};

const TEST_LOG_FILTER: &str = "warn,hana=warn";

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
    println!("Error string: {}", error_string);
}

#[test]
fn test_ensure_shutdown() -> Result<()> {
    // Use Cargo's provided env var to determine the path of the built helper.
    // Make sure that "hana_helper" is specified as a binary target in your Cargo.toml.
    let helper_path = PathBuf::from(env!("CARGO_BIN_EXE_hana_helper"));

    let visualization = Process::run(helper_path, TEST_LOG_FILTER)?;

    // Use a short timeout so that ensure_shutdown will trigger killing.
    let timeout = Duration::from_millis(100);

    let result = visualization.ensure_shutdown(timeout);

    assert!(
        result.is_err(),
        "ensure_shutdown should error out because hana_helper never exits gracefully"
    );

    if let Err(err) = result {
        // Directly check that the current context is NotResponding

        assert!(
            matches!(err.current_context(), Error::NotResponding),
            "Expected error to be Error::NotResponding, got: Error::{:?}",
            err.current_context()
        );
    }

    Ok(())
}
