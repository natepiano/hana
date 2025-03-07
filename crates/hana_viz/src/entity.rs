use std::path::PathBuf;

use bevy::prelude::*;
use hana_network::HanaEndpoint;
use hana_process::Process;

// --- State Marker Components ---

/// Marker component for a visualization that has not been started
#[derive(Component, Debug)]
pub struct Unstarted;

/// Marker component for a visualization that is in the process of starting
#[derive(Component, Debug)]
pub struct Starting;

/// Marker component for a visualization that has successfully connected
#[derive(Component, Debug)]
pub struct Connected;

/// Marker component for a visualization that has failed to connect or has disconnected
#[derive(Component, Debug)]
pub struct Disconnected {
    /// Optional error information
    pub error: Option<String>,
}

/// Marker component for a visualization that is shutting down
#[derive(Component, Debug)]
pub struct ShuttingDown;

// --- Core Components ---

/// Main component for visualization metadata
#[derive(Component, Debug)]
pub struct Visualization {
    /// Path to the visualization executable
    pub path: PathBuf,

    /// Human-readable name for the visualization
    pub name: String,

    /// Environment filter for logging
    pub env_filter: String,

    /// Additional tags for categorization
    pub tags: Vec<String>,
}

/// Component to hold the process when started
#[derive(Component)]
pub struct ProcessHandle {
    /// The underlying process
    pub process: Process,
}

/// Component to hold the network connection when established
#[derive(Component)]
pub struct NetworkHandle {
    /// The network endpoint
    pub endpoint: HanaEndpoint,
}

// --- Events ---

/// Event to request starting a visualization
#[derive(Event, Debug, Clone)]
pub struct StartVisualization {
    /// Target entity to start (if None, creates a new visualization)
    pub entity: Option<Entity>,

    /// Path to the visualization executable (required for new visualizations)
    pub path: Option<PathBuf>,

    /// Name for the visualization (defaults to filename if not provided)
    pub name: Option<String>,

    /// Environment filter (defaults to parent process RUST_LOG)
    pub env_filter: Option<String>,

    /// Tags for categorization
    pub tags: Vec<String>,
}

/// Event to request shutting down a visualization
#[derive(Event, Debug, Clone)]
pub struct ShutdownVisualization {
    /// Target entity to shut down
    pub entity: Entity,

    /// Timeout in milliseconds
    pub timeout_ms: u64,
}

/// Event to request sending an instruction to a visualization
#[derive(Event, Debug, Clone)]
pub struct SendInstruction {
    /// Target entity
    pub entity: Entity,

    /// Instruction to send
    pub instruction: hana_network::Instruction,
}

/// Event emitted when a visualization's state changes
#[derive(Event, Debug, Clone)]
pub struct VisualizationStateChanged {
    /// The entity that changed state
    pub entity: Entity,

    /// The new state (as a string for simplicity)
    pub new_state: String,

    /// Optional error information
    pub error: Option<String>,
}
