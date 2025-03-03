mod error;
mod utils;

use std::path::PathBuf;
use std::time::Duration;

use error::{Error, Result};
use error_stack::ResultExt;
use hana_visualization::{Unstarted, Visualization};
use tracing::{info, trace};

#[tokio::main]
async fn main() -> Result<()> {
    trace!("Starting Hana visualization management system");

    let env_filter_str = utils::setup_logging();

    let viz_path = PathBuf::from("./target/debug/basic-visualization");

    // Create and connect visualization using typestate pattern (i.e. <Unstarted>)
    let viz = Visualization::<Unstarted>::start(viz_path, env_filter_str)
        .await
        .change_context(Error::Visualization)?;

    trace!("Visualization process started, establishing connection...");

    let mut viz = viz.connect().await.change_context(Error::Visualization)?;

    for _ in 0..8 {
        viz.ping().await.change_context(Error::Visualization)?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // return to the editor - very useful when you're in full screen in macos
    // after the visualization ends, you'll pop right back to this editor window
    #[cfg(debug_assertions)]
    match hana_process::debug::activate_parent_window() {
        Ok(_) => {
            info!("Successfully activated parent window");
        }
        Err(report) => {
            // Log the full error report with context and attached printable messages
            tracing::warn!("Failed to activate parent window: {report:?}");
        }
    };

    // Shutdown
    info!("initiating shutdown...");
    viz.shutdown(Duration::from_secs(5))
        .await
        .change_context(Error::Visualization)?;
    info!("shutdown complete");

    Ok(())
}
