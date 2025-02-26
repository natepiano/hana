//! VisualizationControl bevy plugin for use in bevy based visualizations
use bevy::prelude::*;
use hana_network::{Instruction, Result};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use tracing::{debug, error};

/// The `VisualizationControl` plugin enables remote control of your visualization.
///
/// # Logging
/// This plugin uses tracing for logging and respects the host application's
/// tracing configuration. To control log output:
///
/// - Allow all logs: `RUST_LOG=debug`
/// - Show only plugin logs: `RUST_LOG=hana_plugin=debug,bevy=off`
/// - Suppress all logs: `RUST_LOG=off`
pub struct VisualizationControl;

impl Plugin for VisualizationControl {
    fn build(&self, app: &mut App) {
        // Channel for sending commands from network thread to Bevy app
        let (tx, rx) = channel(32);
        // let rx = Arc::new(Mutex::new(rx));
        // let rx = Arc::new(Mutex::new(Some(rx)));

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
// struct InstructionReceiver(Arc<Mutex<std::sync::mpsc::Receiver<Instruction>>>);
struct InstructionReceiver(Receiver<Instruction>);

// Convert network instructions to Bevy events
fn handle_instructions(
    mut receiver: ResMut<InstructionReceiver>,
    mut viz_events: EventWriter<VisualizationEvent>,
) {
    while let Ok(instruction) = receiver.0.try_recv() {
        let _ = match instruction {
            Instruction::Ping => viz_events.send(VisualizationEvent::Ping),
            _ => return,
        };
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
async fn handle_connection(mut stream: TcpStream, tx: Sender<Instruction>) -> Result<()> {
    debug!("New connection established!");

    loop {
        match hana_network::receive_instruction(&mut stream).await {
            Ok(Some(instruction)) => {
                match instruction {
                    Instruction::Shutdown => {
                        debug!("Received shutdown instruction, terminating...");
                        std::process::exit(0);
                    }
                    // Forward other instructions to Bevy using tokio's send
                    _ => {
                        if let Err(e) = tx.send(instruction).await {
                            error!("Failed to send instruction: {}", e);
                            return Ok(()); // Return Result to satisfy the function signature
                        }
                    }
                }
            }
            Ok(None) => {
                debug!("Connection closed by controller");
                return Ok(());
            }
            Err(e) => {
                error!("Connection error: {}", e);
                return Err(e);
            }
        }
    }
}
