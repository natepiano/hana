use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use flume::{Receiver, Sender};
use hana_visualization::{Connected, Unstarted, Visualization};
use tokio::runtime::Runtime;

use crate::prelude::*;

// original code
// // Create and connect visualization using typestate pattern (i.e. <Unstarted>)
// let viz = Visualization::<Unstarted>::start(viz_path, env_filter_str)
//     .await
//     .change_context(Error::Visualization)?;

// trace!("Visualization process started, establishing connection...");

// let mut viz = viz.connect().await.change_context(Error::Visualization)?;

// for _ in 0..8 {
//     viz.ping().await.change_context(Error::Visualization)?;
//     tokio::time::sleep(Duration::from_millis(500)).await;
// }

// fn shutdown_basic(user_input: Res<ActionState<Action>>) {
//     if user_input.just_pressed(&Action::StopVisualization) {
//         info!("stopping basic visualization");
//         // Shutdown
//         // info!("initiating shutdown...");
//         // viz.shutdown(Duration::from_secs(5))
//         //     .await
//         //     .change_context(Error::Visualization)?;
//         info!("shutdown complete");
//     }
// }

/// Plugin that sets up the Tokio runtime and communication channels between Bevy and Tokio
pub struct TokioRuntimePlugin;

impl Plugin for TokioRuntimePlugin {
    fn build(&self, app: &mut App) {
        // Create Tokio runtime
        let runtime = Arc::new(Runtime::new().expect("Failed to create Tokio runtime"));

        // Create channels for communication
        let (to_tokio_tx, to_tokio_rx) = flume::unbounded();
        let (to_bevy_tx, to_bevy_rx) = flume::unbounded();

        // Store runtime and channels as resources
        app.insert_resource(TokioSender(to_tokio_tx))
            .insert_resource(BevyReceiver(to_bevy_rx));

        // Spawn the Tokio worker thread
        spawn_tokio_thread(runtime, to_tokio_rx, to_bevy_tx);

        // Add system to process messages from Tokio
        app.add_systems(
            Update,
            process_non_error_events.after(crate::error_handling::process_error_events),
        );
    }
}

/// Resource containing the channel sender to Tokio
#[derive(Resource, Clone)]
pub struct TokioSender(pub Sender<VisualizationInstruction>);

/// Resource containing the channel receiver from Tokio
#[derive(Resource)]
pub struct BevyReceiver(pub Receiver<BevyVisualizationEvent>);

/// Commands that Bevy can send to Tokio
#[derive(Debug, Clone)]
pub enum VisualizationInstruction {
    Start {
        path:       std::path::PathBuf,
        env_filter: String,
    },
    Ping, // No ID required - implicit "ping the active one"
    Shutdown {
        timeout: Duration,
    },
    // Add more commands as needed
}

/// Events that Tokio can send back to Bevy
#[derive(Debug)]
pub enum BevyVisualizationEvent {
    Started,
    Pinged,
    Shutdown,
    Failed(error_stack::Report<Error>), // Send the full Report
}

/// Unique identifier for visualizations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VisualizationId(pub u64);

fn spawn_tokio_thread(
    runtime: Arc<Runtime>,
    instructions_rx: Receiver<VisualizationInstruction>,
    events_tx: Sender<BevyVisualizationEvent>,
) {
    std::thread::spawn(move || {
        // Active visualization state, wrapped in Arc and Mutex for thread safety
        let visualization_state = Arc::new(tokio::sync::Mutex::new(VisualizationState::new()));

        // Process commands from Bevy
        runtime.block_on(async {
            while let Ok(instruction) = instructions_rx.recv_async().await {
                // Clone the state and sender for the spawned task
                let state = Arc::clone(&visualization_state);
                let tx = events_tx.clone();

                // Process each instruction concurrently
                runtime.spawn(async move {
                    match instruction {
                        VisualizationInstruction::Start { path, env_filter } => {
                            VisualizationState::handle_start(state, tx, path, env_filter).await;
                        }
                        VisualizationInstruction::Ping => {
                            VisualizationState::handle_ping(state, tx).await;
                        }
                        VisualizationInstruction::Shutdown { timeout } => {
                            VisualizationState::handle_shutdown(state, tx, timeout).await;
                        }
                    }
                });
            }
        });
    });
}

/// Encapsulates the state and operations for visualization management
struct VisualizationState {
    active_visualization: Option<Visualization<Connected>>,
}

impl VisualizationState {
    fn new() -> Self {
        Self {
            active_visualization: None,
        }
    }

