//! Monitor management for window restoration.
//!
//! Provides a `Monitors` resource that maintains a sorted list of monitors,
//! automatically updated when monitors are added or removed.

use std::ops::Deref;

use bevy::prelude::*;
use bevy::window::Monitor;
use bevy::window::PrimaryWindow;
use bevy::window::WindowMode;
use bevy_diagnostic::FrameCount;
use bevy_kana::ToI32;

/// Plugin that manages the `Monitors` resource.
pub(crate) struct MonitorPlugin;

impl Plugin for MonitorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, init_monitors)
            .add_systems(Update, update_monitors);
    }
}

/// Information about a single monitor.
#[derive(Clone, Copy, Debug, Reflect)]
pub struct MonitorInfo {
    /// Index in the sorted monitor list.
    pub index:             usize,
    /// Scale factor (typically 1.0 or 2.0 on macOS).
    pub scale:             f64,
    /// Top-left corner of the monitor.
    pub physical_position: IVec2,
    /// Monitor dimensions in pixels.
    pub physical_size:     UVec2,
}

/// Sorted monitor list, updated when monitors change.
///
/// `Monitors` are sorted with primary (at 0,0) first, then by position.
#[derive(Resource, Reflect)]
#[reflect(Resource)]
pub struct Monitors {
    pub list: Vec<MonitorInfo>,
}

/// Component storing the current monitor and effective window mode.
///
/// This is the single source of truth for which monitor a window is on and its
/// effective display mode. Updated automatically by the plugin's unified monitor
/// detection system.
///
/// The `effective_mode` field reflects what the user actually sees, even when
/// `window.mode` is stale (e.g., macOS green button fullscreen reports `Windowed`).
///
/// Derefs to [`MonitorInfo`] for convenient access to monitor fields:
/// ```ignore
/// fn my_system(query: Query<(&Window, &CurrentMonitor), With<PrimaryWindow>>) {
///     let (window, monitor) = query.single();
///     println!("Monitor {} at scale {}, mode: {:?}", monitor.index, monitor.scale, monitor.effective_mode);
/// }
/// ```
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component)]
pub struct CurrentMonitor {
    /// The monitor this window is currently on.
    pub monitor_info:   MonitorInfo,
    /// The effective window mode, accounting for OS-level fullscreen changes.
    pub effective_mode: WindowMode,
}

impl Deref for CurrentMonitor {
    type Target = MonitorInfo;

    fn deref(&self) -> &Self::Target { &self.monitor_info }
}

impl Monitors {
    /// Find monitor containing position `(physical_x, physical_y)`.
    ///
    /// Coordinates are physical pixels — winit's monitor coordinate space.
    #[must_use]
    pub fn at(&self, physical_x: i32, physical_y: i32) -> Option<&MonitorInfo> {
        self.list.iter().find(|monitor| {
            physical_x >= monitor.physical_position.x
                && physical_x < monitor.physical_position.x + monitor.physical_size.x.to_i32()
                && physical_y >= monitor.physical_position.y
                && physical_y < monitor.physical_position.y + monitor.physical_size.y.to_i32()
        })
    }

    /// Get monitor by index in sorted list.
    #[must_use]
    pub fn by_index(&self, index: usize) -> Option<&MonitorInfo> { self.list.get(index) }

    /// Returns true if no monitors are available.
    ///
    /// This can happen when the laptop lid is closed or all displays are disconnected.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.list.is_empty() }

