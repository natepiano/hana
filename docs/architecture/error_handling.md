# Error Handling in the Hana System

## Overview
The Hana system employs a modular and layered error-handling strategy using `eyre` and `thiserror` to ensure consistency, maintainability, and clarity. This approach supports seamless propagation and management of errors across different components, fostering a robust and fault-tolerant architecture with strong tracing integration.

## Key Principles
1. **Module-Specific Error Types**  
   Each library module defines its own error type using **`thiserror`**, encapsulating errors specific to its operations.

2. **Unified Error Propagation**  
   Library-specific errors are propagated as-is, allowing module consumers to handle them directly or convert them into application-level errors.

3. **Centralized Error Handling in Management Application**  
   The Management Application integrates errors from different libraries using **`eyre`**, ensuring flexibility and ease of error reporting with tracing context.

4. **Type Alias for `Result`**  
   Each library module includes a type alias for `Result` to pair its error type with the return value of functions.

5. **Avoiding `unwrap` and `expect`**  
   The use of `unwrap` and `expect` is prohibited in production code to avoid panics. Instead, errors are explicitly handled or propagated.

## Dependencies and Setup

### Dependencies
```toml
[dependencies]
eyre = "0.6"
thiserror = "0.2"
tracing = "0.1"
tracing-error = "0.2"
tracing-subscriber = "0.3"
```

### Error Handler Setup
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

## Libraries: Using `thiserror`

### Module-Specific Error Definitions
Each library defines an error enum using the **`thiserror`** crate to encapsulate specific error cases:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("Failed to connect: {0}")]
    ConnectionFailed(String),

    #[error("Timeout occurred")]
    Timeout,

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Underlying I/O error: {0}")]
    IoError(#[from] std::io::Error),
}
```

This approach ensures:
- **Clarity:** Errors are explicitly tied to their source library
- **Extensibility:** Adding new error cases doesn't affect other modules

### Unified `Result` Type Alias
Each library defines a type alias for `Result` to simplify function signatures:

```rust
pub type Result<T> = std::result::Result<T, NetworkError>;
```

### Propagating Errors Between Libraries
When one library depends on another, its error type can wrap the other's error:

```rust
impl From<hana_input::InputError> for NetworkError {
    fn from(err: hana_input::InputError) -> Self {
        Self::InvalidConfig(format!("Input error: {}", err))
    }
}
```

## Management Application: Using `eyre`

In the Management Application, **`eyre`** simplifies error handling while maintaining rich context through tracing integration.

### Handling Errors with `eyre`

#### Example with Tracing Integration
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
2. Add context with `wrap_err()` when propagating errors
3. Include relevant data in span fields
4. Keep error types focused and well-documented

### Error Handling Patterns
```rust
#[instrument]
fn process_data(input: &str) -> Result<()> {
    let span_trace = SpanTrace::capture();
    
    // Add context to library errors
    validate_input(input)
        .wrap_err("Invalid input format")?;
        
    // Record additional context
    info!("Processing validated input");
    
    process_validated_input(input)
        .wrap_err("Processing failed")?;
        
    Ok(())
}
```

## Advanced Usage

### Custom Error Reporting
```rust
fn setup_custom_error_handling() {
    eyre::set_hook(Box::new(|error| {
        Box::new(CustomErrorReport {
            span_trace: SpanTrace::capture(),
            error: error.to_string(),
        })
    }))
    .expect("Failed to set custom error hook");
}
```

### Integration with Logging System
Errors automatically integrate with the logging framework:
- Span context preserved in logs
- Error chain available in log output
- Unified debugging experience

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
- [Plugin Development](../plugins/README.md) - Guidelines for plugin development
- [User](../developer/README.md) - Hana user documentation
