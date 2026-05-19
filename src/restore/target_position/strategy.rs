use bevy::prelude::Reflect;

/// State for `MonitorScaleStrategy::HigherToLower` (high→low DPI restore).
///
/// When restoring from a high-DPI to low-DPI monitor, we must set position BEFORE size
/// because Bevy's `changed_windows` system processes size changes before position changes.
/// If we set both together, the window resizes first while still at the old position,
/// temporarily extending into the wrong monitor and triggering a scale factor bounce from macOS.
///
/// By moving a 1x1 window to the final position first, we ensure the window is already
/// at the correct location when we later apply size in `ApplySize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub(crate) enum WindowRestoreState {
    /// Initial state: window needs to be moved to the target monitor to trigger a scale change.
    /// Handled by `restore_windows` which calls `apply_initial_move` and transitions to
    /// `WaitingForScaleChange`. This unified entry point replaces the old separate paths
    /// (`PreStartup` `move_to_target_monitor` for primary, inline guard for managed).
    NeedInitialMove,
    /// Position applied with compensation, waiting for `ScaleChanged` message.
    WaitingForScaleChange,
    /// Scale changed, ready to apply final size (position already set in phase 1).
    ApplySize,
}

/// Phase-based fullscreen restore state machine.
///
/// Fullscreen restore requires platform-specific sequencing:
///
/// - **Linux X11**: Move to target monitor first, wait a frame for the compositor to process, then
///   apply fullscreen mode as a fresh change.
/// - **Linux Wayland**: Apply mode directly (no position available).
/// - **Windows (DX12)**: Wait for surface creation before applying fullscreen (see <https://github.com/rust-windowing/winit/issues/3124>).
/// - **macOS**: Apply mode directly.
///
/// The key insight: on X11, if fullscreen mode is set in the same frame as
/// position, the compositor may briefly honor it then revert. Splitting into
/// separate frames ensures each change is processed independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub(crate) enum FullscreenRestoreState {
    /// Move window to target monitor position. Skipped on Wayland (no position).
    MoveToMonitor,
    /// Wait for compositor to process the position change (1 frame).
    WaitForMove,
    /// Wait for GPU surface creation (Windows DX12 workaround, winit #3124).
    WaitForSurface,
    /// Apply the fullscreen mode.
    ApplyMode,
}

/// Restore strategy based on scale factor relationship between launch and target monitors.
///
/// # The Problem
///
/// Winit's `request_inner_size` and `set_outer_position` use the current window's scale factor
/// when interpreting coordinates, rather than the target monitor's scale factor. This causes
/// incorrect sizing/positioning when restoring windows across monitors with different DPIs.
///
/// See: <https://github.com/rust-windowing/winit/issues/4440>
///
/// # Platform Differences
///
/// ## Windows
///
/// - **Position**: Winit uses physical coordinates directly - no compensation needed
/// - **Size**: Winit applies scale conversion using current monitor's scale - needs compensation
/// - Strategy: `CompensateSizeOnly` when scales differ
///
/// Note: Windows has a separate issue where `GetWindowRect` includes an invisible
/// resize border (~7-11 pixels). See: <https://github.com/rust-windowing/winit/issues/4107>
///
/// ## macOS / Linux X11
///
/// - **Position**: Winit converts using current monitor's scale - needs compensation
/// - **Size**: Winit converts using current monitor's scale - needs compensation
/// - Strategy: `LowerToHigher` or `HigherToLower` depending on scale relationship
///
/// ## Linux Wayland
///
/// Cannot detect starting monitor or set position, so no compensation is applied.
///
/// # Variants
///
/// - **`ApplyUnchanged`**: Apply position and size directly without compensation.
///
/// - **`CompensateSizeOnly`**: Windows only. Uses two-phase approach via `WindowRestoreState`:
///   1. Apply position directly + compensated size (triggers `WM_DPICHANGED`)
///   2. After scale changes, re-apply exact target size to eliminate rounding errors
///
/// - **`LowerToHigher`**: macOS/Linux X11. Low→High DPI (1x→2x, ratio < 1). Multiply both position
///   and size by ratio.
///
/// - **`HigherToLower`**: macOS/Linux X11. High→Low DPI (2x→1x, ratio > 1). Uses two-phase approach
///   via `WindowRestoreState` to avoid size clamping:
///   1. Move a 1x1 window to final position (compensated) to trigger scale change
///   2. After scale changes, apply size without compensation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub(crate) enum MonitorScaleStrategy {
    /// Same scale - apply position and size directly.
    ApplyUnchanged,
    /// Windows cross-DPI: position direct, size in two phases.
    CompensateSizeOnly(WindowRestoreState),
    /// Low→High DPI (1x→2x) - apply with compensation (ratio < 1).
    LowerToHigher,
    /// High→Low DPI (2x→1x) - requires two phases (see enum docs).
    HigherToLower(WindowRestoreState),
}
