use bevy::prelude::*;
use bevy_kana::ToI32;
use bevy_kana::ToU32;

use super::strategy::FullscreenRestoreState;
use super::strategy::MonitorScaleStrategy;
use crate::Platform;
use crate::monitors::MonitorInfo;
use crate::persistence::PersistedWindowState;
use crate::persistence::SavedWindowMode;
use crate::restore::settle_state::SettleState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreparedWindowPosition {
    PersistedCoordinate(IVec2),
    PersistedWithoutCoordinate,
    CapturedRestorable {
        physical_position: IVec2,
        logical_position:  IVec2,
    },
    CompositorControlled,
    TargetUnavailable,
}

/// Holds the target window state during the restore process.
///
/// Values converted from saved state are stored as `IVec2`, `UVec2`,
/// `WindowMode`, and scale factors during loading, before restore logic reads
/// them.
///
/// Dimensions stored here are **inner** (content area only), matching what
/// Bevy's `Window.resolution` represents and what we save to the state file.
/// Outer dimensions (including title bar) are only used during loading for
/// clamping calculations.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub(crate) struct TargetPosition {
    /// Final clamped position (adjusted to fit within target monitor).
    /// None on Wayland where clients can't access window position.
    pub(crate) physical_position:        Option<IVec2>,
    /// Pre-scale position from the saved state, preserved for event reporting.
    /// None on Wayland (no position was ever saved) or when the saved state had none.
    pub(crate) logical_position:         Option<IVec2>,
    /// Target size in physical pixels (content area, excluding window decoration).
    pub(crate) physical_size:            UVec2,
    /// Target size in logical pixels from the saved state.
    pub(crate) logical_size:             UVec2,
    /// Scale factor of the target monitor.
    pub(crate) target_scale:             f64,
    /// Scale factor of the monitor where the window starts (keyboard focus monitor).
    pub(crate) starting_scale:           f64,
    /// Strategy for handling scale factor differences between monitors.
    pub(crate) monitor_scale_strategy:   MonitorScaleStrategy,
    /// Window mode to restore.
    pub(crate) saved_window_mode:        SavedWindowMode,
    /// Target monitor index for fullscreen restore.
    /// On non-Wayland platforms, this could be derived from position, but Wayland
    /// doesn't provide window position, so we store it explicitly.
    pub(crate) monitor_index:            usize,
    /// Fullscreen restore state (DX12/DXGI workaround).
    pub(crate) fullscreen_restore_state: Option<FullscreenRestoreState>,
    /// Settling state. When set, `try_apply_restore` has completed and we're waiting
    /// for the compositor/winit to deliver stable, matching state.
    ///
    /// Uses a two-timer approach:
    /// - **Stability timer** (200ms): resets whenever any compared value changes between frames.
    ///   Fires `WindowRestored` when all values have been stable for 200ms.
    /// - **Total timeout** (2s): hard deadline. If values never stabilize for 200ms continuously,
    ///   fires `WindowRestoreMismatch` with whatever state exists at timeout.
    ///
    /// This handles transient Wayland `wl_surface.enter`/`wl_surface.leave`
    /// reports where `current_monitor()` briefly returns the wrong monitor during
    /// fullscreen transitions.
    pub(crate) settle_state:             Option<SettleState>,
}

impl TargetPosition {
    /// Scale ratio between starting and target monitors.
    #[must_use]
    pub(super) const fn ratio(&self) -> f64 { self.starting_scale / self.target_scale }

    /// Position compensated for scale factor differences.
    ///
    /// Multiplies physical position by the ratio to account for winit dividing by launch scale.
    /// Returns None if position is not available (Wayland).
    #[must_use]
    pub(super) fn compensated_position(&self) -> Option<IVec2> {
        let ratio = self.ratio();
        self.physical_position.map(|position| {
            IVec2::new(
                (f64::from(position.x) * ratio).to_i32(),
                (f64::from(position.y) * ratio).to_i32(),
            )
        })
    }

    /// Size compensated for scale factor differences.
    ///
    /// Multiplies physical size by the ratio to account for winit dividing by launch scale.
    #[must_use]
    pub(super) fn compensated_size(&self) -> UVec2 {
        let ratio = self.ratio();
        UVec2::new(
            (f64::from(self.physical_size.x) * ratio).to_u32(),
            (f64::from(self.physical_size.y) * ratio).to_u32(),
        )
    }
}

