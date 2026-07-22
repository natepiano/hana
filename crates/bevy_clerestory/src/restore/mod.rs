//! Window restore startup, target state, and settle verification.

mod restore_attempt;
mod settle_state;
mod target_position;
mod winit_info;

use bevy::prelude::*;
use bevy::time::Virtual;
pub(crate) use restore_attempt::NativeWindowReady;
use restore_attempt::RestoreAttemptIds;
pub(crate) use restore_attempt::RestoreDisposition;
pub(crate) use restore_attempt::RestorePreparation;
pub(crate) use restore_attempt::cancel_restore;
use restore_attempt::clear_native_window_ready;
pub(crate) use restore_attempt::mark_native_window_ready;
pub(crate) use restore_attempt::prepare_restore_targets;
pub(crate) use restore_attempt::reconcile_runtime_restore_attempts;
pub(crate) use settle_state::check_restore_settling;
pub(crate) use target_position::FullscreenRestoreState;
pub(crate) use target_position::MonitorScaleStrategy;
use target_position::ObservedScaleInputs;
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
use crate::ClerestoryUpdateSet;
use crate::monitors;
use crate::recovery;
pub(crate) struct RestorePlugin;

impl Plugin for RestorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RestoreAttemptIds>()
            .init_resource::<ObservedScaleInputs>()
            .init_resource::<Time<Virtual>>()
            .add_observer(mark_native_window_ready)
            .add_observer(clear_native_window_ready)
            .add_observer(restore_attempt::validate_runtime_restore_completion);

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
            (
                // Stamp `WindowScaleFactorChanged` with the current `RestoreAttemptId`
                // before `RecoveryTopology` can replace `RestorePreparation`.
                target_position::capture_scale_inputs.in_set(ClerestoryUpdateSet::MonitorTopology),
                (
                    restore_attempt::accept_explicit_restore_requests,
                    restore_attempt::accept_automatic_restore_intents,
                    ApplyDeferred,
                )
                    .chain()
                    .after(recovery::accept_eligible_registrations)
                    .after(recovery::advance_fallback_windows)
                    .in_set(ClerestoryUpdateSet::RecoveryWindow),
                (
                    restore_attempt::timeout_runtime_restore_attempts,
                    ApplyDeferred,
                    restore_attempt::reject_stale_restore_attempts,
                    ApplyDeferred,
                    prepare_restore_targets,
                    ApplyDeferred,
                )
                    .chain()
                    .after(monitors::update_current_monitor)
                    .in_set(ClerestoryUpdateSet::RestorePreparation),
            ),
        );

        app.add_systems(
            Update,
            (
                restore_windows
                    .after(prepare_restore_targets)
                    .in_set(ClerestoryUpdateSet::RestoreApplication),
                check_restore_settling
                    .after(restore_windows)
                    .in_set(ClerestoryUpdateSet::RestoreSettling),
                ApplyDeferred
                    .after(check_restore_settling)
                    .in_set(ClerestoryUpdateSet::RestoreSettling),
            )
                .run_if(has_restoring_windows),
        );
    }
}
