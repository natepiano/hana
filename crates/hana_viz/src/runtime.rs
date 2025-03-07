use std::sync::Arc;
use std::time::Duration;
use std::{collections::HashMap, path::PathBuf};

use bevy::prelude::*;
use error_stack::{Report, ResultExt};
use hana_async::{AsyncRuntime, CommandSender, EventReceiver};
use hana_network::{HanaEndpoint, Instruction};
use hana_process::Process;
use tokio::sync::Mutex;

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub enum VisualizationCommand {
    /// Start a visualization process and connect to it
    Start {
        entity: Entity,
        path: PathBuf,
        env_filter: String,
    },
    /// Send a network instruction to the running visualization
    Send {
        entity: Entity,
        instruction: Instruction,
    },
    /// Terminate the visualization process (with optional timeout)
    Terminate { entity: Entity, timeout: Duration },
}

/// Events sent from the async worker back to Bevy systems
#[derive(Debug)]
pub enum VisualizationEvent {
    /// Visualization process was started and connected successfully
    Started { entity: Entity },
    /// An instruction was sent successfully
    InstructionSent {
        entity: Entity,
        instruction: Instruction,
    },
    /// A visualization has shut down
    Shutdown { entity: Entity },
    /// An error occurred
    Error {
        entity: Entity,
        error: Report<Error>,
    },
}

/// Resource for sending commands to the async worker
#[derive(Resource, Clone)]
pub struct VisualizationCommandSender(pub CommandSender<VisualizationCommand>);

/// Resource for receiving events from the async worker
#[derive(Resource)]
pub struct VisualizationEventReceiver(pub EventReceiver<VisualizationEvent>);

/// Initialize the async runtime for visualization management
pub fn setup_visualization_runtime(
    async_runtime: &AsyncRuntime,
) -> (VisualizationCommandSender, VisualizationEventReceiver) {
    // Create shared state for the worker
    let state = Arc::new(Mutex::new(VisualizationState::new()));

    // Use the create_worker method from AsyncRuntime
    let (cmd_sender, event_receiver) =
        async_runtime.create_worker(move |command: VisualizationCommand| {
            let state = Arc::clone(&state);
            async move {
                match command {
                    VisualizationCommand::Start {
                        entity,
                        path,
                        env_filter,
                    } => match handle_start(state.clone(), entity, path, env_filter).await {
                        Ok(()) => vec![VisualizationEvent::Started { entity }],
                        Err(err) => vec![VisualizationEvent::Error { entity, error: err }],
                    },
                    VisualizationCommand::Send {
                        entity,
                        instruction,
                    } => {
                        let instruction_clone = instruction.clone();
                        let was_shutdown = matches!(instruction, Instruction::Shutdown);

                        match handle_send(state.clone(), entity, instruction).await {
                            Ok(()) => {
                                let mut events = vec![VisualizationEvent::InstructionSent {
                                    entity,
                                    instruction: instruction_clone,
                                }];

                                if was_shutdown {
                                    events.push(VisualizationEvent::Shutdown { entity });
                                }

                                events
                            }
                            Err(err) => vec![VisualizationEvent::Error { entity, error: err }],
                        }
                    }
                    VisualizationCommand::Terminate { entity, timeout } => {
                        match handle_terminate(state.clone(), entity, timeout).await {
                            Ok(()) => vec![VisualizationEvent::Shutdown { entity }],
                            Err(err) => vec![VisualizationEvent::Error { entity, error: err }],
                        }
                    }
                }
            }
        });

    (
        VisualizationCommandSender(cmd_sender),
        VisualizationEventReceiver(event_receiver),
    )
}

/// Internal state for the visualization worker
struct VisualizationState {
    active_visualizations: HashMap<Entity, (Process, HanaEndpoint)>,
}

impl VisualizationState {
    fn new() -> Self {
        Self {
            active_visualizations: HashMap::new(),
        }
    }
}

/// Handle starting a visualization
async fn handle_start(
    state: Arc<Mutex<VisualizationState>>,
    entity: Entity,
    path: PathBuf,
    env_filter: String,
) -> Result<()> {
    // First, shut down any existing visualization for this entity
    {
        let mut state_guard = state.lock().await;
        if state_guard.active_visualizations.remove(&entity).is_some() {
            debug!("Removed existing visualization for entity {:?}", entity);
        }
    }

    // Start the new process
    debug!(
        "Starting visualization process for entity {:?}: {:?}",
        entity, path
    );

    // Use Process::run to start the process
    let process = Process::run(path.clone(), env_filter.clone())
        .await
        .change_context(Error::Process)?;

    // Try to connect to it
    debug!("Process started for entity {:?}, connecting...", entity);
    let endpoint = HanaEndpoint::connect_to_visualization()
        .await
        .change_context(Error::Network)?;

    debug!(
        "Successfully connected to visualization for entity {:?}",
        entity
    );

    // Store in state
    let mut state_guard = state.lock().await;
    state_guard
        .active_visualizations
        .insert(entity, (process, endpoint));

    Ok(())
}

/// Handle terminating a visualization process
async fn handle_terminate(
    state: Arc<Mutex<VisualizationState>>,
    entity: Entity,
    timeout: Duration,
) -> Result<()> {
    // Take the specific visualization out to avoid lock issues
    let viz_option = {
        let mut state_guard = state.lock().await;
        state_guard.active_visualizations.remove(&entity)
    };

    if let Some((process, _)) = viz_option {
        debug!(
            "Terminating visualization process for entity {:?} with timeout: {:?}",
            entity, timeout
        );

        // Since we don't have a direct terminate method with timeout,
        // we'll just drop the process which should clean up
        // In a real implementation, this would need proper process termination logic
        drop(process);
        Ok(())
    } else {
        debug!(
            "No active visualization to terminate for entity {:?}",
            entity
        );
        // Still return OK if there's no process to terminate
        Ok(())
    }
}

/// Handle sending an instruction to the visualization
async fn handle_send(
    state: Arc<Mutex<VisualizationState>>,
    entity: Entity,
    instruction: Instruction,
) -> Result<()> {
    // Get a clone of the visualization if it exists
    let viz_option = {
        let mut state_guard = state.lock().await;
        state_guard.active_visualizations.remove(&entity)
    };

    if let Some((process, mut endpoint)) = viz_option {
        debug!(
            "Sending instruction to entity {:?}: {:?}",
            entity, instruction
        );

        let was_shutdown = matches!(instruction, Instruction::Shutdown);

        // Send the instruction
        endpoint
            .send(&instruction)
            .await
            .change_context(Error::Network)?;

        debug!("Instruction sent successfully to entity {:?}", entity);

        // If it was not a shutdown instruction, put the visualization back in the map
        if !was_shutdown {
            let mut state_guard = state.lock().await;
            state_guard
                .active_visualizations
                .insert(entity, (process, endpoint));
        }

        Ok(())
    } else {
        // Return error if no visualization found
        Err(
            Report::new(Error::NoActiveVisualization).attach_printable(format!(
                "No active visualization to send instruction to for entity {:?}",
                entity
            )),
        )
    }
}
