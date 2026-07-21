//! Window state persistence authority, serialization adapter, and I/O.

mod captured_window_state;
mod constants;
mod format;
mod load;
mod save;
mod window_state;

use bevy::prelude::*;
pub(crate) use captured_window_state::CapturedWindowPlacement;
#[cfg(test)]
pub(crate) use captured_window_state::CapturedWindowPosition;
pub(crate) use captured_window_state::CapturedWindowStates;
pub use format::WindowKey;
pub(crate) use load::get_default_state_path;
pub(crate) use load::get_state_path_for_app;
#[cfg(test)]
pub(crate) use save::InjectedWindowPositions;
pub(crate) use save::write_dirty_window_states;
pub(crate) use window_state::PersistedWindowState;
pub(crate) use window_state::SavedWindowMode;

use crate::ClerestoryPreStartupSet;
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
                    save::capture_changed_windows
                        .after(monitors::update_current_monitor)
                        .after(restore::check_restore_settling),
                    save::write_dirty_window_states,
                )
                    .chain(),
            );
    }
}
