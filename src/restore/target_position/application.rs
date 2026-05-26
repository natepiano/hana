use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::MonitorSelection;
use bevy::window::WindowMode;
use bevy::window::WindowPosition;
use bevy::window::WindowScaleFactorChanged;
use bevy::winit::WINIT_WINDOWS;
use bevy_kana::ToI32;
use bevy_kana::ToU32;

use super::strategy::FullscreenRestoreState;
use super::strategy::MonitorScaleStrategy;
use super::strategy::WindowRestoreState;
use super::target::TargetPosition;
use crate::Platform;
use crate::constants::RESTORE_STRATEGY_APPLY_UNCHANGED;
use crate::constants::RESTORE_STRATEGY_LOWER_TO_HIGHER;
use crate::constants::SCALE_FACTOR_EPSILON;
use crate::persistence::SavedWindowMode;
use crate::restore::settle_state::SettleState;
use crate::restore::winit_info::X11FrameCompensated;

/// Apply the initial window move to the target monitor.
fn apply_initial_move(target_position: &TargetPosition, window: &mut Window) {
    if target_position.saved_window_mode.is_fullscreen() {
        if let Some(physical_position) = target_position.physical_position {
            debug!(
                "[apply_initial_move] Moving to target position {:?} for fullscreen mode {:?}",
                physical_position, target_position.saved_window_mode
            );
            window.position = WindowPosition::At(physical_position);
        } else {
            debug!(
                "[apply_initial_move] No saved position, fullscreen mode {:?} targets monitor {} via WindowMode",
                target_position.saved_window_mode, target_position.monitor_index
            );
        }
        return;
    }

    let Some(physical_position) = target_position.physical_position else {
        debug!(
            "[apply_initial_move] No saved position, centering on monitor {}",
            target_position.monitor_index
        );
        window.position =
            WindowPosition::Centered(MonitorSelection::Index(target_position.monitor_index));
        return;
    };

    // HigherToLower (macOS/X11 high→low) compensates position by ×ratio
    // (= starting_scale / target_scale, e.g. ×2 for 2x→1x): `set_outer_position` is
    // applied at the starting monitor's scale, so crossing to the half-scale target
    // halves the physical position unless pre-multiplied. Size stays a placeholder —
    // the WaitingForScaleChange → ApplySize phase re-applies the full physical size
    // after the scale change settles.
    let (physical_move_position, physical_move_size) = match target_position.monitor_scale_strategy
    {
        MonitorScaleStrategy::HigherToLower(_) => {
            let ratio = target_position.ratio();
            let physical_compensated_x = (f64::from(physical_position.x) * ratio).to_i32();
            let physical_compensated_y = (f64::from(physical_position.y) * ratio).to_i32();
            debug!(
                "[apply_initial_move] HigherToLower: compensating position {physical_position:?} -> ({physical_compensated_x}, {physical_compensated_y}) (ratio={ratio})",
            );
            (
                IVec2::new(physical_compensated_x, physical_compensated_y),
                target_position.physical_size,
            )
        },
        MonitorScaleStrategy::CompensateSizeOnly(_) => {
            let compensated_size = target_position.compensated_size();
            debug!(
                "[apply_initial_move] CompensateSizeOnly: position={:?} compensated_size={}x{} (ratio={})",
                physical_position,
                compensated_size.x,
                compensated_size.y,
                target_position.ratio()
            );
            (physical_position, compensated_size)
        },
        _ => (physical_position, target_position.physical_size),
    };

    debug!(
        "[apply_initial_move] position={physical_move_position:?} size={}x{} visible={}",
        physical_move_size.x, physical_move_size.y, window.visible
    );

    window.position = WindowPosition::At(physical_move_position);
    window
        .resolution
        .set_physical_resolution(physical_move_size.x, physical_move_size.y);
}

