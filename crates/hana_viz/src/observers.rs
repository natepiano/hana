//! Observer functions for visualization state transitions
use bevy::prelude::*;

use crate::entity::*;

/// Observer for when a visualization starts (Unstarted is removed)
pub fn on_visualization_start(
    trigger: Trigger<OnRemove, Unstarted>,
    visualizations: Query<&Visualization>,
    mut state_events: EventWriter<VisualizationStateChanged>,
) {
    let entity = trigger.entity();
    if let Ok(visualization) = visualizations.get(entity) {
        info!(
            "Starting visualization: {} from {:?}",
            visualization.name, visualization.path
        );

        state_events.send(VisualizationStateChanged {
            entity,
            new_state: "Starting".to_string(),
            error: None,
        });

        // Starting is added by the system that processes the StartVisualization event
    }
}

/// Observer for when a visualization becomes connected
pub fn on_visualization_connected(
    trigger: Trigger<OnAdd, NetworkHandle>,
    mut state_events: EventWriter<VisualizationStateChanged>,
    mut commands: Commands,
) {
    let entity = trigger.entity();

    info!("Visualization connected: {:?}", entity);

    commands
        .entity(entity)
        .remove::<Starting>()
        .insert(Connected);

    state_events.send(VisualizationStateChanged {
        entity,
        new_state: "Connected".to_string(),
        error: None,
    });
}

/// Observer for when a visualization is disconnected
pub fn on_visualization_disconnected(
    trigger: Trigger<OnAdd, Disconnected>,
    disconnected: Query<&Disconnected>,
    mut state_events: EventWriter<VisualizationStateChanged>,
) {
    let entity = trigger.entity();
    let error_str = disconnected.get(entity).ok().and_then(|d| d.error.clone());

    info!(
        "Visualization disconnected: {:?} (error: {:?})",
        entity, error_str
    );

    state_events.send(VisualizationStateChanged {
        entity,
        new_state: "Disconnected".to_string(),
        error: error_str,
    });
}

/// Observer for when a process handle is removed (process terminated)
pub fn on_process_terminated(
    trigger: Trigger<OnRemove, ProcessHandle>,
    mut commands: Commands,
    network_handles: Query<Entity, With<NetworkHandle>>,
    shutting_down: Query<(), With<ShuttingDown>>,
) {
    let entity = trigger.entity();

    info!("Visualization process terminated: {:?}", entity);

    // If the entity still has a network handle, remove it
    if network_handles.get(entity).is_ok() {
        commands.entity(entity).remove::<NetworkHandle>();
    }

    // Add Disconnected if not already shutting down
    if shutting_down.get(entity).is_err() {
        commands.entity(entity).insert(Disconnected {
            error: Some("Process terminated unexpectedly".to_string()),
        });
    }
}

/// Observer for when a visualization finishes shutting down
pub fn on_visualization_shutdown_complete(
    trigger: Trigger<OnRemove, ShuttingDown>,
    mut commands: Commands,
    mut state_events: EventWriter<VisualizationStateChanged>,
) {
    let entity = trigger.entity();

    info!("Visualization shutdown complete: {:?}", entity);

    // Return to unstarted state
    commands
        .entity(entity)
        .insert(Unstarted)
        .remove::<Connected>()
        .remove::<Disconnected>();

    state_events.send(VisualizationStateChanged {
        entity,
        new_state: "Unstarted".to_string(),
        error: None,
    });
}
