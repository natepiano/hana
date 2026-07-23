use std::collections::HashMap;
#[cfg(test)]
use std::collections::HashSet;

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
use super::strategy::NativeFullscreenState;
use super::strategy::WindowRestoreState;
use super::target::TargetPosition;
use crate::Platform;
use crate::constants::MILLIS_PER_SECOND;
use crate::constants::RESTORE_STRATEGY_APPLY_UNCHANGED;
use crate::constants::RESTORE_STRATEGY_LOWER_TO_HIGHER;
use crate::constants::SCALE_FACTOR_EPSILON;
use crate::constants::SETTLE_STABILITY_SECS;
use crate::constants::SETTLE_TIMEOUT_SECS;
use crate::macos_tabbing_fix;
use crate::macos_tabbing_fix::NativeFullscreenObservations;
use crate::monitors::CurrentMonitor;
use crate::monitors::MonitorTopologyRevision;
use crate::monitors::Monitors;
use crate::persistence::SavedWindowMode;
use crate::recovery::RecoveryRegistrations;
use crate::restore::RestorePreparation;
use crate::restore::restore_attempt;
use crate::restore::restore_attempt::RestoreAttemptId;
use crate::restore::restore_attempt::RestoreAttemptStatus;
use crate::restore::settle_state::SettleState;
use crate::restore::winit_info::X11FrameCompensated;

