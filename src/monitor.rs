//! Monitor detection logic.
//!
//! Maintains `CurrentMonitor` on all managed windows using winit detection
//! with position-based fallback.

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::MonitorSelection;
use bevy::window::PrimaryWindow;
use bevy::window::WindowMode;
use bevy::winit::WINIT_WINDOWS;
use bevy_kana::ToI32;

use super::ManagedWindow;
use super::monitors::CurrentMonitor;
use super::monitors::MonitorInfo;
use super::monitors::Monitors;
use crate::constants::MONITOR_SOURCE_EXISTING;
use crate::constants::MONITOR_SOURCE_FALLBACK;
use crate::constants::MONITOR_SOURCE_POSITION;
use crate::constants::MONITOR_SOURCE_WINIT;

/// Unified monitor detection system. Maintains `CurrentMonitor` on all managed windows.
///
/// Detection priority:
/// 1. winit's `current_monitor()` — most reliable, works even before `window.position` is set
/// 2. Position-based center-point detection — uses `window.position` when available
/// 3. Existing `CurrentMonitor` value — preserves last-known monitor during transient states
/// 4. `monitors.first()` — last resort fallback
///
/// All platforms: computes `effective_mode` (handles macOS green button fullscreen).
pub(crate) fn update_current_monitor(
    mut commands: Commands,
    windows: Query<
        (Entity, &Window, Option<&CurrentMonitor>),
        Or<(With<PrimaryWindow>, With<ManagedWindow>)>,
    >,
    monitors: Res<Monitors>,
    _: NonSendMarker,
) {
    if monitors.is_empty() {
        return;
    }

    for (entity, window, existing) in &windows {
        let winit_result = winit_detect_monitor(entity, &monitors);
        let position_result = if winit_result.is_none() {
            position_detect_monitor(window, &monitors)
        } else {
            None
        };

        let (monitor_info, source) = match (winit_result, position_result, existing) {
            (Some(info), _, _) => (info, MONITOR_SOURCE_WINIT),
            (_, Some(info), _) => (info, MONITOR_SOURCE_POSITION),
            (_, _, Some(current_monitor)) => (current_monitor.monitor, MONITOR_SOURCE_EXISTING),
            _ => (*monitors.first(), MONITOR_SOURCE_FALLBACK),
        };

        // Compute effective mode
        let effective_mode = compute_effective_mode(window, &monitor_info, &monitors);

        let new_current = CurrentMonitor {
            monitor: monitor_info,
            effective_mode,
        };

        // Only insert if changed to avoid unnecessary change detection triggers
        let changed = existing.is_none_or(|current_monitor| {
            current_monitor.monitor.index != new_current.monitor.index
                || current_monitor.effective_mode != new_current.effective_mode
        });

        if changed {
            debug!(
                "[update_current_monitor] source={} index={} scale={} effective_mode={:?}",
                source, monitor_info.index, monitor_info.scale, effective_mode
            );
            commands.entity(entity).insert(new_current);
        }
    }
}

/// Detect monitor via winit's `current_monitor()`.
fn winit_detect_monitor(entity: Entity, monitors: &Monitors) -> Option<MonitorInfo> {
    WINIT_WINDOWS.with(|winit_windows| {
        let winit_windows = winit_windows.borrow();
        winit_windows.get_window(entity).and_then(|winit_window| {
            winit_window.current_monitor().and_then(|current_monitor| {
                let physical_position = current_monitor.position();
                monitors
                    .at(physical_position.x, physical_position.y)
                    .copied()
            })
        })
    })
}

/// Detect monitor from `window.position` using center-point logic.
fn position_detect_monitor(window: &Window, monitors: &Monitors) -> Option<MonitorInfo> {
    if let WindowPosition::At(physical_position) = window.position {
        Some(*monitors.monitor_for_window(
            physical_position,
            window.physical_width(),
            window.physical_height(),
        ))
    } else {
        None
    }
}

