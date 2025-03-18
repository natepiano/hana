use std::collections::HashMap;

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
