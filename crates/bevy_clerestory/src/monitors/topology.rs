//! Monitor management for window restoration.
//!
//! Provides a `Monitors` resource that maintains a sorted list of monitors,
//! automatically updated when monitors are added or removed.

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::Monitor;
use bevy::window::PrimaryWindow;
use bevy::winit::WinitMonitors;
use bevy_diagnostic::FrameCount;
use bevy_kana::ToI32;

use super::CurrentMonitor;
use super::identity;
use super::identity::MonitorId;

/// Information about a single monitor.
#[derive(Clone, Copy, Debug, Reflect)]
#[type_path = "bevy_clerestory::monitors"]
pub struct MonitorInfo {
    /// Stable OS display id, unchanged across rearrangement.
    pub id:                MonitorId,
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
#[type_path = "bevy_clerestory::monitors"]
pub struct Monitors {
    /// Monitors in sort order: primary (at 0,0) first, then by position.
    pub list: Vec<MonitorInfo>,
}

/// A display was connected.
///
/// Triggered after [`Monitors`] is rebuilt to include it. Global rather than
/// entity-targeted because the payload is the identity of the changed display; a
/// consumer maps [`MonitorInfo::id`] to its own state.
///
/// Observe it:
/// ```ignore
/// app.add_observer(|connected: On<MonitorConnected>| {
///     let id = connected.event().monitor.id;
/// });
/// ```
///
/// A reconnect of the same physical display fires this again, so treat connect
/// as idempotent (resume, don't duplicate).
#[derive(Event, Debug, Clone, Reflect)]
#[type_path = "bevy_clerestory::monitors"]
pub struct MonitorConnected {
    /// The newly present monitor, as recorded in [`Monitors`].
    pub monitor: MonitorInfo,
}

/// A display was disconnected.
///
/// Triggered after [`Monitors`] is rebuilt without it. The payload comes from the
/// previous [`Monitors`] snapshot, since the winit `Monitor` entity and its handle
/// are gone by the time this fires.
#[derive(Event, Debug, Clone, Reflect)]
#[type_path = "bevy_clerestory::monitors"]
pub struct MonitorDisconnected {
    /// Geometry of the monitor as last known, before it vanished.
    pub monitor: MonitorInfo,
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
        if let Some(monitor) = self.at(physical_x, physical_y) {
            return monitor;
        }