enum RestoreStatus {
    Complete,
    Waiting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScaleInputProvenance {
    Startup,
    Recovery(RestoreAttemptId),
}

impl From<&RestorePreparation> for ScaleInputProvenance {
    fn from(preparation: &RestorePreparation) -> Self {
        preparation
            .attempt_id()
            .map_or(Self::Startup, Self::Recovery)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ObservedScaleInput {
    provenance: ScaleInputProvenance,
    scale:      f64,
}

#[derive(Default, Resource)]
pub(crate) struct ObservedScaleInputs {
    entries: HashMap<Entity, Vec<ObservedScaleInput>>,
}

pub(crate) fn capture_scale_inputs(
    mut messages: MessageReader<WindowScaleFactorChanged>,
    preparations: Query<&RestorePreparation>,
    mut inputs: ResMut<ObservedScaleInputs>,
) {
    inputs.entries.clear();
    for message in messages.read() {
        let Ok(preparation) = preparations.get(message.window) else {
            continue;
        };
        inputs
            .entries
            .entry(message.window)
            .or_default()
            .push(ObservedScaleInput {
                provenance: preparation.into(),
                scale:      message.scale_factor,
            });
    }
}

#[cfg(test)]
#[derive(Default, Resource)]
pub(crate) struct InjectedWinitWindows {
    entities: HashSet<Entity>,
}

#[cfg(test)]
impl InjectedWinitWindows {
    fn contains(&self, entity: Entity) -> bool { self.entities.contains(&entity) }
}

#[cfg(test)]
impl Extend<Entity> for InjectedWinitWindows {
    fn extend<T: IntoIterator<Item = Entity>>(&mut self, entities: T) {
        self.entities.extend(entities);
    }
}

#[cfg(test)]
fn native_window_exists(entity: Entity, injected_windows: Option<&InjectedWinitWindows>) -> bool {
    injected_windows.is_some_and(|windows| windows.contains(entity))
        || WINIT_WINDOWS.with(|winit_windows| winit_windows.borrow().get_window(entity).is_some())
}

#[cfg(not(test))]
fn native_window_exists(entity: Entity) -> bool {
    WINIT_WINDOWS.with(|winit_windows| winit_windows.borrow().get_window(entity).is_some())
}

fn matching_scale_change(
    entity: Entity,
    current_attempt: Option<RestoreAttemptId>,
    transition_attempt: Option<RestoreAttemptId>,
    target_scale: f64,
    live_scale: f64,
    scale_inputs: &ObservedScaleInputs,
) -> bool {
    let current_provenance = current_attempt.map_or(
        ScaleInputProvenance::Startup,
        ScaleInputProvenance::Recovery,
    );
    let transition_provenance = transition_attempt.map_or(
        ScaleInputProvenance::Startup,
        ScaleInputProvenance::Recovery,
    );
    current_provenance == transition_provenance
        && scale_inputs.entries.get(&entity).is_some_and(|inputs| {
            inputs.iter().any(|input| {
                input.provenance == current_provenance
                    && (input.scale - target_scale).abs() <= SCALE_FACTOR_EPSILON
            }) && (live_scale - target_scale).abs() <= SCALE_FACTOR_EPSILON
        })
}

fn correct_initial_starting_scale(
    entity: Entity,
    target_position: &mut TargetPosition,
    window: &Window,
    platform: Platform,
) {
    if !platform.needs_managed_scale_fixup()
        || !matches!(
            target_position.monitor_scale_strategy,
            MonitorScaleStrategy::ApplyUnchanged
                | MonitorScaleStrategy::LowerToHigher
                | MonitorScaleStrategy::HigherToLower(WindowRestoreState::NeedInitialMove)
                | MonitorScaleStrategy::CompensateSizeOnly(WindowRestoreState::NeedInitialMove)
        )
    {
        return;
    }

    let actual_scale = f64::from(window.resolution.base_scale_factor());
    if (actual_scale - target_position.starting_scale).abs() <= SCALE_FACTOR_EPSILON {
        return;
    }

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
            let physical_compensated_size = target_position.compensated_size();
            debug!(
                "[apply_initial_move] CompensateSizeOnly: position={:?} compensated_size={}x{} (ratio={})",
                physical_position,
                physical_compensated_size.x,
                physical_compensated_size.y,
                target_position.ratio()
            );
            (physical_position, physical_compensated_size)
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
/// resolves to `target.physical_size` once `WindowPosition::Centered` selects the target monitor).
/// The `WindowScaleFactorChanged` -> `WindowRestoreState::ApplySize` transition is
/// skipped because macOS does not fire `WindowScaleFactorChanged` for windows that are
/// still hidden; waiting for it would deadlock. Settle starts immediately and verifies
/// the resulting state.
fn begin_cross_dpi_restore(
    target_position: &mut TargetPosition,
    window: &mut Window,
    attempt_id: Option<RestoreAttemptId>,
) {
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
            MonitorScaleStrategy::HigherToLower(WindowRestoreState::WaitingForScaleChange {
                attempt_id,
            })
        },
        _ => MonitorScaleStrategy::CompensateSizeOnly(WindowRestoreState::WaitingForScaleChange {
            attempt_id,
        }),
    };
}

fn advance_fullscreen_restore(
    #[cfg(target_os = "macos")] entity: Entity,
    #[cfg(not(target_os = "macos"))] _entity: Entity,
    target_position: &mut TargetPosition,
    window: &mut Window,
    current_monitor: Option<&CurrentMonitor>,
    native_fullscreen: NativeFullscreenState,
) -> RestoreStatus {
    let Some(fullscreen_restore_state) = target_position.fullscreen_restore_state else {
        return RestoreStatus::Complete;
    };
    match fullscreen_restore_state {
        FullscreenRestoreState::LeaveFullscreen => {
            debug!("[restore_windows] macOS fullscreen: leaving the current fullscreen Space");
            window.mode = WindowMode::Windowed;
            target_position.fullscreen_restore_state =
                Some(FullscreenRestoreState::MoveWindowedToTarget);
            RestoreStatus::Waiting
        },
        FullscreenRestoreState::MoveWindowedToTarget => {
            if native_fullscreen != NativeFullscreenState::Windowed {
                debug!(
                    "[restore_windows] macOS fullscreen: waiting for AppKit to finish leaving fullscreen"
                );
                return RestoreStatus::Waiting;
            }
            let target_monitor_reached = current_monitor.is_some_and(|current_monitor| {
                current_monitor.monitor_info.index == target_position.monitor_index
            });
            if target_monitor_reached {
                debug!(
                    "[restore_windows] macOS fullscreen: windowed window reached target monitor {}",
                    target_position.monitor_index
                );
                target_position.fullscreen_restore_state = Some(FullscreenRestoreState::ApplyMode);
            } else {
                debug!(
                    "[restore_windows] macOS fullscreen: moving windowed window to target monitor {}",
                    target_position.monitor_index
                );
                window.position = WindowPosition::Centered(MonitorSelection::Index(
                    target_position.monitor_index,
                ));
            }
            RestoreStatus::Waiting
        },
        FullscreenRestoreState::MoveToMonitor => {
            if let Some(position) = target_position.physical_position {
                debug!("[restore_windows] Fullscreen MoveToMonitor: position={position:?}");
                window.position = WindowPosition::At(position);
            }
            target_position.fullscreen_restore_state = Some(FullscreenRestoreState::WaitForMove);
            RestoreStatus::Waiting
        },
        FullscreenRestoreState::WaitForMove => {
            debug!("[restore_windows] Fullscreen WaitForMove: waiting for compositor");
            target_position.fullscreen_restore_state = Some(FullscreenRestoreState::ApplyMode);
            RestoreStatus::Waiting
        },
        FullscreenRestoreState::WaitForSurface => {
            debug!("[restore_windows] Fullscreen WaitForSurface: waiting for GPU surface");
            target_position.fullscreen_restore_state = Some(FullscreenRestoreState::ApplyMode);
            RestoreStatus::Waiting
        },
        FullscreenRestoreState::ApplyMode => RestoreStatus::Complete,
        FullscreenRestoreState::ActivateWindow => {
            #[cfg(target_os = "macos")]
            macos_tabbing_fix::activate_fullscreen_window(entity);
            debug!("[restore_windows] macOS fullscreen: activated window after mode request");
            target_position.fullscreen_restore_state = Some(FullscreenRestoreState::WaitForTarget);
            RestoreStatus::Waiting
        },
        FullscreenRestoreState::WaitForTarget => {
            let target_monitor_reached = current_monitor.is_some_and(|current_monitor| {
                current_monitor.monitor_info.index == target_position.monitor_index
            });
            if native_fullscreen != NativeFullscreenState::Fullscreen || !target_monitor_reached {
                debug!(
                    "[restore_windows] macOS fullscreen: waiting for fullscreen on target monitor {}",
                    target_position.monitor_index
                );
                return RestoreStatus::Waiting;
            }
            debug!(
                "[restore_windows] macOS fullscreen: AppKit reported fullscreen on target monitor {}",
                target_position.monitor_index
            );
            target_position.fullscreen_restore_state = None;
            RestoreStatus::Complete
        },
    }
}

/// Apply pending window restore. Runs only when entities with `TargetPosition` exist.
pub(crate) fn restore_windows(
    mut windows: Query<
        (
            Entity,
            &RestorePreparation,
            &mut TargetPosition,
            &mut Window,
            Option<&CurrentMonitor>,
        ),
        With<X11FrameCompensated>,
    >,
    _: NonSendMarker,
    #[cfg(target_os = "macos")] mut fullscreen_observations: NonSendMut<
        NativeFullscreenObservations,
    >,
    platform: Res<Platform>,
    scale_inputs: Res<ObservedScaleInputs>,
    registrations: Option<Res<RecoveryRegistrations>>,
    monitors: Option<Res<Monitors>>,
    revision: Option<Res<MonitorTopologyRevision>>,
    #[cfg(test)] injected_windows: Option<Res<InjectedWinitWindows>>,
) {
    for (entity, restore_preparation, mut target_position, mut window, current_monitor) in
        &mut windows
    {
        if let (Some(restore_attempt), Some(registrations), Some(monitors), Some(revision)) = (
            restore_preparation.recovery_attempt(),
            &registrations,
            &monitors,
            &revision,
        ) && restore_attempt::restore_attempt_is_current(
            restore_attempt,
            entity,
            registrations,
            monitors,
            **revision,
        ) == RestoreAttemptStatus::Stale
        {
            continue;
        }
        #[cfg(test)]
        let native_window_exists = native_window_exists(entity, injected_windows.as_deref());
        #[cfg(not(test))]
        let native_window_exists = native_window_exists(entity);
        #[cfg(target_os = "macos")]
        let native_fullscreen = if native_window_exists
            && *platform == Platform::MacOs
            && target_position.saved_window_mode.is_fullscreen()
        {
            fullscreen_observations.observe(entity)
        } else {
            NativeFullscreenState::Unavailable
        };
        #[cfg(not(target_os = "macos"))]
        let native_fullscreen = NativeFullscreenState::Unavailable;
        restore_window(
            entity,
            restore_preparation,
            &mut target_position,
            &mut window,
            &scale_inputs,
            *platform,
            native_window_exists,
            current_monitor,
            native_fullscreen,
        );
        #[cfg(target_os = "macos")]
        if target_position.settle_state.is_some() {
            fullscreen_observations.stop(entity);
        }
    }
}

fn restore_window(
    entity: Entity,
    restore_preparation: &RestorePreparation,
    target_position: &mut TargetPosition,
    window: &mut Window,
    scale_inputs: &ObservedScaleInputs,
    platform: Platform,
    native_window_exists: bool,
    current_monitor: Option<&CurrentMonitor>,
    native_fullscreen: NativeFullscreenState,
) {
    if target_position.settle_state.is_some() {
        return;
    }

    if !native_window_exists {
        debug!("[restore_windows] Skipping entity {entity:?}: winit window not yet created");
        return;
    }

    correct_initial_starting_scale(entity, target_position, window, platform);

    let macos_fullscreen =
        platform == Platform::MacOs && target_position.saved_window_mode.is_fullscreen();
    if macos_fullscreen
        && matches!(
            advance_fullscreen_restore(
                entity,
                target_position,
                window,
                current_monitor,
                native_fullscreen,
            ),
            RestoreStatus::Waiting
        )
    {
        return;
    }

    if !macos_fullscreen
        && matches!(
            target_position.monitor_scale_strategy,
            MonitorScaleStrategy::HigherToLower(WindowRestoreState::NeedInitialMove)
                | MonitorScaleStrategy::CompensateSizeOnly(WindowRestoreState::NeedInitialMove)
        )
    {
        begin_cross_dpi_restore(target_position, window, restore_preparation.attempt_id());
        return;
    }

    match target_position.monitor_scale_strategy {
        MonitorScaleStrategy::HigherToLower(WindowRestoreState::WaitingForScaleChange {
            attempt_id,
        }) if matching_scale_change(
            entity,
            restore_preparation.attempt_id(),
            attempt_id,
            target_position.target_scale,
            f64::from(window.resolution.base_scale_factor()),
            scale_inputs,
        ) =>
        {
            debug!(
                "[Restore] ScaleChanged received, transitioning to WindowRestoreState::ApplySize"
            );
            target_position.monitor_scale_strategy =
                MonitorScaleStrategy::HigherToLower(WindowRestoreState::ApplySize);
        },
        MonitorScaleStrategy::CompensateSizeOnly(WindowRestoreState::WaitingForScaleChange {
            attempt_id,
        }) if matching_scale_change(
            entity,
            restore_preparation.attempt_id(),
            attempt_id,
            target_position.target_scale,
            f64::from(window.resolution.base_scale_factor()),
            scale_inputs,
        ) =>
        {
            debug!("[Restore] CompensateSizeOnly: transitioning to ApplySize");
            target_position.monitor_scale_strategy =
                MonitorScaleStrategy::CompensateSizeOnly(WindowRestoreState::ApplySize);
        },
        _ => {},
    }

    if !macos_fullscreen
        && matches!(
            advance_fullscreen_restore(
                entity,
                target_position,
                window,
                current_monitor,
                native_fullscreen,
            ),
            RestoreStatus::Waiting
        )
    {
        return;
    }

    let applying_macos_fullscreen = macos_fullscreen
        && target_position.fullscreen_restore_state == Some(FullscreenRestoreState::ApplyMode);
    let restore_status = try_apply_restore(target_position, window, platform);
    if matches!(restore_status, RestoreStatus::Waiting) {
        return;
    }
    if applying_macos_fullscreen {
        target_position.fullscreen_restore_state = Some(FullscreenRestoreState::ActivateWindow);
        return;
    }

    if target_position.settle_state.is_none() {
        let settle_stability_ms = SETTLE_STABILITY_SECS * MILLIS_PER_SECOND;
        debug!(
            "[restore_windows] Restore applied, starting settle ({settle_stability_ms:.0}ms stability / {SETTLE_TIMEOUT_SECS:.0}s timeout)"
        );
        target_position.settle_state = Some(SettleState::new());
    }
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
            WindowRestoreState::NeedInitialMove | WindowRestoreState::WaitingForScaleChange { .. },
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
            // `request_inner_size` produces the requested full physical size
            // (compensating it would halve it).
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
            WindowRestoreState::NeedInitialMove | WindowRestoreState::WaitingForScaleChange { .. },
        ) => {
            debug!("[Restore] HigherToLower: waiting for initial move or ScaleChanged message");
            return RestoreStatus::Waiting;
        },
    }

