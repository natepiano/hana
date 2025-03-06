use std::env;
use std::path::PathBuf;
use std::time::Duration;

use bevy::prelude::*;
use error_stack::ResultExt;
use tracing::info;

use crate::action::*;
use crate::prelude::*;
use crate::tokio_runtime::{TokioSender, VisualizationInstruction};

/// Proof of concept plugin to control a visualization for basic functionality
pub struct BasicPlugin;

impl Plugin for BasicPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                start_visualization.run_if(just_pressed(Action::Start)),
                ping_visualization.run_if(just_pressed(Action::Ping)),
                shutdown_visualization.run_if(just_pressed(Action::Shutdown)),
            ),
        );
    }
}

/// return the environment variable for RUST_LOG so we can
/// pass it to the visualization process
fn get_rust_log_value() -> String {
    env::var("RUST_LOG").unwrap_or_else(|_| {
        // Return this default if RUST_LOG is not set
        "info,hana=info".to_string()
    })
}

/// System to start a visualization when the StartVisualization action is triggered
fn start_visualization(tokio_sender: Res<TokioSender>, exit: EventWriter<AppExit>) {
    info!("Starting visualization...");

    let path = PathBuf::from("./target/debug/basic-visualization");
    let env_filter = get_rust_log_value();

    // Send command to Tokio runtime
    send_visualization_instruction(
        tokio_sender,
        VisualizationInstruction::Start { path, env_filter },
        exit,
    );
}

/// send visualization instructions from the systems in basic.rs so that we have
/// the same error handling for all of them
fn send_visualization_instruction(
    tokio_sender: Res<TokioSender>,
    instruction: VisualizationInstruction,
    mut exit: EventWriter<AppExit>,
) {
    let result = tokio_sender
        .0
        .send(instruction.clone())
        .change_context(Error::TokioRuntimeChannelClosed)
        .attach_printable_lazy(|| format!("VisualizationCommand: {instruction:?}"));

    if let Err(report) = result {
        error!("CRITICAL ERROR: {report:?}");
        exit.send(AppExit::Error(std::num::NonZeroU8::new(1).unwrap()));
    }
}

/// System to ping a visualization when the PingVisualization action is triggered
fn ping_visualization(tokio_sender: Res<TokioSender>, exit: EventWriter<AppExit>) {
    info!("Attempting to ping visualization...");
    send_visualization_instruction(tokio_sender, VisualizationInstruction::Ping, exit);
}

/// System to shutdown a visualization when the StopVisualization action is triggered
fn shutdown_visualization(tokio_sender: Res<TokioSender>, exit: EventWriter<AppExit>) {
    info!("Attempting to shut down visualization...");
    send_visualization_instruction(
        tokio_sender,
        VisualizationInstruction::Shutdown {
            timeout: Duration::from_secs(5),
        },
        exit,
    );
}