    async fn handle_ping(state: Arc<tokio::sync::Mutex<Self>>, tx: Sender<BevyVisualizationEvent>) {
        debug!("Handling ping command");

        // Take visualization temporarily (if available)
        let viz_option = {
            let mut state_guard = state.lock().await;
            state_guard.active_visualization.take()
        };

        // Process based on whether we have a visualization
        match viz_option {
            Some(mut viz) => {
                match viz.ping().await {
                    Ok(_) => {
                        debug!("Ping successful");
                        // Put visualization back
                        let mut state_guard = state.lock().await;
                        state_guard.active_visualization = Some(viz);
                        tx.send(BevyVisualizationEvent::Pinged).ok();
                    }
                    Err(err) => {
                        error!("Ping failed: {err:?}");
                        // Don't put visualization back - it's in a bad state
                        let report = err.change_context(Error::Visualization);
                        tx.send(BevyVisualizationEvent::Failed(report)).ok();
                    }
                }
            }
            None => {
                warn!("No active visualization to ping");
                let report = error_stack::Report::new(Error::Visualization)
                    .attach_printable("No active visualization to ping");
                tx.send(BevyVisualizationEvent::Failed(report)).ok();
            }
        }
    }

    // Updated handle_start with cleaner mutex handling
    async fn handle_start(
        state: Arc<tokio::sync::Mutex<Self>>,
        tx: Sender<BevyVisualizationEvent>,
        path: std::path::PathBuf,
        env_filter: String,
    ) {
        debug!("Handling start command for {:?}", path);

        // Shut down any existing visualization
        {
            let viz_option = {
                let mut state_guard = state.lock().await;
                state_guard.active_visualization.take()
            };

            if let Some(viz) = viz_option {
                match viz.shutdown(Duration::from_secs(3)).await {
                    Ok(_) => {
                        tx.send(BevyVisualizationEvent::Shutdown).ok();
                    }
                    Err(err) => {
                        let report = err.change_context(Error::Visualization);
                        tx.send(BevyVisualizationEvent::Failed(report)).ok();
                        return;
                    }
                }
            }
        }

        // Start a new visualization
        match Visualization::<Unstarted>::start(path.clone(), env_filter).await {
            Ok(started) => {
                match started.connect().await {
                    Ok(connected) => {
                        // Store the new visualization
                        let mut state_guard = state.lock().await;
                        state_guard.active_visualization = Some(connected);
                        tx.send(BevyVisualizationEvent::Started).ok();
                    }
                    Err(err) => {
                        let report = err.change_context(Error::Visualization);
                        tx.send(BevyVisualizationEvent::Failed(report)).ok();
                    }
                }
            }
            Err(err) => {
                let report = err.change_context(Error::Visualization);
                tx.send(BevyVisualizationEvent::Failed(report)).ok();
            }
        }
    }

    // Updated handle_shutdown with cleaner mutex handling
    async fn handle_shutdown(
        state: Arc<tokio::sync::Mutex<Self>>,
        tx: Sender<BevyVisualizationEvent>,
        timeout: Duration,
    ) {
        debug!("Handling shutdown command");

        // Take visualization temporarily (if available)
        let viz_option = {
            let mut state_guard = state.lock().await;
            state_guard.active_visualization.take()
        };

        match viz_option {
            Some(viz) => match viz.shutdown(timeout).await {
                Ok(_) => {
                    debug!("Shutdown successful");
                    tx.send(BevyVisualizationEvent::Shutdown).ok();
                }
                Err(err) => {
                    error!("Shutdown failed: {err:?}");
                    let report = err.change_context(Error::Visualization);
                    tx.send(BevyVisualizationEvent::Failed(report)).ok();
                }
            },
            None => {
                warn!("No active visualization to shut down");
                let report = error_stack::Report::new(Error::Visualization)
                    .attach_printable("No active visualization to shut down");
                tx.send(BevyVisualizationEvent::Failed(report)).ok();
            }
        }
    }
}

fn process_non_error_events(
    bevy_receiver: Res<BevyReceiver>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    // Process all pending messages from Tokio
    while let Ok(event) = bevy_receiver.0.try_recv() {
        match event {
            BevyVisualizationEvent::Started => {
                info!("Visualization started successfully");
                if let Ok(mut window) = windows.get_single_mut() {
                    window.focused = true;
                }
            }
            BevyVisualizationEvent::Pinged => {
                info!("Visualization pinged successfully");
            }
            BevyVisualizationEvent::Shutdown => {
                info!("Visualization shut down successfully");
            }
            // Don't handle Failed here - let the error handling system do it
            BevyVisualizationEvent::Failed(_) => {}
        }
    }
}