/// Handle the initial move for cross-DPI strategies.
///
/// With a saved position, we apply a compensated position+size on the starting monitor,
/// then transition to `WaitingForScaleChange` so winit's `WindowScaleFactorChanged`
/// triggers the final `ApplySize` phase at `target_scale`.
///
/// With no saved position, we anchor the window on the saved monitor via
/// `WindowPosition::Centered` and size at `starting_scale` (so the stored logical size
/// resolves to `target.physical_size` once the window lands on the target monitor).
/// The two-phase scale-change dance is skipped because macOS does not fire
/// `WindowScaleFactorChanged` for windows that are still hidden; waiting for it would
/// deadlock. Settle starts immediately and verifies the resulting state.
fn begin_cross_dpi_restore(target_position: &mut TargetPosition, window: &mut Window) {
    if target_position.physical_position.is_none() {
        // Size at `starting_scale`: `set_physical_resolution` is interpreted at the
        // window's current scale factor, which is `starting_scale` until the move
        // completes. Storing logical = `starting_size / starting_scale = logical_size`
        // means the post-move physical size resolves to `logical_size * target_scale`,
        // matching `target.physical_size` for settle.
        let physical_width =
            (f64::from(target_position.logical_size.x) * target_position.starting_scale).to_u32();
        let physical_height =
            (f64::from(target_position.logical_size.y) * target_position.starting_scale).to_u32();
        debug!(
            "[begin_cross_dpi_restore] no saved position, centering on monitor {} at \
             starting_scale={} (physical {}x{} → logical {}x{} after move to target_scale={})",
            target_position.monitor_index,
            target_position.starting_scale,
            physical_width,
            physical_height,
            target_position.logical_size.x,
            target_position.logical_size.y,
            target_position.target_scale
        );
        window.position =
            WindowPosition::Centered(MonitorSelection::Index(target_position.monitor_index));
        window
            .resolution
            .set_physical_resolution(physical_width, physical_height);
        window.visible = true;
        target_position.settle_state = Some(SettleState::new());
        return;
    }

    apply_initial_move(target_position, window);
    target_position.monitor_scale_strategy = match target_position.monitor_scale_strategy {
        MonitorScaleStrategy::HigherToLower(_) => {
            MonitorScaleStrategy::HigherToLower(WindowRestoreState::WaitingForScaleChange)
        },
        _ => MonitorScaleStrategy::CompensateSizeOnly(WindowRestoreState::WaitingForScaleChange),
    };
}

