mod error;

use error::{Error, Result};
use error_stack::ResultExt;
use hana_controller::{Unstarted, Visualization};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, trace};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    trace!("Starting Hana visualization management system");

    let log_filter = setup_logging();

    let viz_path = PathBuf::from("./target/debug/basic-visualization");

    // Create and connect visualization using typestate pattern
    let viz = Visualization::<Unstarted>::start(viz_path, log_filter)
        .change_context(Error::Controller)?;

    trace!("Visualization process started, establishing connection...");

    let mut viz = viz.connect().await.change_context(Error::Controller)?;

    for _ in 0..8 {
        viz.ping().await.change_context(Error::Controller)?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    #[cfg(debug_assertions)]
    hana_process::debug::activate_parent_window().change_context(Error::Controller)?;

    // Shutdown
    info!("initiating shutdown...");
    viz.shutdown(Duration::from_secs(5))
        .await
        .change_context(Error::Controller)?;
    info!("shutdown complete");

    Ok(())
}

fn setup_logging() -> String {
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
