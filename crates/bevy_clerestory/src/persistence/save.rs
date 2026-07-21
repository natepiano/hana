//! Dirty-batch capture, projection, and persistence writes.

use std::collections::HashMap;
use std::env::current_exe;
use std::fs::create_dir_all;
use std::fs::write;
use std::path::Path;

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
#[cfg(any(
    target_os = "macos",
    all(target_os = "linux", feature = "workaround-winit-4443")
))]
use bevy::winit::WINIT_WINDOWS;

use super::CapturedWindowPlacement;
use super::CapturedWindowStates;
use super::PersistedWindowState;
use super::WindowKey;
use super::format;
use crate::ManagedWindow;
use crate::Platform;
use crate::WindowRestored;
use crate::monitors::CurrentMonitor;
use crate::restore_window_config::RestoreWindowConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StateFileWrite {
    Written,
    Failed,
}

#[cfg(test)]
#[derive(Default, Resource)]
pub(crate) struct InjectedWindowPositions {
    positions:   HashMap<Entity, Option<IVec2>>,
    pub lookups: usize,
}

#[cfg(test)]
impl InjectedWindowPositions {
    pub(crate) fn set(&mut self, entity: Entity, position: Option<IVec2>) {
        self.positions.insert(entity, position);
    }

    pub(crate) const fn reset_activity(&mut self) { self.lookups = 0; }

    fn get(&mut self, entity: Entity) -> Option<IVec2> {
        self.lookups += 1;
        self.positions.get(&entity).copied().flatten()
    }
}

pub(super) fn save_all_states(
    path: &Path,
    states: &HashMap<WindowKey, PersistedWindowState>,
) -> StateFileWrite {
    if let Some(parent) = path.parent()
        && let Err(error) = create_dir_all(parent)
    {
        warn!("[save_all_states] Failed to create directory {parent:?}: {error}");
        return StateFileWrite::Failed;
    }
    let contents = match format::encode(states) {
        Ok(contents) => contents,
        Err(error) => {
            warn!("[save_all_states] Failed to serialize state: {error}");
            return StateFileWrite::Failed;
        },
    };
    if let Err(error) = write(path, &contents) {
        warn!("[save_all_states] Failed to write state file {path:?}: {error}");
        StateFileWrite::Failed
    } else {
        StateFileWrite::Written
    }
}

/// Capture changed primary and managed windows from their installed [`CurrentMonitor`].
pub(super) fn capture_changed_windows(
    windows: Query<
        (
            Entity,
            &Window,
            &CurrentMonitor,
            Option<&ManagedWindow>,
            Has<PrimaryWindow>,
        ),
        (
            Or<(With<PrimaryWindow>, With<ManagedWindow>)>,
            Or<(Changed<Window>, Changed<CurrentMonitor>)>,
        ),
    >,
    platform: Res<Platform>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
    #[cfg(test)] mut injected_positions: Option<ResMut<InjectedWindowPositions>>,
    _: NonSendMarker,
) {
    for (entity, window, current_monitor, managed_window, primary) in &windows {
        #[cfg(test)]
        captured_window_states.record_window_scan();
        let window_key = if primary {
            WindowKey::Primary
        } else if let Some(managed_window) = managed_window {
            WindowKey::Managed(managed_window.name.clone())
        } else {
            continue;
        };
        let physical_position = capture_window_position(
            entity,
            window,
            *platform,
            #[cfg(test)]
            injected_positions.as_deref_mut(),
        );
        let placement =
            CapturedWindowPlacement::capture(window, current_monitor, physical_position, *platform);
        captured_window_states.capture(window_key, entity, placement);
    }
}

/// Promote a successfully restored adapter entry to a live captured placement.
pub(super) fn on_window_restored(
    restored: On<WindowRestored>,
    windows: Query<(&Window, &CurrentMonitor)>,
    platform: Res<Platform>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
    #[cfg(test)] mut injected_positions: Option<ResMut<InjectedWindowPositions>>,
) {
    let Ok((window, current_monitor)) = windows.get(restored.entity) else {
        return;
    };
    let physical_position = capture_window_position(
        restored.entity,
        window,
        *platform,
        #[cfg(test)]
        injected_positions.as_deref_mut(),
    );
    let placement =
        CapturedWindowPlacement::capture(window, current_monitor, physical_position, *platform);
    captured_window_states.promote(restored.window_key.clone(), restored.entity, placement);
}

/// Project and write one whole-map batch when captured state is dirty.
pub(crate) fn write_dirty_window_states(
    config: Res<RestoreWindowConfig>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
) {
    if !captured_window_states.is_dirty() {
        return;
    }

    #[cfg(test)]
    captured_window_states.record_projection();
    let states = captured_window_states.project(&application_name());
    #[cfg(test)]
    captured_window_states.record_write();
    save_all_states(&config.path, &states);
    captured_window_states.mark_clean();
}

