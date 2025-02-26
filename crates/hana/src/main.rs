mod error;
mod utils;

use error::{Error, Result};
use error_stack::ResultExt;
use hana_visualization::{Unstarted, Visualization};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, trace};

#[tokio::main]
async fn main() -> Result<()> {
    trace!("Starting Hana visualization management system");

    let log_filter = utils::setup_logging();

    let viz_path = PathBuf::from("./target/debug/basic-visualization");

    // Create and connect visualization using typestate pattern (i.e. <Unstarted>)
    let viz = Visualization::<Unstarted>::start(viz_path, log_filter)
        .change_context(Error::Controller)?;

    trace!("Visualization process started, establishing connection...");

    let mut viz = viz.connect().await.change_context(Error::Controller)?;

    for _ in 0..8 {
        viz.ping().await.change_context(Error::Controller)?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // return to the editor - very useful when you're in full screen in macos
    // after the visualization ends, you'll pop right back to this editor window
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
