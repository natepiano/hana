# Tracing

## Overview
Hana uses the tracing crate to manage logging and tracing within the application. This section provides an overview of the tracing setup and configuration.

## Tracing Configuration

The tracing setup in this application is configured through the `setup_logging` function in `hana/crates/hana/src/utils/trace.rs`. Here's how it works:

### Tracing Configuration

The tracing system uses `tracing-subscriber` to handle the collection and formatting of log events. Here's how it's set up:

1. **Environment-Based Configuration**: The function first attempts to read logging configuration from the `RUST_LOG` environment variable using `EnvFilter::try_from_default_env()`.

2. **Default Fallback**: If no environment variable is set, it falls back to a default configuration with warning-level logs for most components and specific warning-level logs for the `hana` module.

3. **Subscriber Setup**: The function creates a tracing registry with a formatting layer that includes:
   - Thread IDs
   - Source file information
   - Line numbers
   - Target module information

4. **Return Value**: The function returns the filter string so that it can be passed to child processes (like visualizations) to ensure consistent logging behavior across the application ecosystem.

### Usage in Application

In the hana `main.rs` file, this setup is used at application startup:

```rust
let env_filter_str = utils::setup_logging();
```

The returned filter string is then passed to the visualization process:

```rust
let viz = Visualization::<Unstarted>::start(viz_path, env_filter_str)
    .await
    .change_context(Error::Visualization)?;
```

This ensures that both the main Hana application and any child visualization processes use consistent logging configurations, creating a unified logging experience across the system.

### Customization Options

The comments in the code explain how users can customize the logging behavior through environment variables:

- `RUST_LOG=debug` - Allow all logs at debug level and above
- `RUST_LOG=hana_plugin=debug,bevy=off` - Show only plugin logs, suppress Bevy logs
- `RUST_LOG=off` - Suppress all logs
