use bevy::prelude::*;
use bevy_kana::ToI32;
use bevy_kana::ToU32;

use super::strategy::FullscreenRestoreState;
use super::strategy::MonitorScaleStrategy;
use crate::Platform;
use crate::monitors::MonitorInfo;
use crate::persistence::SavedWindowMode;
use crate::persistence::WindowState;
use crate::restore::settle_state::SettleState;

/// Holds the target window state during the restore process.
///
/// All values are pre-computed with proper types. Casting from saved state
/// happens once during loading, not scattered throughout the restore logic.
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
    pub physical_position:        Option<IVec2>,
    /// Pre-scale position from the saved state, preserved for event reporting.
    /// None on Wayland (no position was ever saved) or when the saved state had none.
    pub logical_position:         Option<IVec2>,
    /// Target size in physical pixels (content area, excluding window decoration).
    pub physical_size:            UVec2,
    /// Target size in logical pixels from the saved state.
    pub logical_size:             UVec2,
    /// Scale factor of the target monitor.
    pub target_scale:             f64,
    /// Scale factor of the monitor where the window starts (keyboard focus monitor).
    pub starting_scale:           f64,
    /// Strategy for handling scale factor differences between monitors.
    pub monitor_scale_strategy:   MonitorScaleStrategy,
    /// Window mode to restore.
    pub saved_window_mode:        SavedWindowMode,
    /// Target monitor index for fullscreen restore.
    /// On non-Wayland platforms, this could be derived from position, but Wayland
    /// doesn't provide window position, so we store it explicitly.
    pub monitor_index:            usize,
    /// Fullscreen restore state (DX12/DXGI workaround).
    pub fullscreen_restore_state: Option<FullscreenRestoreState>,
    /// Settling state. When set, `try_apply_restore` has completed and we're waiting
    /// for the compositor/winit to deliver stable, matching state.
    ///
    /// Uses a two-timer approach:
    /// - **Stability timer** (200ms): resets whenever any compared value changes between frames.
    ///   Fires `WindowRestored` when all values have been stable for 200ms.
    /// - **Total timeout** (1s): hard deadline. If values never stabilize for 200ms continuously,
    ///   fires `WindowRestoreMismatch` with whatever state exists at timeout.
    ///
    /// This handles compositor artifacts like Wayland `wl_surface.enter`/`leave` bounces
    /// where `current_monitor()` transiently reports the wrong monitor during fullscreen
    /// transitions.
    pub(crate) settle_state:      Option<SettleState>,
}

impl TargetPosition {
    /// Scale ratio between starting and target monitors.
    #[must_use]
    pub const fn ratio(&self) -> f64 { self.starting_scale / self.target_scale }

    /// Position compensated for scale factor differences.
    ///
    /// Multiplies physical position by the ratio to account for winit dividing by launch scale.
    /// Returns None if position is not available (Wayland).
    #[must_use]
    pub fn compensated_position(&self) -> Option<IVec2> {
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
    pub fn compensated_size(&self) -> UVec2 {
        let ratio = self.ratio();
        UVec2::new(
            (f64::from(self.physical_size.x) * ratio).to_u32(),
            (f64::from(self.physical_size.y) * ratio).to_u32(),
        )
    }
}

/// Compute a `TargetPosition` from saved state and a resolved target monitor.
#[must_use]
pub(crate) fn compute_target_position(
    saved_state: &WindowState,
    target_info: &MonitorInfo,
    logical_fallback_position: Option<(i32, i32)>,
    physical_decoration: UVec2,
    starting_scale: f64,
    platform: Platform,
) -> TargetPosition {
    let target_scale = target_info.scale;

    // Convert logical → physical using the target monitor's scale factor.
    // This is the single conversion point for size values.
    let physical_width = (f64::from(saved_state.logical_width) * target_scale).to_u32();
    let physical_height = (f64::from(saved_state.logical_height) * target_scale).to_u32();

    let physical_outer_width = physical_width + physical_decoration.x;
    let physical_outer_height = physical_height + physical_decoration.y;
    let physical_position = logical_fallback_position.map(|(x, y)| {
        // Convert logical position to physical using the target monitor's scale factor.
        let physical_x = (f64::from(x) * target_scale).round().to_i32();
        let physical_y = (f64::from(y) * target_scale).round().to_i32();
        clamp_position_to_monitor(
            physical_x,
            physical_y,
            target_info,
            physical_outer_width,
            physical_outer_height,
            platform,
        )
    });

    TargetPosition {
        physical_position,
        logical_position: logical_fallback_position.map(|(x, y)| IVec2::new(x, y)),
        physical_size: UVec2::new(physical_width, physical_height),
        logical_size: UVec2::new(saved_state.logical_width, saved_state.logical_height),
        target_scale,
        starting_scale,
        monitor_scale_strategy: platform.scale_strategy(starting_scale, target_scale),
        saved_window_mode: saved_state.saved_window_mode.clone(),
        monitor_index: target_info.index,
        fullscreen_restore_state: saved_state
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
