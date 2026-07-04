//! Settle checking logic.
//!
//! After a window restore is applied, monitors the actual window state each frame
//! to confirm the compositor delivered matching values (or detect mismatches).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowMode;
use bevy_kana::ToI32;
use bevy_kana::ToU32;

use super::target_position::TargetPosition;
use super::winit_info::X11FrameCompensated;
use crate::ManagedWindow;
use crate::Platform;
use crate::WindowKey;
use crate::constants::MILLIS_PER_SECOND;
use crate::constants::PRIMARY_MONITOR_INDEX;
use crate::constants::SETTLE_STABILITY_SECS;
use crate::constants::SETTLE_TIMEOUT_SECS;
use crate::events::WindowRestoreMismatch;
use crate::events::WindowRestored;
use crate::monitors::CurrentMonitor;

/// Snapshot of compared values for change detection between frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
struct SettleSnapshot {
    physical_position: Option<IVec2>,
    physical_size:     UVec2,
    window_mode:       WindowMode,
    monitor:           usize,
}

/// Tracks the two-timer settling state after restore completes.
#[derive(Debug, Clone, Reflect)]
pub(crate) struct SettleState {
    /// Hard deadline timer — fires mismatch if stability is never reached.
    total_timeout:   Timer,
    /// Resets whenever any compared value changes between frames.
    stability_timer: Timer,
    /// Snapshot of last frame's compared values, used to detect changes.
    last_snapshot:   Option<SettleSnapshot>,
}

impl SettleState {
    /// Create a new settle state with default durations.
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            total_timeout:   Timer::from_seconds(SETTLE_TIMEOUT_SECS, TimerMode::Once),
            stability_timer: Timer::from_seconds(SETTLE_STABILITY_SECS, TimerMode::Once),
            last_snapshot:   None,
        }
    }
}

/// Build a [`SettleSnapshot`] from the current window state, returning the snapshot
/// and the actual scale factor (tracked separately since scale is informational).
fn build_actual_snapshot(
    window: &Window,
    current_monitor: Option<&CurrentMonitor>,
    platform: Platform,
) -> (SettleSnapshot, f64) {
    let physical_position = if platform.position_available() {
        match window.position {
            WindowPosition::At(p) => Some(IVec2::new(p.x, p.y)),
            _ => None,
        }
    } else {
        None
    };
    let physical_size = UVec2::new(
        window.resolution.physical_width(),
        window.resolution.physical_height(),
    );
    (
        SettleSnapshot {
            physical_position,
            physical_size,
            window_mode: window.mode,
            monitor: current_monitor.map_or(PRIMARY_MONITOR_INDEX, |current_monitor| {
                current_monitor.monitor_info.index
            }),
        },
        f64::from(window.resolution.scale_factor()),
    )
}

/// Check whether actual window state matches the target for settle purposes.
///
/// Fullscreen modes skip position and size comparison — the window fills the
/// monitor so the stored position/size are irrelevant. On macOS, borderless
/// fullscreen reports position offset by the menu bar height; on X11 (W6),
/// frame vs client coords differ. The physical size can also differ when
/// scales differ between backends (e.g. Wayland scale 1 vs `XWayland` scale 2).
fn check_settle_matches(
    target_position: &TargetPosition,
    target_physical_position: Option<IVec2>,
    target_physical_size: UVec2,
    target_window_mode: WindowMode,
    target_monitor: usize,
    settle_snapshot: &SettleSnapshot,
    platform: Platform,
) -> SettleComparison {
    let is_fullscreen = target_position.saved_window_mode.is_fullscreen();
    // Skip position comparison when:
    // - fullscreen (window fills monitor; saved position is irrelevant)
    // - no saved position (window was anchored via `WindowPosition::Centered`; the resulting `At`
    //   position is OS-chosen and not part of the comparison)
    // - X11 W6 frame-vs-client coordinate mismatch
    let skip_position = is_fullscreen
        || target_physical_position.is_none()
        || !platform.position_reliable_for_settle();
    let position_matches =
        skip_position || target_physical_position == settle_snapshot.physical_position;
    let size_match = is_fullscreen || target_physical_size == settle_snapshot.physical_size;
    let mode_match = platform.modes_match(target_window_mode, settle_snapshot.window_mode);
    let monitor_match = target_monitor == settle_snapshot.monitor;
    SettleComparison {
        position: position_matches.into(),
        size:     size_match.into(),
        mode:     mode_match.into(),
        monitor:  monitor_match.into(),
    }
}

#[derive(Clone, Copy)]
enum Comparison {
    Match,
    Mismatch,
}

impl From<bool> for Comparison {
    fn from(matches: bool) -> Self { if matches { Self::Match } else { Self::Mismatch } }
}

impl Comparison {
    const fn is_match(self) -> bool { matches!(self, Self::Match) }
}

struct SettleComparison {
    position: Comparison,
    size:     Comparison,
    mode:     Comparison,
    monitor:  Comparison,
}