/// Apply pending window restore. Runs only when entities with `TargetPosition` exist.
pub(crate) fn restore_windows(
    mut scale_changed_messages: MessageReader<WindowScaleFactorChanged>,
    mut windows: Query<(Entity, &mut TargetPosition, &mut Window), With<X11FrameCompensated>>,
    _: NonSendMarker,
    platform: Res<Platform>,
) {
    let scale_changed = scale_changed_messages.read().last().is_some();

    for (entity, mut target_position, mut window) in &mut windows {
        if target_position.settle_state.is_some() {
            continue;
        }

        let winit_window_exists =
            WINIT_WINDOWS.with(|winit_windows| winit_windows.borrow().get_window(entity).is_some());
        if !winit_window_exists {
            debug!("[restore_windows] Skipping entity {entity:?}: winit window not yet created");
            continue;
        }

        if platform.needs_managed_scale_fixup() {
            let actual_scale = f64::from(window.resolution.base_scale_factor());
            if (actual_scale - target_position.starting_scale).abs() > SCALE_FACTOR_EPSILON {
                let old_monitor_scale_strategy = target_position.monitor_scale_strategy;
                target_position.starting_scale = actual_scale;
                target_position.monitor_scale_strategy =
                    platform.scale_strategy(actual_scale, target_position.target_scale);
                debug!(
                    "[restore_windows] Corrected starting_scale for entity {entity:?}: \
                     monitor_scale_strategy: {old_monitor_scale_strategy:?} -> {:?} \
                     (actual_scale={actual_scale:.2})",
                    target_position.monitor_scale_strategy
                );
            }
        }

        if matches!(
            target_position.monitor_scale_strategy,
            MonitorScaleStrategy::HigherToLower(WindowRestoreState::NeedInitialMove)
                | MonitorScaleStrategy::CompensateSizeOnly(WindowRestoreState::NeedInitialMove)
        ) {
            begin_cross_dpi_restore(&mut target_position, &mut window);
            continue;
        }

        match target_position.monitor_scale_strategy {
            MonitorScaleStrategy::HigherToLower(WindowRestoreState::WaitingForScaleChange)
                if scale_changed =>
            {
                debug!(
                    "[Restore] ScaleChanged received, transitioning to WindowRestoreState::ApplySize"
                );
                target_position.monitor_scale_strategy =
                    MonitorScaleStrategy::HigherToLower(WindowRestoreState::ApplySize);
            },
            MonitorScaleStrategy::CompensateSizeOnly(WindowRestoreState::WaitingForScaleChange) => {
                debug!(
                    "[Restore] CompensateSizeOnly: transitioning to ApplySize (scale_changed={scale_changed})"
                );
                target_position.monitor_scale_strategy =
                    MonitorScaleStrategy::CompensateSizeOnly(WindowRestoreState::ApplySize);
            },
            _ => {},
        }

        if let Some(fullscreen_restore_state) = target_position.fullscreen_restore_state {
            match fullscreen_restore_state {
                FullscreenRestoreState::MoveToMonitor => {
                    if let Some(position) = target_position.physical_position {
                        debug!("[restore_windows] Fullscreen MoveToMonitor: position={position:?}");
                        window.position = WindowPosition::At(position);
                    }
                    target_position.fullscreen_restore_state =
                        Some(FullscreenRestoreState::WaitForMove);
                    continue;
                },
                FullscreenRestoreState::WaitForMove => {
                    debug!("[restore_windows] Fullscreen WaitForMove: waiting for compositor");
                    target_position.fullscreen_restore_state =
                        Some(FullscreenRestoreState::ApplyMode);
                    continue;
                },
                FullscreenRestoreState::WaitForSurface => {
                    debug!("[restore_windows] Fullscreen WaitForSurface: waiting for GPU surface");
                    target_position.fullscreen_restore_state =
                        Some(FullscreenRestoreState::ApplyMode);
                    continue;
                },
                FullscreenRestoreState::ApplyMode => {},
            }
        }

        if matches!(
            try_apply_restore(&target_position, &mut window, *platform),
            RestoreStatus::Complete
        ) && target_position.settle_state.is_none()
        {
            info!(
                "[restore_windows] Restore applied, starting settle (200ms stability / 1s timeout)"
            );
            target_position.settle_state = Some(SettleState::new());
        }
    }
}

enum RestoreStatus {
    Complete,
    Waiting,
}

fn apply_window_geometry(
    window: &mut Window,
    physical_position: Option<IVec2>,
    physical_size: UVec2,
    strategy: &str,
    ratio: Option<f64>,
    monitor_index: usize,
) {
    if let Some(physical_position) = physical_position {
        if let Some(ratio) = ratio {
            debug!(
                "[try_apply_restore] position={:?} size={}x{} ({strategy}, ratio={ratio})",
                physical_position, physical_size.x, physical_size.y
            );
        } else {
            debug!(
                "[try_apply_restore] position={:?} size={}x{} ({strategy})",
                physical_position, physical_size.x, physical_size.y
            );
        }
        window.position = WindowPosition::At(physical_position);
    } else {
        if let Some(ratio) = ratio {
            debug!(
                "[try_apply_restore] size={}x{} centered on monitor {monitor_index} ({strategy}, ratio={ratio}, no saved position)",
                physical_size.x, physical_size.y
            );
        } else {
            debug!(
                "[try_apply_restore] size={}x{} centered on monitor {monitor_index} ({strategy}, no saved position)",
                physical_size.x, physical_size.y
            );
        }
        window.position = WindowPosition::Centered(MonitorSelection::Index(monitor_index));
    }
    window
        .resolution
        .set_physical_resolution(physical_size.x, physical_size.y);
}

