mod error;
mod utils;

use std::path::PathBuf;
use std::time::Duration;

use bevy::prelude::*;
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

    // Shutdown
    info!("initiating shutdown...");
    viz.shutdown(Duration::from_secs(5))
        .await
        .change_context(Error::Visualization)?;
    info!("shutdown complete");

    // hook up leafwing and then create a button that will launch the app and another that will
    // close it
    let _ = App::new().add_plugins(DefaultPlugins).run();

    Ok(())
}