impl SettleComparison {
    const fn all_match(&self) -> bool {
        self.position.is_match()
            && self.size.is_match()
            && self.mode.is_match()
            && self.monitor.is_match()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TimeoutState {
    Active,
    TimedOut,
}

#[derive(Clone, Copy)]
enum ChangeHandling {
    Skip,
    Continue,
}

/// Detect whether the settle snapshot changed from the previous frame and reset the
/// stability timer if so.
fn detect_settle_change(
    settle: &mut SettleState,
    settle_snapshot: SettleSnapshot,
    window_key: &WindowKey,
    total_elapsed_ms: f32,
    timeout_state: TimeoutState,
) -> ChangeHandling {
    let changed = settle.last_snapshot.as_ref() != Some(&settle_snapshot);
    if changed {
        if settle.last_snapshot.is_some() {
            debug!(
                "[check_restore_settling] [{window_key}] {total_elapsed_ms:.0}ms: values changed, \
                 resetting stability timer"
            );
        }
        settle.stability_timer.reset();
        settle.last_snapshot = Some(settle_snapshot);
        match timeout_state {
            TimeoutState::TimedOut => ChangeHandling::Continue,
            TimeoutState::Active => ChangeHandling::Skip,
        }
    } else {
        ChangeHandling::Continue
    }
}

/// Resolve the [`WindowKey`] for an entity — `Primary` if it has the `PrimaryWindow`
/// marker, otherwise the `ManagedWindow` name (falling back to `Primary`).
fn resolve_window_key(
    entity: Entity,
    primary_query: &Query<(), With<PrimaryWindow>>,
    managed_query: &Query<&ManagedWindow>,
) -> WindowKey {
    if primary_query.get(entity).is_ok() {
        WindowKey::Primary
    } else if let Ok(managed) = managed_query.get(entity) {
        WindowKey::Managed(managed.name.clone())
    } else {
        WindowKey::Primary
    }
}

/// Check settling windows each frame using a two-timer approach.
///
/// - **Stability timer** (200ms): resets whenever any compared value changes. If values stay stable
///   for 200ms, fires `WindowRestored`.
/// - **Total timeout** (2s): hard deadline. Fires `WindowRestoreMismatch` if stability is never
///   reached.
///
/// Runs while `TargetPosition` entities exist (same gate as `restore_windows`).
/// Only processes entities that have a `settle_state` set.
pub(crate) fn check_restore_settling(
    mut commands: Commands,
    time: Res<Time>,
    mut windows: Query<
        (
            Entity,
            &mut TargetPosition,
            &Window,
            Option<&CurrentMonitor>,
        ),
        With<X11FrameCompensated>,
    >,
    primary_query: Query<(), With<PrimaryWindow>>,
    managed_query: Query<&ManagedWindow>,
    platform: Res<Platform>,
) {
    for (entity, mut target_position, window, current_monitor) in &mut windows {
        let target_window_mode = target_position
            .saved_window_mode
            .to_window_mode(target_position.monitor_index);
        let target_physical_size = target_position.physical_size;
        let target_logical_size = target_position.logical_size;
        let target_monitor = target_position.monitor_index;
        let expected_scale = target_position.target_scale;

        let target_physical_position = platform
            .position_available()
            .then_some(target_position.physical_position)
            .flatten();
        let target_logical_position = platform
            .position_available()
            .then_some(target_position.logical_position)
            .flatten();
        let window_key = resolve_window_key(entity, &primary_query, &managed_query);
        let (current_snapshot, actual_scale) =
            build_actual_snapshot(window, current_monitor, *platform);

        let Some(settle) = target_position.settle_state.as_mut() else {
            continue;
        };
        settle.total_timeout.tick(time.delta());
        settle.stability_timer.tick(time.delta());

        let total_elapsed_ms = settle.total_timeout.elapsed_secs() * MILLIS_PER_SECOND;
        let stability_elapsed_ms = settle.stability_timer.elapsed_secs() * MILLIS_PER_SECOND;
        let timeout_state = if settle.total_timeout.is_finished() {
            TimeoutState::TimedOut
        } else {
            TimeoutState::Active
        };

        if matches!(
            detect_settle_change(
                settle,
                current_snapshot,
                &window_key,
                total_elapsed_ms,
                timeout_state,
            ),
            ChangeHandling::Skip
        ) {
            continue;
        }
        let stable = settle.stability_timer.is_finished();
        let comparison = check_settle_matches(
            &target_position,
            target_physical_position,
            target_physical_size,
            target_window_mode,
            target_monitor,
            &current_snapshot,
            *platform,
        );
        debug!(
            "[check_restore_settling] [{window_key}] {total_elapsed_ms:.0}ms (stable: {stability_elapsed_ms:.0}ms): \
             position={} size={} mode={} monitor={} | \
             size: {target_physical_size} vs {}, \
             mode: {target_window_mode:?} vs {:?}, \
             monitor: {target_monitor} vs {}, \
             scale: {expected_scale} vs {actual_scale}",
            comparison.position.is_match(),
            comparison.size.is_match(),
            comparison.mode.is_match(),
            comparison.monitor.is_match(),
            current_snapshot.physical_size,
            current_snapshot.window_mode,
            current_snapshot.monitor,
        );

        let settle_target = SettleTarget {
            physical_position: target_physical_position,
            logical_position:  target_logical_position,
            physical_size:     target_physical_size,
            logical_size:      target_logical_size,
            window_mode:       target_window_mode,
            monitor:           target_monitor,
            scale:             expected_scale,
        };
        if stable && comparison.all_match() {
            emit_settle_success(
                &mut commands,
                entity,
                window_key,
                &settle_target,
                total_elapsed_ms,
                stability_elapsed_ms,
            );
        } else if timeout_state == TimeoutState::TimedOut {
            emit_settle_mismatch(
                &mut commands,
                entity,
                window_key,
                &settle_target,
                &build_settle_actual(window, current_snapshot, actual_scale),
                total_elapsed_ms,
            );
        }
    }
}

/// Bundled actual values for settle mismatch reporting.
struct SettleActual {
    settle_snapshot: SettleSnapshot,
    scale:           f64,
    logical_size:    UVec2,
}

fn build_settle_actual(
    window: &Window,
    settle_snapshot: SettleSnapshot,
    scale: f64,
) -> SettleActual {
    SettleActual {
        settle_snapshot,
        scale,
        logical_size: UVec2::new(
            window.resolution.width().to_u32(),
            window.resolution.height().to_u32(),
        ),
    }
}

/// Extracted target values for settle resolution, avoiding too-many-arguments.
struct SettleTarget {
    physical_position: Option<IVec2>,
    logical_position:  Option<IVec2>,
    logical_size:      UVec2,
    physical_size:     UVec2,
    window_mode:       WindowMode,
    monitor:           usize,
    scale:             f64,
}

/// Emit `WindowRestored` and clean up `TargetPosition` when settle succeeds.
fn emit_settle_success(
    commands: &mut Commands,
    entity: Entity,
    window_key: WindowKey,
    settle_target: &SettleTarget,
    total_elapsed_ms: f32,
    stability_elapsed_ms: f32,
) {
    debug!(
        "[check_restore_settling] [{window_key}] Settled after {total_elapsed_ms:.0}ms \
         (stable for {stability_elapsed_ms:.0}ms)"
    );
    commands
        .entity(entity)
        .trigger(|entity| WindowRestored {
            entity,
            window_key,
            physical_position: settle_target.physical_position,
            logical_position: settle_target.logical_position,
            logical_size: settle_target.logical_size,
            physical_size: settle_target.physical_size,
            window_mode: settle_target.window_mode,
            monitor_index: settle_target.monitor,
        })
        .remove::<TargetPosition>()
        .remove::<X11FrameCompensated>();
}

/// Emit `WindowRestoreMismatch` and clean up `TargetPosition` when settle times out.
fn emit_settle_mismatch(
    commands: &mut Commands,
    entity: Entity,
    window_key: WindowKey,
    settle_target: &SettleTarget,
    settle_actual: &SettleActual,
    total_elapsed_ms: f32,
) {
    warn!(
        "[check_restore_settling] [{window_key}] Settle timeout after {total_elapsed_ms:.0}ms — \
        mismatch remains: \
         position: {:?} vs {:?}, \
         size: {} vs {}, \
         mode: {:?} vs {:?}, \
         monitor: {} vs {}, \
         scale: {} vs {}",
        settle_target.physical_position,
        settle_actual.settle_snapshot.physical_position,
        settle_target.physical_size,
        settle_actual.settle_snapshot.physical_size,
        settle_target.window_mode,
        settle_actual.settle_snapshot.window_mode,
        settle_target.monitor,
        settle_actual.settle_snapshot.monitor,
        settle_target.scale,
        settle_actual.scale,
    );
    let actual_logical_position = settle_actual
        .settle_snapshot
        .physical_position
        .map(|position| {
            IVec2::new(
                (f64::from(position.x) / settle_actual.scale)
                    .round()
                    .to_i32(),
                (f64::from(position.y) / settle_actual.scale)
                    .round()
                    .to_i32(),
            )
        });
    commands
        .entity(entity)
        .trigger(|entity| WindowRestoreMismatch {
            entity,
            window_key,
            expected_physical_position: settle_target.physical_position,
            actual_physical_position: settle_actual.settle_snapshot.physical_position,
            expected_logical_position: settle_target.logical_position,
            actual_logical_position,
            expected_physical_size: settle_target.physical_size,
            actual_physical_size: settle_actual.settle_snapshot.physical_size,
            expected_logical_size: settle_target.logical_size,
            actual_logical_size: settle_actual.logical_size,
            expected_window_mode: settle_target.window_mode,
            actual_window_mode: settle_actual.settle_snapshot.window_mode,
            expected_monitor: settle_target.monitor,
            actual_monitor: settle_actual.settle_snapshot.monitor,
            expected_scale: settle_target.scale,
            actual_scale: settle_actual.scale,
        })
        .remove::<TargetPosition>()
        .remove::<X11FrameCompensated>();
}
