//! Window restore startup, target state, and settle verification.

mod settle_state;
mod target_position;
mod winit_info;

use bevy::prelude::*;
pub(crate) use settle_state::check_restore_settling;
pub(crate) use target_position::FullscreenRestoreState;
pub(crate) use target_position::MonitorResolutionSource;
pub(crate) use target_position::MonitorScaleStrategy;
#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
pub(crate) use target_position::TargetPosition;
pub(crate) use target_position::WindowRestoreState;
pub(crate) use target_position::compute_target_position;
pub(crate) use target_position::has_restoring_windows;
pub(crate) use target_position::no_restoring_windows;
pub(crate) use target_position::resolve_target_monitor_and_position;
pub(crate) use target_position::restore_windows;
pub(crate) use winit_info::WinitInfo;
pub(crate) use winit_info::X11FrameCompensated;
pub(crate) use winit_info::init_winit_info;
pub(crate) use winit_info::load_target_position;
pub(crate) use winit_info::move_to_target_monitor;

use crate::monitors;

pub(crate) struct RestorePlugin;

impl Plugin for RestorePlugin {
    fn build(&self, app: &mut App) {
        // X11 fullscreen: move window to target monitor before first event loop.
        // Must be chained (not `.after()`) so `apply_deferred` runs between
        // `load_target_position` and `move_to_target_monitor` — otherwise the
        // `TargetPosition` component inserted via deferred commands won't exist yet.
        // `move_to_target_monitor` self-guards on `platform.is_x11()`.
        app.add_systems(
            PreStartup,
            (
                init_winit_info,
                load_target_position,
                move_to_target_monitor,
            )
                .chain()
                .after(monitors::init_monitors),
        );

        app.add_systems(
            Update,
            (
                restore_windows,
                check_restore_settling.after(restore_windows),
            )
                .run_if(has_restoring_windows),
        );
    }
}