fn apply_fullscreen_restore(
    target_position: &TargetPosition,
    window: &mut Window,
    platform: Platform,
) {
    let monitor_index = target_position.monitor_index;

    let window_mode = if platform.exclusive_fullscreen_fallback()
        && matches!(
            target_position.saved_window_mode,
            SavedWindowMode::Fullscreen { .. }
        ) {
        warn!(
            "Exclusive fullscreen is not supported on Wayland, restoring as BorderlessFullscreen"
        );
        WindowMode::BorderlessFullscreen(MonitorSelection::Index(monitor_index))
    } else {
        target_position
            .saved_window_mode
            .to_window_mode(monitor_index)
    };

    debug!(
        "[Restore] Applying fullscreen mode {:?} on monitor {} -> WindowMode::{:?}",
        target_position.saved_window_mode, monitor_index, window_mode
    );
    debug!(
        "[Restore] Current window state: position={:?} mode={:?}",
        window.position, window.mode
    );

    window.mode = window_mode;
}

fn try_apply_restore(
    target_position: &TargetPosition,
    window: &mut Window,
    platform: Platform,
) -> RestoreStatus {
    if target_position.saved_window_mode.is_fullscreen() {
        debug!(
            "[try_apply_restore] fullscreen: mode={:?} target_monitor={} current_physical={}x{} current_mode={:?} current_position={:?}",
            target_position.saved_window_mode,
            target_position.monitor_index,
            window.physical_width(),
            window.physical_height(),
            window.mode,
            window.position,
        );
        apply_fullscreen_restore(target_position, window, platform);
        window.visible = true;
        return RestoreStatus::Complete;
    }

    debug!(
        "[Restore] target_position={:?} target_scale={} monitor_scale_strategy={:?}",
        target_position.physical_position,
        target_position.target_scale,
        target_position.monitor_scale_strategy
    );

    match target_position.monitor_scale_strategy {
        MonitorScaleStrategy::ApplyUnchanged => {
            apply_window_geometry(
                window,
                target_position.physical_position,
                target_position.physical_size,
                RESTORE_STRATEGY_APPLY_UNCHANGED,
                None,
                target_position.monitor_index,
            );
        },
        MonitorScaleStrategy::CompensateSizeOnly(WindowRestoreState::ApplySize) => {
            debug!(
                "[try_apply_restore] size={}x{} ONLY (CompensateSizeOnly::ApplySize, position already set)",
                target_position.physical_size.x, target_position.physical_size.y
            );
            window.resolution.set_physical_resolution(
                target_position.physical_size.x,
                target_position.physical_size.y,
            );
        },
        MonitorScaleStrategy::CompensateSizeOnly(
            WindowRestoreState::NeedInitialMove | WindowRestoreState::WaitingForScaleChange,
        ) => {
            debug!(
                "[Restore] CompensateSizeOnly: waiting for initial move or ScaleChanged message"
            );
            return RestoreStatus::Waiting;
        },
        MonitorScaleStrategy::LowerToHigher => {
            // Position still needs ratio compensation: on a low→high cross-scale
            // move, `set_outer_position` is applied at the starting monitor's scale,
            // so the move doubles it. Size must NOT be compensated: as of bevy 0.19,
            // `request_inner_size` resolves at the target monitor's scale, so the
            // full physical size lands correctly (compensating it would halve it).
            apply_window_geometry(
                window,
                target_position.compensated_position(),
                target_position.physical_size,
                RESTORE_STRATEGY_LOWER_TO_HIGHER,
                Some(target_position.ratio()),
                target_position.monitor_index,
            );
        },
        MonitorScaleStrategy::HigherToLower(WindowRestoreState::ApplySize) => {
            debug!(
                "[try_apply_restore] size={}x{} ONLY (HigherToLower::ApplySize, position already set)",
                target_position.physical_size.x, target_position.physical_size.y
            );
            window.resolution.set_physical_resolution(
                target_position.physical_size.x,
                target_position.physical_size.y,
            );
        },
        MonitorScaleStrategy::HigherToLower(
            WindowRestoreState::NeedInitialMove | WindowRestoreState::WaitingForScaleChange,
        ) => {
            debug!("[Restore] HigherToLower: waiting for initial move or ScaleChanged message");
            return RestoreStatus::Waiting;
        },
    }

    window.visible = true;
    RestoreStatus::Complete
}
