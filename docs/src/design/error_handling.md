# Error Handling

The Hana system implements structured error handling using the `error-stack` crate to provide context and create a clear chain of errors across module and crate boundaries.

Each crate defines its error types in a dedicated `error.rs` file. These error enums serve to contextualize lower-level errors into aggregate error types within the crate. For example, when a crate uses `hana_network`, it may define a single error variant like `Network` that represents all possible underlying `hana_network` errors rather than exposing those implementation details.

Here's a concrete example from the codebase showing how standard library errors are transformed:

```rust
// In hana_network/src/transport/unix.rs
if path.exists() {
    debug!("Found existing socket file at {:?}, removing it", path);
    std::fs::remove_file(path)
        .change_context(Error::Io)
        .attach_printable_lazy(|| {
            format!("Failed to remove existing socket file at {:?}", path)
        })?;
}
```

In this example, a standard library `io::Result<()>` from `std::fs::remove_file` is mapped to the `hana_network` error type using `change_context(Error::Io)`. This transforms the low-level IO error into a domain-specific error that can be further contextualized up the error stack. The additional context from `attach_printable_lazy` provides specific details about what operation failed.

Each crate also includes a `prelude.rs` module that exports a type alias for `Result`:

```rust
// In error.rs
pub type Result<T> = error_stack::Result<T, Error>;

// In prelude.rs
pub use crate::error::{Error, Result};
```

This type alias lets code use a simple `Result<T>` rather than `error_stack::Result<T, Error>` throughout the crate.

The system uses `change_context()` specifically when crossing crate boundaries and `attach_printable()` to add context without changing the error type within the same crate.

## example error output
Below is the output from an example error showing how a broken pipe propagates from hana_network up to hana_visualization up to the hana app itself

With the contextualization that happens into each layer's error type plus extra information added with attach_printable or attach_printable_lazy. The information provided is robust and helps pinpoint actual issues.

```
Error: Visualization error
├╴at crates/hana/src/main.rs:30:26
│
├─▶ Network error
│   ├╴at /Users/natemccoy/RustroverProjects/hana/crates/hana_visualization/src/lib.rs:85:14
│   ╰╴Failed to send instruction: Ping
│
├─▶ IO operation failed
│   ├╴at /Users/natemccoy/RustroverProjects/hana/crates/hana_network/src/endpoint/base_endpoint.rs:44:14
│   ├╴Failed to write length prefix: '4' to message: 'Ping'
│   ╰╴transport: UnixTransport { peer_addr: None }
│
╰─▶ Broken pipe (os error 32)
    ╰╴at /Users/natemccoy/RustroverProjects/hana/crates/hana_network/src/endpoint/base_endpoint.rs:44:14
```

With this we can see that an underlying IO error gets turned into  `hana_network::error::Error::Io` which in turn gets turned into a `hana_visualization::error::Error::Network` which in turn gets turned into hana::error::Error::Visualization.

Because Hana is interacting with the hana_visualization lib, it sees the error a Visualization error. Because the hana_visualization is calling hana_network, it sees the error as a Network error and because hana_network is calling underlying Io methods, it sees the error as an IO error.

Voilá!

## a note about the underlying error from libraries we don't control
In experimenting with Io errors, error-stack seems very good at providing a report for the original underlying error so that when we change context to our own error, we see a good message for the underlying.

As a result, we don't try to capture that source information in a field of the enum variant as it is already captured in the error-stack.

## Enum with field(s)
This is the error from hana_process/src/error.rs:
```rust
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Io error")]
    Io,
    #[error("Process not responding")]
    NotResponding,
    #[error("Process check failed")]
    ProcessCheckFailed { path: PathBuf },
}

pub type Result<T> = error_stack::Result<T, Error>;
```

Notice that ProcessCheckFailed has a field - that's not a problem as you can just create a new error report and propagate it up - here's one example:

```rust
pub async fn is_running(&mut self) -> Result<bool> {
    match self.child.try_wait() {
        Ok(Some(_)) => Ok(false), // Process has exited
        Ok(None) => Ok(true),     // Process is still running
        Err(e) => Err(Report::new(e).change_context(Error::ProcessCheckFailed {
            path: self.path.clone(),
        })),
    }
}
```
In this case we're propagating the error ourselves so we create a new error-stack Report with the underlying error and then change_context to a fully qualified ProcessCheckFailed that includes that path information for the failed process.

## ? in tests
To easily use ? in tests, we can make the result for the test ```Result<(), Box<dyn std::error::Error>>``` which allows it to just pass through any error. We don't need special error handling machinery for tests as we just want to catch things if they fail and then make them pass.