    window.visible = true;
    RestoreStatus::Complete
}

#[cfg(all(test, target_os = "macos"))]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use bevy::window::MonitorSelection;

    use super::*;
    use crate::WindowKey;
    use crate::monitors::MonitorIdentity;

    const FALLBACK_MONITOR_INDEX: usize = 0;
    const TARGET_MONITOR_INDEX: usize = 2;

    const fn current_monitor(index: usize, scale: f64) -> CurrentMonitor {
        CurrentMonitor {
            monitor_info:          crate::MonitorInfo {
                identity: MonitorIdentity::Unverified,
                index,
                scale,
                physical_position: IVec2::ZERO,
                physical_size: UVec2::new(3_440, 1_440),
            },
            effective_window_mode: WindowMode::BorderlessFullscreen(MonitorSelection::Index(index)),
        }
    }

    const fn borderless_target() -> TargetPosition {
        TargetPosition {
            physical_position:        Some(IVec2::new(-4_256, -2_249)),
            logical_position:         Some(IVec2::new(-4_256, -2_249)),
            physical_size:            UVec2::new(3_440, 1_440),
            logical_size:             UVec2::new(3_440, 1_440),
            target_scale:             1.0,
            starting_scale:           1.0,
            monitor_scale_strategy:   MonitorScaleStrategy::ApplyUnchanged,
            saved_window_mode:        SavedWindowMode::BorderlessFullscreen,
            monitor_index:            TARGET_MONITOR_INDEX,
            fullscreen_restore_state: Some(FullscreenRestoreState::LeaveFullscreen),
            settle_state:             None,
        }
    }

    fn advance(
        target: &mut TargetPosition,
        window: &mut Window,
        current_monitor: &CurrentMonitor,
        native_fullscreen: NativeFullscreenState,
    ) {
        restore_window(
            Entity::from_bits(1),
            &RestorePreparation::startup(WindowKey::Primary),
            target,
            window,
            &ObservedScaleInputs::default(),
            Platform::MacOs,
            true,
            Some(current_monitor),
            native_fullscreen,
        );
    }

    #[test]
    fn macos_borderless_retarget_moves_windowed_to_target_before_fullscreen() {
        let fallback = current_monitor(FALLBACK_MONITOR_INDEX, 2.0);
        let target_monitor = current_monitor(TARGET_MONITOR_INDEX, 1.0);
        let mut target = borderless_target();
        let mut window = Window {
            mode: WindowMode::BorderlessFullscreen(MonitorSelection::Index(FALLBACK_MONITOR_INDEX)),
            ..default()
        };

        advance(
            &mut target,
            &mut window,
            &fallback,
            NativeFullscreenState::Fullscreen,
        );
        assert_eq!(window.mode, WindowMode::Windowed);
        assert_eq!(
            target.fullscreen_restore_state,
            Some(FullscreenRestoreState::MoveWindowedToTarget)
        );

        advance(
            &mut target,
            &mut window,
            &target_monitor,
            NativeFullscreenState::Fullscreen,
        );
        assert_eq!(
            target.fullscreen_restore_state,
            Some(FullscreenRestoreState::MoveWindowedToTarget)
        );
        assert_eq!(window.position, WindowPosition::Automatic);

        advance(
            &mut target,
            &mut window,
            &fallback,
            NativeFullscreenState::Windowed,
        );
        assert_eq!(
            target.fullscreen_restore_state,
            Some(FullscreenRestoreState::MoveWindowedToTarget)
        );
        assert_eq!(
            window.position,
            WindowPosition::Centered(MonitorSelection::Index(TARGET_MONITOR_INDEX))
        );

        advance(
            &mut target,
            &mut window,
            &target_monitor,
            NativeFullscreenState::Windowed,
        );
        assert_eq!(
            target.fullscreen_restore_state,
            Some(FullscreenRestoreState::ApplyMode)
        );

        advance(
            &mut target,
            &mut window,
            &fallback,
            NativeFullscreenState::Windowed,
        );
        assert_eq!(
            window.mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Index(TARGET_MONITOR_INDEX))
        );
        assert_eq!(
            target.fullscreen_restore_state,
            Some(FullscreenRestoreState::ActivateWindow)
        );
        assert!(target.settle_state.is_none());

        advance(
            &mut target,
            &mut window,
            &fallback,
            NativeFullscreenState::Windowed,
        );
        assert_eq!(
            target.fullscreen_restore_state,
            Some(FullscreenRestoreState::WaitForTarget)
        );

        advance(
            &mut target,
            &mut window,
            &fallback,
            NativeFullscreenState::Fullscreen,
        );
        assert!(target.settle_state.is_none());

        advance(
            &mut target,
            &mut window,
            &target_monitor,
            NativeFullscreenState::Fullscreen,
        );
        assert!(target.fullscreen_restore_state.is_none());
        assert!(target.settle_state.is_some());
    }
}
