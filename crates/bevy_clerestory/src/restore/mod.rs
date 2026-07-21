//! Window restore startup, target state, and settle verification.

mod restore_attempt;
mod settle_state;
mod target_position;
mod winit_info;

use bevy::prelude::*;
pub(crate) use restore_attempt::NativeWindowReady;
pub(crate) use restore_attempt::RestorePreparation;
use restore_attempt::mark_native_window_ready;
pub(crate) use restore_attempt::prepare_restore_targets;
pub(crate) use settle_state::check_restore_settling;
pub(crate) use target_position::FullscreenRestoreState;
pub(crate) use target_position::MonitorScaleStrategy;
#[cfg(any(test, all(target_os = "linux", feature = "workaround-winit-4445")))]
pub(crate) use target_position::TargetPosition;
pub(crate) use target_position::WindowRestoreState;
pub(crate) use target_position::has_restoring_windows;
pub(crate) use target_position::restore_windows;
#[cfg(test)]
pub(crate) use winit_info::WinitInfo;
#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
pub(crate) use winit_info::X11FrameCompensated;
pub(crate) use winit_info::init_winit_info;
pub(crate) use winit_info::move_to_target_monitor;
pub(crate) use winit_info::queue_primary_restore;

use crate::ClerestoryPreStartupSet;
use crate::monitors;

pub(crate) struct RestorePlugin;

impl Plugin for RestorePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(mark_native_window_ready);

        // X11 fullscreen: move window to target monitor before first event loop.
        // Must be chained (not `.after()`) so `apply_deferred` runs between
        // `prepare_restore_targets` and `move_to_target_monitor` — otherwise the
        // `TargetPosition` component inserted via deferred commands won't exist yet.
        // `move_to_target_monitor` self-guards on `platform.is_x11()`.
        app.add_systems(
            PreStartup,
            (
                init_winit_info,
                queue_primary_restore,
                prepare_restore_targets,
                move_to_target_monitor,
            )
                .chain()
                .after(ClerestoryPreStartupSet::PersistenceLoaded),
        );

        app.add_systems(
            Update,
            prepare_restore_targets.after(monitors::update_current_monitor),
        );

        app.add_systems(
            Update,
            (
                restore_windows.after(prepare_restore_targets),
                check_restore_settling.after(restore_windows),
            )
                .run_if(has_restoring_windows),
        );
    }
}