/// Durable record of a restore's launch context and chosen strategy.
///
/// Unlike [`TargetPosition`], this is **not** removed when the restore settles —
/// it persists so a test can read, via BRP, which monitor the window actually
/// launched on and which [`MonitorScaleStrategy`] ran. The launch monitor is
/// environmental on macOS (the OS picks the spawn display), so a cross-DPI test
/// can silently degrade into a same-scale restore; asserting these fields makes
/// `RestoreDiagnostics` expose that same-scale fallback through BRP assertions.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component)]
pub(crate) struct RestoreDiagnostics {
    /// Monitor the window launched on, before any restore move.
    pub(crate) starting_monitor_index: usize,
    /// Scale factor of the launch monitor.
    pub(crate) starting_scale:         f64,
    /// Scale factor of the restore target monitor.
    pub(crate) target_scale:           f64,
    /// Strategy chosen from the launch-versus-target scale relationship.
    pub(crate) monitor_scale_strategy: MonitorScaleStrategy,
}

/// Compute a `TargetPosition` from saved state and a resolved target monitor.
#[must_use]
pub(crate) fn compute_target_position(
    saved_window_state: &PersistedWindowState,
    target_info: &MonitorInfo,
    prepared_window_position: PreparedWindowPosition,
    physical_decoration: UVec2,
    starting_scale: f64,
    platform: Platform,
) -> TargetPosition {
    let target_scale = target_info.scale;

    // Convert logical → physical using the target monitor's scale factor.
    // This is the single conversion point for size values.
    let physical_width = (f64::from(saved_window_state.logical_width) * target_scale).to_u32();
    let physical_height = (f64::from(saved_window_state.logical_height) * target_scale).to_u32();

    let physical_outer_width = physical_width + physical_decoration.x;
    let physical_outer_height = physical_height + physical_decoration.y;
    let (physical_position, logical_position) = match prepared_window_position {
        PreparedWindowPosition::PersistedCoordinate(logical_position) => {
            let physical_position = clamp_position_to_monitor(
                (f64::from(logical_position.x) * target_scale)
                    .round()
                    .to_i32(),
                (f64::from(logical_position.y) * target_scale)
                    .round()
                    .to_i32(),
                target_info,
                physical_outer_width,
                physical_outer_height,
                platform,
            );
            (Some(physical_position), Some(logical_position))
        },
        PreparedWindowPosition::CapturedRestorable {
            physical_position,
            logical_position,
        } => (Some(physical_position), Some(logical_position)),
        PreparedWindowPosition::PersistedWithoutCoordinate
        | PreparedWindowPosition::CompositorControlled
        | PreparedWindowPosition::TargetUnavailable => (None, None),
    };

    TargetPosition {
        physical_position,
        logical_position,
        physical_size: UVec2::new(physical_width, physical_height),
        logical_size: UVec2::new(
            saved_window_state.logical_width,
            saved_window_state.logical_height,
        ),
        target_scale,
        starting_scale,
        monitor_scale_strategy: platform.scale_strategy(starting_scale, target_scale),
        saved_window_mode: saved_window_state.saved_window_mode.clone(),
        monitor_index: target_info.index,
        fullscreen_restore_state: saved_window_state
            .saved_window_mode
            .is_fullscreen()
            .then_some(platform.fullscreen_restore_state()),
        settle_state: None,
    }
}

/// Calculate restored window position, with optional clamping.
///
/// On macOS, clamps to monitor bounds because macOS may resize/reposition windows
/// that extend beyond the screen. macOS does not allow windows to span monitors.
///
/// On Windows and Linux, windows can legitimately span multiple monitors,
/// so we preserve the exact saved position without clamping.
#[must_use]
fn clamp_position_to_monitor(
    physical_saved_x: i32,
    physical_saved_y: i32,
    target_info: &MonitorInfo,
    physical_outer_width: u32,
    physical_outer_height: u32,
    platform: Platform,
) -> IVec2 {
    if platform.should_clamp_position() {
        let physical_monitor_right =
            target_info.physical_position.x + target_info.physical_size.x.to_i32();
        let physical_monitor_bottom =
            target_info.physical_position.y + target_info.physical_size.y.to_i32();

        let mut physical_x = physical_saved_x;
        let mut physical_y = physical_saved_y;

        if physical_x + physical_outer_width.to_i32() > physical_monitor_right {
            physical_x = physical_monitor_right - physical_outer_width.to_i32();
        }
        if physical_y + physical_outer_height.to_i32() > physical_monitor_bottom {
            physical_y = physical_monitor_bottom - physical_outer_height.to_i32();
        }
        physical_x = physical_x.max(target_info.physical_position.x);
        physical_y = physical_y.max(target_info.physical_position.y);

        if physical_x != physical_saved_x || physical_y != physical_saved_y {
            debug!(
                "[clamp_position_to_monitor] Clamped: ({physical_saved_x}, {physical_saved_y}) -> ({physical_x}, {physical_y}) for outer size {physical_outer_width}x{physical_outer_height}"
            );
        }

        IVec2::new(physical_x, physical_y)
    } else {
        IVec2::new(physical_saved_x, physical_saved_y)
    }
}
