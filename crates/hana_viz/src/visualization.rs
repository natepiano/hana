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

// --- Events ---
/// Event to request starting a visualization
#[derive(Event, Debug, Clone)]
pub struct StartVisualizationEvent {
    /// Path to the visualization executable
    pub path: PathBuf,

    /// Name for the visualization (defaults to filename if not specified)
    pub name: String,

    /// Environment filter for logging
    pub env_filter: String,
}

/// Event to request shutting down a visualization
#[derive(Event, Debug, Clone)]
pub struct ShutdownVisualizationEvent {
    /// Target entity to shut down
    pub entity: Entity,

    /// Timeout in milliseconds before forced termination
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
