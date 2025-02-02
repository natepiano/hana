# Error Handling in the Hana System

## Overview
The Hana system employs a modular and layered error-handling strategy using `eyre` and `thiserror` to ensure consistency, maintainability, and clarity. This approach supports seamless propagation and management of errors across different components, fostering a robust and fault-tolerant architecture with strong tracing integration.

## Key Principles
1. **Module-Specific Error Types**
  Each library module defines its own error type using **`thiserror`**, encapsulating errors specific to its operations.

1. **Module-Specific Naming Convention**
  Module-specific error types are named with the pattern my_module::Error which mirrors what is becoming common practice in the Rust ecosystem.

1. **Type Alias for `Result`**
  Each library module includes a type alias for `Result` to pair the module's error type with the return value of functions, then using the Result from this module ensures we're using this module's error type.

1. **Enums for Error Variants**
  Error types are defined as enums with variants representing different error cases, ensuring clarity and extensibility. Wrap errors from dependencies and use thiserror #from attribute, which implements From trait, to allow easy conversion to the module's error type. You can add Serde::Serialize to support logging.

1. **Use struct variant's to provide extra information**
Here we have ConnectionFailed - don't do this:
```rust
#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to connect: {0}")]
    ConnectionFailed(String, u32),
}
```
instead do this:
```rust
#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to connect: {0}")]
    ConnectionFailed{message: String, code: u32},
}
```
And it will allow you to see that the String is a message and the u32 is a code. And this will propagate wherever you're looking at it with Debug or Display.

1. **Centralized Error Handling in Management Application**
  The [Management Application](../../crates/hana/README.md) integrates errors from different libraries using **`eyre`**, ensuring flexibility and ease of error reporting with tracing context.

1. **Avoiding `unwrap` and `expect`**
   The use of `unwrap` and `expect` is prohibited in production code to avoid panics. Instead, errors are explicitly handled or propagated. Make it easy to use ? to propagate errors.

## Libraries: Using `thiserror`
You can create an error.rs to drop module specific error enum and type alias. Your library can re-export the type alias and error enum. This way you can have a single place to manage all the errors for a library - and make it easy to use.

### Module-Specific Error Definitions
Each library defines an error enum using the **`thiserror`** crate to encapsulate specific error cases - here is an example from the `hana_network` library:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] IoError),
    #[error("Serialization error: {0}")]
    Serialization(#[from] BincodeError),
}
```

This approach ensures:
- **Clarity:** Errors are explicitly tied to their source library
- **Extensibility:** Adding new error cases doesn't affect other modules

### Unified `Result` Type Alias
Each library defines a type alias for `Result` to simplify function signatures:

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

### Propagating Errors Between Libraries
When one library depends on another, its error type can wrap the other's error:

```rust
impl From<hana_input::Error> for hana_network::Error {
    fn from(err: hana_input::Error) -> Self {
        Self::InvalidConfig(format!("Input error: {}", err))
    }
}
```

## Management Application: Using `eyre`

In the Management Application, **`eyre`** simplifies error handling while maintaining rich context through tracing integration.

### Handling Errors with `eyre`
**Setup**

```rust
use eyre::{Result, WrapErr};
use tracing_error::SpanTrace;

fn setup_error_handling() {
    eyre::set_hook(Box::new(|_| {
        Box::new(SpanTrace::capture())
    }))
    .expect("Failed to set eyre hook");
}
```
### Example with Tracing Integration
```rust
use eyre::{Result, WrapErr};
use tracing::{info, instrument};

#[instrument]
fn perform_network_operation() -> Result<()> {
    let trace = SpanTrace::capture();
    info!("Starting network operation");

    hana_network::connect("https://example.com")
        .wrap_err("Failed to perform network operation")?;

    Ok(())
}

fn main() -> Result<()> {
    setup_error_handling();

    if let Err(e) = perform_network_operation() {
        eprintln!("Error with context: {:?}", e);
        // Error will include span trace
    }
    Ok(())
}
```

### Error Output Example
```
Error: Failed to perform network operation
Caused by:
    0: Connection refused (os error 61)
Span trace:
    at perform_network_operation
        at src/main.rs:23
    at main
        at src/main.rs:15
```

## Key Features of Integration

### Rich Context Through Spans
- Every error includes its trace context
- Spans show exact error origin
- Function entry/exit tracking
- Additional context via span fields

### Error Propagation with Context
- Automatic span capture
- Error source chain preservation
- Custom error reports possible
- Integration with logging system

## Best Practices

### Development Guidelines
1. Use `#[instrument]` on functions for automatic span creation
2. Implement `From` traits for error conversion between modules
3. Include relevant data in span fields
4. Keep error types focused and well-documented

### Error Handling Patterns
```rust
// In library code:
#[instrument]
fn process_data(input: &str) -> Result<ProcessedData> {
    let validated = validate_input(input)?; // Uses From trait for error conversion
    let processed = process_validated_input(validated)?; // Uses From trait
    Ok(processed)
}

// In management application:
#[instrument]
fn handle_data_processing(input: &str) -> eyre::Result<()> {
    // Here we can use wrap_err() since we're in the application layer
    process_data(input)
        .wrap_err("Failed to process data")?;
    Ok(())
}
```

## Advanced Usage

### Context Enrichment
```rust
// In the management application, we can add rich context to errors:
#[instrument(err(Debug))]
fn complex_operation() -> eyre::Result<()> {
    // Add structured fields to spans for better debugging
    tracing::info!(user_id = "123", operation = "sync", "Starting complex operation");

    let result = do_something()?;

    // Record outcomes
    tracing::info!(status = "complete", items_processed = 42);
    Ok(())
}
```

### Integration with Logging System
Since we use `tracing`, errors are automatically integrated with our logging infrastructure:

```rust
use tracing::{info, error, instrument};

#[instrument]
fn database_operation() -> Result<()> {
    info!("Starting database operation");

    match perform_query() {
        Ok(result) => {
            info!(rows_affected = result.rows, "Query successful");
            Ok(())
        }
        Err(e) => {
            // Error will automatically include span context
            error!(error = ?e, "Database query failed");
            Err(e)
        }
    }
}
```

## Summary
- Use `thiserror` in libraries for typed errors
- Use `eyre` in the Management Application
- Always include tracing context
- Maintain proper error propagation
- Keep error types focused and documented

## Doc Links
- [Architecture](README.md) - High level system design
- [Developer](../developer/README.md) - Coding guidelines for hana contributors
- [Overview](../../README.md) - Hana overview
- [Plugin Development](../visualization/README.md) - Guidelines for plugin development
- [User](../user/README.md) - Hana user documentation
