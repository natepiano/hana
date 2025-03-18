//! Observer functions for visualization state transitions
//! at this point, this is more of a debugging tool but very helpful for that
//! in the future observed behavior probably can and should cause subsequent
//! changes in the UI
use bevy::prelude::*;

use crate::visualization::{
    Connected, Disconnected, ShuttingDown, Starting, Unstarted, Visualization,
};

/// Observer for when a visualization enters the Starting state
pub fn on_visualization_starting(
    trigger: Trigger<OnAdd, Starting>,
    visualizations: Query<&Visualization>,
) {
    let entity = trigger.entity();
    if let Ok(visualization) = visualizations.get(entity) {
        info!(
            "observing Trigger<OnAdd, Starting>: {} (path: {:?}, entity: {:?})",
            visualization.name, visualization.path, entity
        );
    } else {
        info!("Visualization starting: entity {:?}", entity);
    }
}

/// Observer for when a visualization becomes connected
pub fn on_visualization_connected(
    trigger: Trigger<OnAdd, Connected>,
    visualizations: Query<&Visualization>,
) {
    let entity = trigger.entity();
    if let Ok(visualization) = visualizations.get(entity) {
        info!(
            "observing Trigger<OnAdd, Connected>: {} (entity: {:?})",
            visualization.name, entity
        );
    } else {
        info!("Visualization connected: entity {:?}", entity);
    }
}

/// Observer for when a visualization is disconnected
pub fn on_visualization_disconnected(
    trigger: Trigger<OnAdd, Disconnected>,
    visualizations: Query<(&Disconnected, Option<&Visualization>)>,
) {
    let entity = trigger.entity();
    if let Ok((disconnected, maybe_viz)) = visualizations.get(entity) {
        let name = maybe_viz.map(|v| v.name.as_str()).unwrap_or("Unknown");
        info!(
            "observing Trigger<OnAdd, Disconnected>: {} (entity: {:?}, error: {:?})",
            name, entity, disconnected.error
        );
    } else {
        info!("Visualization disconnected: entity {:?}", entity);
    }
}

/// Observer for when a visualization enters the shutting down state
pub fn on_visualization_shutting_down(
    trigger: Trigger<OnAdd, ShuttingDown>,
    visualizations: Query<&Visualization>,
) {
    let entity = trigger.entity();
    if let Ok(visualization) = visualizations.get(entity) {
        info!(
            "observing Trigger<OnAdd, ShuttingDown>: {} (entity: {:?})",
            visualization.name, entity
        );
    } else {
        info!("Visualization shutting down: entity {:?}", entity);
    }
}

/// Observer for when a visualization returns to unstarted state
pub fn on_visualization_unstarted(
    trigger: Trigger<OnAdd, Unstarted>,
    visualizations: Query<&Visualization>,
) {
    let entity = trigger.entity();
    if let Ok(visualization) = visualizations.get(entity) {
        info!(
            "observing Trigger<OnAdd, Unstarted>: {} (entity: {:?})",
            visualization.name, entity
        );
    } else {
        info!(
            "observing Trigger<OnAdd, Unstarted> (else block of if let Ok()): entity {:?}",
            entity
        );
    }
}
