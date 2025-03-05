//! utility fn's for configuring tracing behavior
//! the only fn currently here is to configure tracing_subscriber
//! it's fine
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// This will take it's tracing level filter from RUST_LOG as shown below or it will
/// have a default. Whatever is created here is also passed on to
/// newly instantiated Visualizations for consistency across the garden
/// (hana = flower so a hana network is a garden)
///
/// - Allow all logs: `RUST_LOG=debug`
/// - Show only plugin logs: `RUST_LOG=hana_plugin=debug,bevy=off`
/// - Suppress all logs: `RUST_LOG=off`
pub fn setup_logging() -> String {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        //default
        EnvFilter::new(
            [
                "warn",      // Default level for everything
                "hana=warn", // in case you want to change this manually
            ]
            .join(","),
        )
    });

    let filter_str = env_filter.to_string();

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .with_target(true),
        )
        .with(env_filter)
        .init();

    filter_str
}
