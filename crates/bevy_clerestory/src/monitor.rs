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
/// All platforms: computes `effective_window_mode` (handles macOS green button fullscreen).
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
            (Some(monitor_info), _, _) => (monitor_info, MONITOR_SOURCE_WINIT),
            (_, Some(monitor_info), _) => (monitor_info, MONITOR_SOURCE_POSITION),
            (_, _, Some(current_monitor)) => {
                (current_monitor.monitor_info, MONITOR_SOURCE_EXISTING)
            },
            _ => (*monitors.first(), MONITOR_SOURCE_FALLBACK),
        };

        let effective_window_mode = compute_effective_window_mode(window, &monitor_info, &monitors);

        let new_current = CurrentMonitor {
            monitor_info,
            effective_window_mode,
        };

        // `changed` prevents redundant `CurrentMonitor` inserts and Bevy change detection.
        let changed = existing.is_none_or(|current_monitor| {
            current_monitor.monitor_info.index != new_current.monitor_info.index
                || current_monitor.effective_window_mode != new_current.effective_window_mode
        });

        if changed {
            debug!(
                "[update_current_monitor] source={} index={} scale={} effective_window_mode={:?}",
                source, monitor_info.index, monitor_info.scale, effective_window_mode
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
fn compute_effective_window_mode(
    window: &Window,
    monitor_info: &MonitorInfo,
    monitors: &Monitors,
) -> WindowMode {
    // `WindowMode::Fullscreen` stays authoritative because the OS controls exclusive fullscreen.
    if matches!(window.mode, WindowMode::Fullscreen(_, _)) {
        return window.mode;
    }

    // An empty `Monitors` resource leaves no `MonitorInfo` for fullscreen inference.
    if monitors.is_empty() {
        return window.mode;
    }

    // `WindowPosition::Automatic` leaves no physical position, so `window.mode` stays
    // authoritative.
    let WindowPosition::At(physical_position) = window.position else {
        return window.mode;
    };

    // `full_width`, `left_aligned`, and `reaches_bottom` model macOS fullscreen detection.
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

    fn monitors_with(monitor_info: MonitorInfo) -> Monitors {
        Monitors {
            list: vec![monitor_info],
        }
    }

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
    fn effective_window_mode_fullscreen_when_window_fills_monitor() {
        let monitor_info = monitor_0();
        let monitors = monitors_with(monitor_info);
        let window = window_at(
            monitor_info.physical_position,
            monitor_info.physical_size.x,
            monitor_info.physical_size.y,
        );

        let effective_window_mode =
            compute_effective_window_mode(&window, &monitor_info, &monitors);
        assert_eq!(
            effective_window_mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Index(0))
        );
    }

    #[test]
    fn effective_window_mode_windowed_when_window_smaller_than_monitor() {
        let monitor_info = monitor_0();
        let monitors = monitors_with(monitor_info);
        let window = window_at(IVec2::new(100, 100), 1600, 1200);

        let effective_window_mode =
            compute_effective_window_mode(&window, &monitor_info, &monitors);
        assert_eq!(effective_window_mode, WindowMode::Windowed);
    }

    #[test]
    fn effective_window_mode_windowed_when_not_left_aligned() {
        let monitor_info = monitor_0();
        let monitors = monitors_with(monitor_info);
        // This `window_at` value sets `full_width` and `reaches_bottom` while
        // keeping `left_aligned` false.
        let window = window_at(
            IVec2::new(1, 0),
            monitor_info.physical_size.x,
            monitor_info.physical_size.y,
        );

        let effective_window_mode =
            compute_effective_window_mode(&window, &monitor_info, &monitors);
        assert_eq!(effective_window_mode, WindowMode::Windowed);
    }

    #[test]
    fn effective_window_mode_trusts_exclusive_fullscreen() {
        let monitor_info = monitor_0();
        let monitors = monitors_with(monitor_info);
        let mut window = window_at(IVec2::ZERO, 800, 600);
        window.mode =
            WindowMode::Fullscreen(MonitorSelection::Index(0), VideoModeSelection::Current);

        let effective_window_mode =
            compute_effective_window_mode(&window, &monitor_info, &monitors);
        assert!(matches!(
            effective_window_mode,
            WindowMode::Fullscreen(_, _)
        ));
    }

    #[test]
    fn effective_window_mode_returns_mode_when_no_position() {
        let monitor_info = monitor_0();
        let monitors = monitors_with(monitor_info);
        let mut window = Window::default();
        window
            .resolution
            .set_physical_resolution(monitor_info.physical_size.x, monitor_info.physical_size.y);
        // `WindowPosition::Automatic` leaves no position for `compute_effective_window_mode`.

        let effective_window_mode =
            compute_effective_window_mode(&window, &monitor_info, &monitors);
        assert_eq!(effective_window_mode, WindowMode::Windowed);
    }

    #[test]
    fn effective_window_mode_returns_mode_when_no_monitors() {
        let monitor_info = monitor_0();
        let empty = Monitors { list: vec![] };
        let window = window_at(
            IVec2::ZERO,
            monitor_info.physical_size.x,
            monitor_info.physical_size.y,
        );

        let effective_window_mode = compute_effective_window_mode(&window, &monitor_info, &empty);
        assert_eq!(effective_window_mode, WindowMode::Windowed);
    }
}
