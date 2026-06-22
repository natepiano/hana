use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::WindowMode;
use bevy_window_manager::WindowRestoreMismatch;
use bevy_window_manager::WindowRestored;

#[derive(Event, Reflect)]
#[reflect(Event)]
pub(crate) struct SpawnManagedWindow;

#[derive(Event, Reflect)]
#[reflect(Event)]
pub(crate) struct SetBorderlessFullscreen;

#[derive(Event, Reflect)]
#[reflect(Event)]
pub(crate) struct SetWindowed;

#[derive(Event, Reflect)]
#[reflect(Event)]
pub(crate) struct SetExclusiveFullscreen;

#[derive(Event, Reflect)]
#[reflect(Event)]
pub(crate) struct TogglePersistence;

#[derive(Event, Reflect)]
#[reflect(Event)]
pub(crate) struct ClearStateAndQuit;

#[derive(Event, Reflect)]
#[reflect(Event)]
pub(crate) struct QuitApp;

#[derive(Debug, Clone, Reflect)]
pub(crate) struct MonitorDifference {
    pub(crate) expected: usize,
    pub(crate) actual:   usize,
}

#[derive(Debug, Clone, Reflect)]
pub(crate) struct WindowModeDifference {
    pub(crate) expected: WindowMode,
    pub(crate) actual:   WindowMode,
}

#[derive(Debug, Clone, Reflect)]
pub(crate) struct PhysicalPositionMismatch {
    pub(crate) expected_physical_position: Option<IVec2>,
    pub(crate) actual_physical_position:   Option<IVec2>,
}

#[derive(Debug, Clone, Reflect)]
pub(crate) struct LogicalPositionMismatch {
    pub(crate) expected_logical_position: Option<IVec2>,
    pub(crate) actual_logical_position:   Option<IVec2>,
}

#[derive(Debug, Clone, Reflect)]
pub(crate) struct PhysicalSizeMismatch {
    pub(crate) expected_physical_size: UVec2,
    pub(crate) actual_physical_size:   UVec2,
}

#[derive(Debug, Clone, Reflect)]
pub(crate) struct LogicalSizeMismatch {
    pub(crate) expected_logical_size: UVec2,
    pub(crate) actual_logical_size:   UVec2,
}

#[derive(Debug, Clone, Reflect)]
pub(crate) struct ScaleFactorDifference {
    pub(crate) expected: f64,
    pub(crate) actual:   f64,
}

#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub(crate) struct WindowRestoredReceived {
    pub(crate) physical_position: Option<IVec2>,
    pub(crate) physical_size:     UVec2,
    pub(crate) window_mode:       WindowMode,
    pub(crate) monitor:           usize,
}

/// Adapts the flat `expected_*` / `actual_*` shape of `WindowRestoreMismatch` into
/// nested comparison structs for BRP inspection. If the public event's field layout
/// changes, this resource's unpacking (in `on_window_restore_mismatch`) must change with it.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub(crate) struct WindowRestoreMismatchReceived {
    pub(crate) monitor_difference:     MonitorDifference,
    pub(crate) physical_size_mismatch: PhysicalSizeMismatch,
    pub(crate) window_mode_difference: WindowModeDifference,
}

#[derive(Resource, Debug, Default, Reflect)]
#[reflect(Resource)]
pub(crate) struct WindowsSettledCount {
    pub(crate) value: usize,
}

#[derive(Clone)]
pub(crate) struct CachedMismatchState {
    pub(crate) physical_position_mismatch: PhysicalPositionMismatch,
    pub(crate) logical_position_mismatch:  LogicalPositionMismatch,
    pub(crate) physical_size_mismatch:     PhysicalSizeMismatch,
    pub(crate) logical_size_mismatch:      LogicalSizeMismatch,
    pub(crate) window_mode_difference:     WindowModeDifference,
    pub(crate) monitor_difference:         MonitorDifference,
    pub(crate) scale_factor_difference:    ScaleFactorDifference,
}

#[derive(Resource, Default)]
pub(crate) struct MismatchStates {
    pub(crate) by_entity: HashMap<Entity, CachedMismatchState>,
}

#[derive(Resource, Default)]
pub(crate) struct RestoredStates {
    pub(crate) by_entity: HashMap<Entity, CachedRestoredState>,
}

