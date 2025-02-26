//! utility fn's for configuring tracing. we're using the name in a generic sense given
//! the only fn currently here is to configure tracing_subscriber
//! it's fine
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn setup_logging() -> String {
    let maybe_env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        //default
        EnvFilter::new(
            [
                "warn",      // Default level for everything
                "hana=warn", // in case you want to change this manually
            ]
            .join(","),
        )
    });

    let filter_str = maybe_env_filter.to_string();

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .with_target(true),
        )
        .with(maybe_env_filter)
        .init();

    filter_str
}
