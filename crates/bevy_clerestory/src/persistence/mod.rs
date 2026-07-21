//! Window state persistence authority, serialization adapter, and I/O.

mod captured_window_state;
mod constants;
mod format;
mod load;
mod save;
mod window_state;

use bevy::prelude::*;
pub(crate) use captured_window_state::CapturedPlacement;
pub(crate) use captured_window_state::CapturedWindowPlacement;
pub(crate) use captured_window_state::CapturedWindowPosition;
pub(crate) use captured_window_state::CapturedWindowStates;
#[cfg(test)]
pub(crate) use captured_window_state::PersistenceWriteState;
pub(crate) use captured_window_state::RebasedCapturedPosition;
#[cfg(test)]
pub(crate) use captured_window_state::on_primary_window_removed;
#[cfg(test)]
pub(crate) use captured_window_state::on_window_removed;
pub use format::WindowKey;
pub(crate) use load::get_default_state_path;
pub(crate) use load::get_state_path_for_app;
#[cfg(test)]
pub(crate) use save::InjectedWindowPositions;
pub(crate) use window_state::PersistedWindowState;
pub(crate) use window_state::SavedWindowMode;

use crate::ClerestoryPreStartupSet;
use crate::ClerestoryUpdateSet;
use crate::managed::on_persistence_changed;
use crate::monitors;
use crate::restore;

pub(crate) struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CapturedWindowStates>()
            .add_observer(captured_window_state::on_primary_window_removed)
            .add_observer(captured_window_state::on_window_removed)
            .add_observer(save::on_window_restored)
            .add_systems(
                PreStartup,
                load::load_captured_window_states
                    .in_set(ClerestoryPreStartupSet::PersistenceLoaded),
            )
            .add_systems(
                Update,
                (
                    on_persistence_changed
                        .run_if(resource_changed::<crate::ManagedWindowPersistence>),
                    save::capture_changed_windows
                        .after(monitors::update_current_monitor)
                        .after(restore::check_restore_settling),
                    captured_window_state::finish_capture_suppression,
                    save::write_dirty_window_states,
                )
                    .chain()
                    .in_set(ClerestoryUpdateSet::Persistence),
            );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use bevy::window::WindowMode;
    use tempfile::NamedTempFile;

    use super::*;
    use crate::ManagedWindowPersistence;
    use crate::Platform;
    use crate::monitors::MonitorIdentity;
    use crate::monitors::MonitorInfo;
    use crate::restore_window_config::RestoreWindowConfig;

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
            saved_window_mode: SavedWindowMode::from(&WindowMode::Windowed),
            captured_scale:    1.0,
        }
    }

    #[test]
    fn persistence_policy_change_precedes_the_same_update_write() {
        let file = NamedTempFile::new();
        assert!(file.is_ok(), "temporary state file should be available");
        let Ok(file) = file else {
            return;
        };
        let key = WindowKey::Managed("inactive".to_string());
        let entity = Entity::PLACEHOLDER;
        let mut states = CapturedWindowStates::default();
        states.seed(HashMap::new());
        states.capture(key.clone(), entity, placement());
        states.deactivate(&key, entity, &ManagedWindowPersistence::RememberAll);
        states.reset_activity();

        let mut app = App::new();
        app.insert_resource(ManagedWindowPersistence::ActiveOnly)
            .insert_resource(Platform::Windows)
            .insert_resource(RestoreWindowConfig {
                path: file.path().to_path_buf(),
            })
            .insert_resource(states)
            .add_plugins(PersistencePlugin);

        app.update();

        let states = app.world().resource::<CapturedWindowStates>();
        assert!(states.entry(&key).is_none());
        assert_eq!(states.activity().projections, 1);
        assert_eq!(states.activity().writes, 1);
        let contents = fs::read_to_string(file.path());
        assert!(contents.is_ok(), "written state file should be readable");
        let Ok(contents) = contents else {
            return;
        };
        let persisted = format::decode(&contents);
        assert!(persisted.is_some());
        assert!(persisted.is_some_and(|persisted| persisted.is_empty()));
    }
}