pub(crate) struct CachedRestoredState {
    pub(crate) physical_position: Option<IVec2>,
    pub(crate) logical_position:  Option<IVec2>,
    pub(crate) physical_size:     UVec2,
    pub(crate) logical_size:      UVec2,
    pub(crate) monitor:           usize,
    pub(crate) window_mode:       WindowMode,
}

pub(crate) fn on_window_restored(
    trigger: On<WindowRestored>,
    mut commands: Commands,
    mut restored_states: ResMut<RestoredStates>,
    mut settled_count: ResMut<WindowsSettledCount>,
) {
    let event = trigger.event();
    info!(
        "[on_window_restored] Restore complete: window_key={} entity={:?} physical_position={:?} logical_position={:?} physical_size={} logical_size={} mode={:?} monitor={}",
        event.window_key,
        event.entity,
        event.physical_position,
        event.logical_position,
        event.physical_size,
        event.logical_size,
        event.window_mode,
        event.monitor_index
    );

    restored_states.by_entity.insert(
        event.entity,
        CachedRestoredState {
            physical_position: event.physical_position,
            logical_position:  event.logical_position,
            physical_size:     event.physical_size,
            logical_size:      event.logical_size,
            monitor:           event.monitor_index,
            window_mode:       event.window_mode,
        },
    );

    commands.insert_resource(WindowRestoredReceived {
        physical_position: event.physical_position,
        physical_size:     event.physical_size,
        window_mode:       event.window_mode,
        monitor:           event.monitor_index,
    });
    settled_count.value += 1;
}

pub(crate) fn on_window_restore_mismatch(
    trigger: On<WindowRestoreMismatch>,
    mut commands: Commands,
    mut restored_states: ResMut<RestoredStates>,
    mut mismatch_states: ResMut<MismatchStates>,
    mut settled_count: ResMut<WindowsSettledCount>,
) {
    let event = trigger.event();
    warn!(
        "[on_window_restore_mismatch] window_key={} entity={:?} \
         monitor: {} vs {}, size: {} vs {}, mode: {:?} vs {:?}",
        event.window_key,
        event.entity,
        event.expected_monitor,
        event.actual_monitor,
        event.expected_physical_size,
        event.actual_physical_size,
        event.expected_window_mode,
        event.actual_window_mode,
    );

    restored_states.by_entity.insert(
        event.entity,
        CachedRestoredState {
            physical_position: event.expected_physical_position,
            logical_position:  event.expected_logical_position,
            physical_size:     event.expected_physical_size,
            logical_size:      event.expected_logical_size,
            monitor:           event.expected_monitor,
            window_mode:       event.expected_window_mode,
        },
    );

    mismatch_states.by_entity.insert(
        event.entity,
        CachedMismatchState {
            physical_position_mismatch: PhysicalPositionMismatch {
                expected_physical_position: event.expected_physical_position,
                actual_physical_position:   event.actual_physical_position,
            },
            logical_position_mismatch:  LogicalPositionMismatch {
                expected_logical_position: event.expected_logical_position,
                actual_logical_position:   event.actual_logical_position,
            },
            physical_size_mismatch:     PhysicalSizeMismatch {
                expected_physical_size: event.expected_physical_size,
                actual_physical_size:   event.actual_physical_size,
            },
            logical_size_mismatch:      LogicalSizeMismatch {
                expected_logical_size: event.expected_logical_size,
                actual_logical_size:   event.actual_logical_size,
            },
            window_mode_difference:     WindowModeDifference {
                expected: event.expected_window_mode,
                actual:   event.actual_window_mode,
            },
            monitor_difference:         MonitorDifference {
                expected: event.expected_monitor,
                actual:   event.actual_monitor,
            },
            scale_factor_difference:    ScaleFactorDifference {
                expected: event.expected_scale,
                actual:   event.actual_scale,
            },
        },
    );

    commands.insert_resource(WindowRestoreMismatchReceived {
        monitor_difference:     MonitorDifference {
            expected: event.expected_monitor,
            actual:   event.actual_monitor,
        },
        physical_size_mismatch: PhysicalSizeMismatch {
            expected_physical_size: event.expected_physical_size,
            actual_physical_size:   event.actual_physical_size,
        },
        window_mode_difference: WindowModeDifference {
            expected: event.expected_window_mode,
            actual:   event.actual_window_mode,
        },
    });
    settled_count.value += 1;
}
