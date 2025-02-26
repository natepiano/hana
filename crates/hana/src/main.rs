mod error;

use std::path::PathBuf;
use std::time::Duration;

use error::{Error, Result};
use error_stack::ResultExt;
use hana_controller::{Unstarted, Visualization};

use tracing::{info, trace};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();
    trace!("Starting Hana visualization management system");

    let viz_path = PathBuf::from("./target/debug/basic-visualization");

    // Create and connect visualization using typestate pattern
    let viz = Visualization::<Unstarted>::start(viz_path).change_context(Error::Controller)?;

    trace!("Visualization process started, establishing connection...");

    let mut viz = viz.connect().await.change_context(Error::Controller)?;

    for _ in 0..8 {
        viz.ping().await.change_context(Error::Controller)?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    #[cfg(debug_assertions)]
    hana_window::activate_parent_window().change_context(Error::Window)?;

    // Shutdown
    info!("Initiating shutdown...");
    viz.shutdown(Duration::from_secs(5))
        .await
        .change_context(Error::Controller)?;
    info!("Shutdown complete");

    Ok(())
}

fn setup_logging() {
    // Initialize subscriber with default configuration and filtering
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .with_target(true),
        )
        .with(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,hana=info")),
        )
        .init();
}
