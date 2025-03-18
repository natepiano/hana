//! visualization entity states and events that correspond
//! to instructions that can be sent to the visualization
use std::path::PathBuf;

use bevy::prelude::*;

/// Main component for visualization metadata
#[derive(Component, Debug)]
pub struct Visualization {
    /// Path to the visualization executable
    pub path: PathBuf,

    /// Human-readable name for the visualization
    pub name: String,

    /// Environment filter for logging
    pub env_filter: String,
}

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

// --- Events ---
/// Event to request starting a visualization
#[derive(Event, Debug, Clone)]
pub struct StartVisualizationEvent {
    /// Target entity to start (if None, creates a new visualization)
    pub entity: Option<Entity>,

    /// Path to the visualization executable (required for new visualizations)
    pub path: Option<PathBuf>,

    /// Name for the visualization (defaults to filename if not provided)
    pub name: Option<String>,

    /// Environment filter (defaults to parent process RUST_LOG)
    pub env_filter: Option<String>,
}

/// Event to request shutting down a visualization
#[derive(Event, Debug, Clone)]
pub struct ShutdownVisualizationEvent {
    /// Target entity to shut down
    pub entity: Entity,

    /// Timeout in milliseconds
    pub timeout_ms: u64,
}

/// Event to request sending an instruction to a visualization
#[derive(Event, Debug, Clone)]
pub struct SendInstructionEvent {
    /// Target entity
    pub entity: Entity,

    /// Instruction to send
    pub instruction: hana_network::Instruction,
}
