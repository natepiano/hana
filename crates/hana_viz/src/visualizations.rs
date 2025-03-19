use std::{collections::HashMap, path::PathBuf};

use bevy::prelude::*;
use hana_network::HanaEndpoint;
use hana_process::Process;

/// Visualization state maintained by the worker
pub struct Visualizations {
    pub active_visualizations: HashMap<Entity, (Process, HanaEndpoint)>,
}

impl Visualizations {
    pub fn new() -> Self {
        Self {
            active_visualizations: HashMap::new(),
        }
    }
}

/// Resource to track visualization connections that are in progress
#[derive(Resource, Default)]
pub struct PendingConnections {
    /// Mapping from reserved entity IDs to details for pending connections
    pub pending: HashMap<Entity, VisualizationDetails>,

    /// List of failed connection attempts for UI feedback
    pub failed: Vec<(VisualizationDetails, String)>,
}

/// Details needed to create a visualization
#[derive(Clone, Debug)]
pub struct VisualizationDetails {
    /// Path to the visualization executable
    pub path: PathBuf,

    /// Human-readable name for the visualization
    pub name: String,

    /// Environment filter for logging
    pub env_filter: String,
}