    /// Get the first monitor (index 0). Used as fallback when no specific monitor is known.
    ///
    /// # Panics
    ///
    /// Panics if no monitors exist (should never happen on a real system).
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "fail fast - no monitors means unrecoverable state"
    )]
    pub fn first(&self) -> &MonitorInfo {
        self.list
            .first()
            .expect("Monitors::first() requires at least one monitor")
    }

    /// Find the monitor a window is on, using window center for detection.
    ///
    /// Uses the center point to correctly handle windows spanning monitor boundaries
    /// and to avoid Windows invisible border offset (winit #4107).
    ///
    /// All inputs are physical pixels — winit's monitor coordinate space.
    #[must_use]
    pub fn monitor_for_window(
        &self,
        physical_position: IVec2,
        physical_width: u32,
        physical_height: u32,
    ) -> &MonitorInfo {
        let physical_center_x = physical_position.x + (physical_width / 2).to_i32();
        let physical_center_y = physical_position.y + (physical_height / 2).to_i32();
        self.closest_to(physical_center_x, physical_center_y)
    }

    /// Find the monitor at position, or the closest one if outside all bounds.
    ///
    /// Unlike [`at`](Self::at), this always returns a monitor by finding
    /// the closest monitor when position is outside all bounds.
    ///
    /// Coordinates are physical pixels — winit's monitor coordinate space.
    ///
    /// # Panics
    ///
    /// Panics if no monitors exist (should never happen on a real system).
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "fail fast - no monitors means unrecoverable state"
    )]
    pub fn closest_to(&self, physical_x: i32, physical_y: i32) -> &MonitorInfo {
        // Try exact match first
        if let Some(monitor) = self.at(physical_x, physical_y) {
            return monitor;
        }

        // Find closest monitor by distance to bounding box
        self.list
            .iter()
            .min_by_key(|monitor| {
                let physical_right = monitor.physical_position.x + monitor.physical_size.x.to_i32();
                let physical_bottom =
                    monitor.physical_position.y + monitor.physical_size.y.to_i32();

                let dx = if physical_x < monitor.physical_position.x {
                    monitor.physical_position.x - physical_x
                } else if physical_x >= physical_right {
                    physical_x - physical_right + 1
                } else {
                    0
                };

                let dy = if physical_y < monitor.physical_position.y {
                    monitor.physical_position.y - physical_y
                } else if physical_y >= physical_bottom {
                    physical_y - physical_bottom + 1
                } else {
                    0
                };

                dx * dx + dy * dy
            })
            .expect("Monitors::closest_to() requires at least one monitor")
    }
}

/// Build monitor list from query (preserves winit enumeration order).
fn build_monitors(monitors: &Query<&Monitor>) -> Monitors {
    let list: Vec<_> = monitors
        .iter()
        .enumerate()
        .map(|(idx, monitor)| MonitorInfo {
            index:             idx,
            scale:             monitor.scale_factor,
            physical_position: monitor.physical_position,
            physical_size:     monitor.physical_size(),
        })
        .collect();

    Monitors { list }
}

/// Initialize `Monitors` resource at startup.
pub(crate) fn init_monitors(mut commands: Commands, monitors: Query<&Monitor>) {
    let monitors_resource = build_monitors(&monitors);
    debug!(
        "[init_monitors] Found {} monitors",
        monitors_resource.list.len()
    );
    for monitor in &monitors_resource.list {
        debug!(
            "[init_monitors] Monitor {}: position=({}, {}) size={}x{} scale={}",
            monitor.index,
            monitor.physical_position.x,
            monitor.physical_position.y,
            monitor.physical_size.x,
            monitor.physical_size.y,
            monitor.scale
        );
    }
    commands.insert_resource(monitors_resource);
}

/// Update `Monitors` resource when monitors are added or removed.
fn update_monitors(
    mut commands: Commands,
    monitors: Query<&Monitor>,
    added: Query<Entity, Added<Monitor>>,
    mut removed: RemovedComponents<Monitor>,
    frame_count: Res<FrameCount>,
    current_monitor_query: Query<Option<&CurrentMonitor>, With<PrimaryWindow>>,
) {
    let has_changes = !added.is_empty() || removed.read().next().is_some();

    if has_changes {
        let monitors_resource = build_monitors(&monitors);
        let current = current_monitor_query.iter().next().flatten();
        if let Some(current_monitor) = current {
            debug!(
                "[update_monitors] frame={} Monitors changed, now {} monitors, current_monitor_index={} current_monitor_scale={}",
                frame_count.0,
                monitors_resource.list.len(),
                current_monitor.monitor_info.index,
                current_monitor.monitor_info.scale,
            );
        } else {
            debug!(
                "[update_monitors] frame={} Monitors changed, now {} monitors, current_monitor=None",
                frame_count.0,
                monitors_resource.list.len(),
            );
        }
        commands.insert_resource(monitors_resource);
    }
}