pub(super) fn application_name() -> String {
    current_exe()
        .ok()
        .and_then(|executable_path| {
            executable_path
                .file_stem()
                .and_then(|file_stem| file_stem.to_str())
                .map(String::from)
        })
        .unwrap_or_default()
}

fn capture_window_position(
    entity: Entity,
    window: &Window,
    platform: Platform,
    #[cfg(test)] injected_positions: Option<&mut InjectedWindowPositions>,
) -> Option<IVec2> {
    #[cfg(test)]
    if let Some(injected_positions) = injected_positions {
        return injected_positions.get(entity);
    }
    get_window_position(entity, window, platform)
}

/// Get the window position without querying or refreshing monitor metadata.
#[cfg(any(
    target_os = "macos",
    all(target_os = "linux", feature = "workaround-winit-4443")
))]
pub(super) fn get_window_position(entity: Entity, _: &Window, platform: Platform) -> Option<IVec2> {
    if !platform.position_available() {
        return None;
    }
    WINIT_WINDOWS.with(|winit_windows| {
        let winit_windows = winit_windows.borrow();
        let winit_window = winit_windows.get_window(entity)?;
        let physical_outer_position = winit_window.outer_position().ok()?;
        Some(IVec2::new(
            physical_outer_position.x,
            physical_outer_position.y,
        ))
    })
}

#[cfg(not(any(
    target_os = "macos",
    all(target_os = "linux", feature = "workaround-winit-4443")
)))]
pub(super) const fn get_window_position(
    _: Entity,
    window: &Window,
    platform: Platform,
) -> Option<IVec2> {
    if !platform.position_available() {
        return None;
    }
    match window.position {
        WindowPosition::At(position) => Some(position),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::panic, reason = "tests should panic on unexpected values")]
mod tests {
    use tempfile::NamedTempFile;
    use tempfile::tempdir;

    use super::*;
    use crate::monitors::MonitorIdentity;
    use crate::monitors::MonitorInfo;
    use crate::persistence::CapturedWindowPosition;
    use crate::persistence::SavedWindowMode;

    fn placement() -> CapturedWindowPlacement {
        CapturedWindowPlacement {
            monitor_snapshot:  MonitorInfo {
                identity:          MonitorIdentity::Unverified,
                index:             0,
                scale:             1.0,
                physical_position: IVec2::ZERO,
                physical_size:     UVec2::new(1_920, 1_080),
            },
            position:          CapturedWindowPosition::Restorable {
                logical_offset: IVec2::new(40, 60),
            },
            logical_size:      UVec2::new(800, 600),
            saved_window_mode: SavedWindowMode::Windowed,
            captured_scale:    1.0,
        }
    }

    #[test]
    fn dirty_batch_projects_and_writes_once() {
        let file = NamedTempFile::new();
        assert!(file.is_ok(), "temporary state file should be available");
        let file = file.unwrap_or_else(|error| panic!("failed to create state file: {error}"));
        let mut world = World::new();
        let mut states = CapturedWindowStates::default();
        states.capture(WindowKey::Primary, Entity::PLACEHOLDER, placement());
        states.reset_activity();
        world.insert_resource(states);
        world.insert_resource(RestoreWindowConfig {
            path: file.path().to_path_buf(),
        });
        let mut schedule = Schedule::default();
        schedule.add_systems(write_dirty_window_states);

        schedule.run(&mut world);
        schedule.run(&mut world);

        let states = world.resource::<CapturedWindowStates>();
        assert_eq!(states.activity().projections, 1);
        assert_eq!(states.activity().writes, 1);
        assert!(!states.is_dirty());
    }

    #[test]
    fn failed_write_consumes_batch_until_another_mutation() {
        let directory = tempdir();
        assert!(directory.is_ok(), "temporary directory should be available");
        let directory = directory
            .unwrap_or_else(|error| panic!("failed to create temporary directory: {error}"));
        let mut world = World::new();
        let mut states = CapturedWindowStates::default();
        states.capture(WindowKey::Primary, Entity::PLACEHOLDER, placement());
        states.reset_activity();
        world.insert_resource(states);
        world.insert_resource(RestoreWindowConfig {
            path: directory.path().to_path_buf(),
        });
        let mut schedule = Schedule::default();
        schedule.add_systems(write_dirty_window_states);

        schedule.run(&mut world);
        schedule.run(&mut world);

        let states = world.resource::<CapturedWindowStates>();
        assert_eq!(states.activity().projections, 1);
        assert_eq!(states.activity().writes, 1);
        assert!(!states.is_dirty());

        let mut changed = placement();
        changed.logical_size.x += 1;
        world.resource_mut::<CapturedWindowStates>().capture(
            WindowKey::Primary,
            Entity::PLACEHOLDER,
            changed,
        );
        schedule.run(&mut world);

        let states = world.resource::<CapturedWindowStates>();
        assert_eq!(states.activity().projections, 2);
        assert_eq!(states.activity().writes, 2);
    }
}