        // `min_by_key` scores each `MonitorInfo` by squared distance to its
        // physical bounds.
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

/// Connect/disconnect deltas between two monitor snapshots, keyed on
/// [`MonitorId`]. Returns `(connected, disconnected)`: displays present only in
/// `rebuilt`, and displays present only in `previous`.
fn monitor_deltas(
    previous: &[MonitorInfo],
    rebuilt: &[MonitorInfo],
) -> (Vec<MonitorInfo>, Vec<MonitorInfo>) {
    let connected = rebuilt
        .iter()
        .filter(|monitor| !previous.iter().any(|prev| prev.id == monitor.id))
        .copied()
        .collect();
    let disconnected = previous
        .iter()
        .filter(|monitor| !rebuilt.iter().any(|next| next.id == monitor.id))
        .copied()
        .collect();
    (connected, disconnected)
}

/// Build monitor list from query (preserves winit enumeration order).
///
/// Each monitor's [`MonitorId`] comes from its winit `MonitorHandle` in
/// [`WinitMonitors`]; a monitor missing from that map (a broken winit invariant)
/// falls back to a hash of its physical position so distinct displays keep
/// distinct ids.
fn build_monitors(
    monitors: &Query<(Entity, &Monitor)>,
    winit_monitors: &WinitMonitors,
) -> Monitors {
    let list: Vec<_> = monitors
        .iter()
        .enumerate()
        .map(|(idx, (entity, monitor))| {
            let id = winit_monitors.find_entity(entity).map_or_else(
                || {
                    warn!(
                        "[build_monitors] no winit handle for monitor entity {entity}; keying id on position"
                    );
                    MonitorId(identity::hash_monitor_key((
                        monitor.physical_position.x,
                        monitor.physical_position.y,
                    )))
                },
                |handle| identity::native_monitor_id(&handle),
            );
            MonitorInfo {
                id,
                index: idx,
                scale: monitor.scale_factor,
                physical_position: monitor.physical_position,
                physical_size: monitor.physical_size(),
            }
        })
        .collect();

    Monitors { list }
}

/// Initialize `Monitors` resource at startup.
pub(crate) fn init_monitors(
    mut commands: Commands,
    monitors: Query<(Entity, &Monitor)>,
    winit_monitors: Res<WinitMonitors>,
    _: NonSendMarker,
) {
    let monitors_resource = build_monitors(&monitors, &winit_monitors);
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

/// Update `Monitors` resource when monitors are added or removed, and trigger a
/// [`MonitorConnected`] / [`MonitorDisconnected`] per changed display.
pub(super) fn update_monitors(
    mut commands: Commands,
    monitors: Query<(Entity, &Monitor)>,
    winit_monitors: Res<WinitMonitors>,
    previous: Res<Monitors>,
    added: Query<Entity, Added<Monitor>>,
    mut removed: RemovedComponents<Monitor>,
    frame_count: Res<FrameCount>,
    current_monitor_query: Query<Option<&CurrentMonitor>, With<PrimaryWindow>>,
    _: NonSendMarker,
) {
    let has_changes = !added.is_empty() || removed.read().next().is_some();
    if !has_changes {
        return;
    }

    let rebuilt = build_monitors(&monitors, &winit_monitors);

    // Diff before overwriting the resource: connect events read the rebuilt list,
    // disconnect events read the previous snapshot (the vanished monitor is
    // already gone from `monitors` and `winit_monitors`).
    let (connected, disconnected) = monitor_deltas(&previous.list, &rebuilt.list);
    for monitor in connected {
        commands.trigger(MonitorConnected { monitor });
    }
    for monitor in disconnected {
        commands.trigger(MonitorDisconnected { monitor });
    }

    if let Some(current_monitor) = current_monitor_query.iter().next().flatten() {
        debug!(
            "[update_monitors] frame={} Monitors changed, now {} monitors, current_monitor_index={} current_monitor_scale={}",
            frame_count.0,
            rebuilt.list.len(),
            current_monitor.monitor_info.index,
            current_monitor.monitor_info.scale,
        );
    } else {
        debug!(
            "[update_monitors] frame={} Monitors changed, now {} monitors, current_monitor=None",
            frame_count.0,
            rebuilt.list.len(),
        );
    }
    commands.insert_resource(rebuilt);
}

#[cfg(test)]
mod tests {
    use bevy::reflect::TypePath;

    use super::*;

    fn monitor(id: u64, position: IVec2) -> MonitorInfo {
        MonitorInfo {
            id:                MonitorId(id),
            index:             0,
            scale:             2.0,
            physical_position: position,
            physical_size:     UVec2::new(2560, 1440),
        }
    }

    #[test]
    fn reflected_type_paths_preserve_legacy_monitor_module() {
        assert_eq!(
            [
                <CurrentMonitor as TypePath>::type_path(),
                <MonitorId as TypePath>::type_path(),
                <MonitorInfo as TypePath>::type_path(),
                <Monitors as TypePath>::type_path(),
                <MonitorConnected as TypePath>::type_path(),
                <MonitorDisconnected as TypePath>::type_path(),
            ],
            [
                "bevy_clerestory::monitors::CurrentMonitor",
                "bevy_clerestory::monitors::MonitorId",
                "bevy_clerestory::monitors::MonitorInfo",
                "bevy_clerestory::monitors::Monitors",
                "bevy_clerestory::monitors::MonitorConnected",
                "bevy_clerestory::monitors::MonitorDisconnected",
            ]
        );
    }

    #[test]
    fn deltas_report_new_display_as_connected() {
        let previous = vec![monitor(1, IVec2::ZERO)];
        let rebuilt = vec![monitor(1, IVec2::ZERO), monitor(2, IVec2::new(2560, 0))];

        let (connected, disconnected) = monitor_deltas(&previous, &rebuilt);

        assert_eq!(connected.len(), 1);
        assert_eq!(connected[0].id, MonitorId(2));
        assert!(disconnected.is_empty());
    }

    #[test]
    fn deltas_report_vanished_display_as_disconnected() {
        let previous = vec![monitor(1, IVec2::ZERO), monitor(2, IVec2::new(2560, 0))];
        let rebuilt = vec![monitor(1, IVec2::ZERO)];

        let (connected, disconnected) = monitor_deltas(&previous, &rebuilt);

        assert!(connected.is_empty());
        assert_eq!(disconnected.len(), 1);
        assert_eq!(disconnected[0].id, MonitorId(2));
    }

    #[test]
    fn deltas_ignore_rearrangement_of_same_ids() {
        // Same displays, rearranged: positions differ but ids match, so no
        // connect/disconnect fires.
        let previous = vec![monitor(1, IVec2::ZERO), monitor(2, IVec2::new(2560, 0))];
        let rebuilt = vec![monitor(1, IVec2::new(2560, 0)), monitor(2, IVec2::ZERO)];

        let (connected, disconnected) = monitor_deltas(&previous, &rebuilt);

        assert!(connected.is_empty());
        assert!(disconnected.is_empty());
    }

    #[test]
    fn deltas_report_swap_as_one_each() {
        let previous = vec![monitor(1, IVec2::ZERO)];
        let rebuilt = vec![monitor(2, IVec2::ZERO)];

        let (connected, disconnected) = monitor_deltas(&previous, &rebuilt);

        assert_eq!(connected.len(), 1);
        assert_eq!(connected[0].id, MonitorId(2));
        assert_eq!(disconnected.len(), 1);
        assert_eq!(disconnected[0].id, MonitorId(1));
    }
}
