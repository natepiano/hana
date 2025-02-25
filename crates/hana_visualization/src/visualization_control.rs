//! VisualizationControl bevy plugin for use in bevy based visualizations
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use hana_network::{Instruction, Result};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error};

/// The `VisualizationControl` plugin enables remote control of your visualization.
///
/// # Logging
/// This plugin uses tracing for logging and respects the host application's
/// tracing configuration. To control log output:
///
/// - Allow all logs: `RUST_LOG=debug`
/// - Show only plugin logs: `RUST_LOG=hana_visualization=debug,bevy=off`
/// - Suppress all logs: `RUST_LOG=off`
pub struct VisualizationControl;

impl Plugin for VisualizationControl {
    fn build(&self, app: &mut App) {
        // Channel for sending commands from network thread to Bevy app
        let (tx, rx) = channel();
        let rx = Arc::new(Mutex::new(rx));

        // Create and spawn the tokio runtime in a separate thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime");

            rt.block_on(async {
                let listener = TcpListener::bind("127.0.0.1:3001")
                    .await
                    .expect("failed to bind to port");
                debug!("started listening for hana instructions on port 3001");

                // Accept a single connection
                match listener.accept().await {
                    Ok((stream, _)) => {
                        //if let Err(e) = handle_connection(stream, tx).await {
                        if let Err(e) = handle_connection(stream, tx).await {
                            eprintln!("Connection error: {}", e);
                        }
                    }
                    Err(e) => eprintln!("Accept failed: {}", e),
                }
            });
        });

        app.add_event::<VisualizationEvent>()
            .insert_resource(InstructionReceiver(rx))
            .add_systems(Update, (handle_instructions, handle_visualization_events));
    }
}

#[derive(Event)]
pub enum VisualizationEvent {
    Ping,
    // Add other visualization events here
}

#[derive(Resource)]
struct InstructionReceiver(Arc<Mutex<std::sync::mpsc::Receiver<Instruction>>>);

// Convert network instructions to Bevy events
fn handle_instructions(
    receiver: Res<InstructionReceiver>,
    mut viz_events: EventWriter<VisualizationEvent>,
) {
    if let Ok(rx) = receiver.0.lock() {
        while let Ok(instruction) = rx.try_recv() {
            let _ = match instruction {
                Instruction::Ping => viz_events.send(VisualizationEvent::Ping),
                _ => return,
            };
        }
    }
}

// Handle the Bevy events
fn handle_visualization_events(mut events: EventReader<VisualizationEvent>) {
    for event in events.read() {
        match event {
            VisualizationEvent::Ping => debug!("Ping event received in Bevy app"),
        }
    }
}

/// The tx channel acts as a thread-safe queue between the network operations and the game logic.
async fn handle_connection(
    mut stream: TcpStream,
    tx: std::sync::mpsc::Sender<Instruction>,
) -> Result<()> {
    debug!("New connection established!");

    loop {
        match hana_network::receive_instruction(&mut stream).await {
            Ok(Some(instruction)) => {
                match instruction {
                    Instruction::Shutdown => {
                        debug!("Received shutdown instruction, terminating...");
                        std::process::exit(0);
                    }
                    // Forward other instructions to Bevy
                    _ => {
                        if let Err(e) = tx.send(instruction) {
                            error!("Failed to send instruction: {}", e);
                            break;
                        }
                    }
                }
            }
            Ok(None) => {
                debug!("Connection closed by controller");
                break;
            }
            Err(e) => {
                error!("Connection error: {}", e);
                break;
            }
        }
    }

    debug!("Connection handler exiting, process will terminate");
    std::process::exit(0);
}