/// Compute the effective window mode, including macOS green button detection.
///
/// On macOS, clicking the green "maximize" button fills the screen but `window.mode`
/// remains `Windowed`. This detects that case and returns `BorderlessFullscreen`.
fn compute_effective_mode(
    window: &Window,
    monitor_info: &MonitorInfo,
    monitors: &Monitors,
) -> WindowMode {
    // Trust exclusive fullscreen - OS manages this mode
    if matches!(window.mode, WindowMode::Fullscreen(_, _)) {
        return window.mode;
    }

    // Can't determine effective mode without monitors
    if monitors.is_empty() {
        return window.mode;
    }

    // On Wayland, position is unavailable so we can only trust self.mode
    let WindowPosition::At(physical_position) = window.position else {
        return window.mode;
    };

    // Check if window spans full width and reaches bottom of monitor
    let full_width = window.physical_width() == monitor_info.physical_size.x;
    let left_aligned = physical_position.x == monitor_info.physical_position.x;
    let reaches_bottom = physical_position.y + window.physical_height().to_i32()
        == monitor_info.physical_position.y + monitor_info.physical_size.y.to_i32();

    if full_width && left_aligned && reaches_bottom {
        WindowMode::BorderlessFullscreen(MonitorSelection::Index(monitor_info.index))
    } else {
        WindowMode::Windowed
    }
}

#[cfg(test)]
mod tests {
    use bevy::window::MonitorSelection;
    use bevy::window::VideoModeSelection;
    use bevy::window::WindowMode;
    use bevy::window::WindowPosition;

    use super::*;

    fn monitor_0() -> MonitorInfo {
        MonitorInfo {
            index:             0,
            scale:             2.0,
            physical_position: IVec2::ZERO,
            physical_size:     UVec2::new(3456, 2234),
        }
    }

    fn monitors_with(info: MonitorInfo) -> Monitors { Monitors { list: vec![info] } }

    fn window_at(physical_position: IVec2, physical_width: u32, physical_height: u32) -> Window {
        let mut window = Window {
            position: WindowPosition::At(physical_position),
            mode: WindowMode::Windowed,
            ..Default::default()
        };
        window
            .resolution
            .set_physical_resolution(physical_width, physical_height);
        window
    }

    #[test]
    fn effective_mode_fullscreen_when_window_fills_monitor() {
        let monitor = monitor_0();
        let monitors = monitors_with(monitor);
        let window = window_at(
            monitor.physical_position,
            monitor.physical_size.x,
            monitor.physical_size.y,
        );

        let mode = compute_effective_mode(&window, &monitor, &monitors);
        assert_eq!(
            mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Index(0))
        );
    }

    #[test]
    fn effective_mode_windowed_when_window_smaller_than_monitor() {
        let monitor = monitor_0();
        let monitors = monitors_with(monitor);
        let window = window_at(IVec2::new(100, 100), 1600, 1200);

        let mode = compute_effective_mode(&window, &monitor, &monitors);
        assert_eq!(mode, WindowMode::Windowed);
    }

    #[test]
    fn effective_mode_windowed_when_not_left_aligned() {
        let monitor = monitor_0();
        let monitors = monitors_with(monitor);
        // Full width + reaches bottom, but offset from left edge
        let window = window_at(
            IVec2::new(1, 0),
            monitor.physical_size.x,
            monitor.physical_size.y,
        );

        let mode = compute_effective_mode(&window, &monitor, &monitors);
        assert_eq!(mode, WindowMode::Windowed);
    }

    #[test]
    fn effective_mode_trusts_exclusive_fullscreen() {
        let monitor = monitor_0();
        let monitors = monitors_with(monitor);
        let mut window = window_at(IVec2::ZERO, 800, 600);
        window.mode =
            WindowMode::Fullscreen(MonitorSelection::Index(0), VideoModeSelection::Current);

        let mode = compute_effective_mode(&window, &monitor, &monitors);
        assert!(matches!(mode, WindowMode::Fullscreen(_, _)));
    }

    #[test]
    fn effective_mode_returns_mode_when_no_position() {
        let monitor = monitor_0();
        let monitors = monitors_with(monitor);
        let mut window = Window::default();
        window
            .resolution
            .set_physical_resolution(monitor.physical_size.x, monitor.physical_size.y);
        // position is Automatic (no position available, like Wayland)

        let mode = compute_effective_mode(&window, &monitor, &monitors);
        assert_eq!(mode, WindowMode::Windowed);
    }

    #[test]
    fn effective_mode_returns_mode_when_no_monitors() {
        let monitor = monitor_0();
        let empty = Monitors { list: vec![] };
        let window = window_at(
            IVec2::ZERO,
            monitor.physical_size.x,
            monitor.physical_size.y,
        );

        let mode = compute_effective_mode(&window, &monitor, &empty);
        assert_eq!(mode, WindowMode::Windowed);
    }
}
