use crate::visualization::Visualization;
use bevy::prelude::*;

/// Observer for when a new visualization is connected
pub fn on_visualization_added(
    trigger: Trigger<OnAdd, Visualization>,
    visualizations: Query<&Visualization>,
) {
    let entity = trigger.entity();
    if let Ok(visualization) = visualizations.get(entity) {
        info!(
            "observer: new visualization connected: {} (path: {:?}, entity: {:?})",
            visualization.name, visualization.path, entity
        );
    }
}

/// Observer for when a visualization is removed
pub fn on_visualization_removed(trigger: Trigger<OnRemove, Visualization>) {
    let entity = trigger.entity();
    info!("observer: visualization removed: entity {:?}", entity);
}
