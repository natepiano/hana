//! Monitor topology snapshots and raw lifetime events.

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::Monitor;
use bevy::window::PrimaryWindow;
use bevy::winit::WinitMonitors;
use bevy_diagnostic::FrameCount;
use bevy_kana::ToI32;

use super::CurrentMonitor;
use super::identity;
use super::identity::MonitorConfiguration;
use super::identity::MonitorConfigurationState;
use super::identity::MonitorId;
use super::identity::MonitorIdentificationError;
use super::identity::MonitorIdentity;
use super::identity::MonitorIdentityRegistry;
use super::identity::MonitorInstanceId;
use crate::Platform;

/// Entity-free information about one live monitor.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
#[type_path = "bevy_clerestory::monitors"]
pub struct MonitorInfo {
    /// Verified physical-panel identity, when qualified evidence is available.
    pub identity:          MonitorIdentity,
    /// Index in the monitor enumeration order.
    pub index:             usize,
    /// Scale factor (typically 1.0 or 2.0 on macOS).
    pub scale:             f64,
    /// Top-left corner of the monitor.
    pub physical_position: IVec2,
    /// Monitor dimensions in pixels.
    pub physical_size:     UVec2,
}

/// One current monitor entity and its entity-free metadata.
#[derive(Clone, Copy, Debug)]
pub struct LiveMonitor<'a> {
    /// Bevy monitor entity for the current monitor lifetime.
    pub entity:       Entity,
    /// Metadata for the current monitor lifetime.
    pub monitor_info: &'a MonitorInfo,
}

/// Current monitor topology in winit enumeration order.
#[derive(Resource, Reflect)]
#[reflect(Resource)]
#[type_path = "bevy_clerestory::monitors"]
pub struct Monitors {
    #[reflect(ignore)]
    live: Vec<MonitorSnapshot>,
}

/// A monitor entity lifetime started.
///
/// The event is emitted for verified and unverified monitors. The `entity`
/// field is valid only while the monitor remains present in [`Monitors`].
#[derive(Event, Debug, Clone, Reflect)]
#[type_path = "bevy_clerestory::monitors"]
pub struct MonitorConnected {
    /// Newly-created Bevy monitor entity.
    pub entity:  Entity,
    /// Entity-free metadata recorded in [`Monitors`].
    pub monitor: MonitorInfo,
}

/// A monitor entity lifetime ended.
///
/// `former_entity` identifies the ended lifetime and must not be retained as a
/// current monitor target.
#[derive(Event, Debug, Clone, Reflect)]
#[type_path = "bevy_clerestory::monitors"]
pub struct MonitorDisconnected {
    /// Bevy monitor entity from the ended lifetime.
    pub former_entity: Entity,
    /// Last entity-free metadata recorded for the monitor.
    pub monitor:       MonitorInfo,
}

impl Monitors {
    /// Iterate over all current monitor entities and their metadata.
    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = LiveMonitor<'_>> + '_ {
        self.live.iter().map(|monitor| LiveMonitor {
            entity:       monitor.entity,
            monitor_info: &monitor.monitor_info,
        })
    }

    /// Resolve one current monitor by an exact verified physical identity.
    #[must_use]
    pub fn by_id(&self, id: MonitorId) -> Option<&MonitorInfo> {
        self.unique_by_id(id).map(|monitor| &monitor.monitor_info)
    }

    /// Resolve one current monitor entity by an exact verified physical identity.
    #[must_use]
    pub fn entity_by_id(&self, id: MonitorId) -> Option<Entity> {
        self.unique_by_id(id).map(|monitor| monitor.entity)
    }

    /// Find the monitor containing position `(physical_x, physical_y)`.
    ///
    /// Coordinates are physical pixels in winit's monitor coordinate space.
    #[must_use]
    pub fn at(&self, physical_x: i32, physical_y: i32) -> Option<&MonitorInfo> {
        self.live
            .iter()
            .map(|monitor| &monitor.monitor_info)
            .find(|monitor_info| {
                physical_x >= monitor_info.physical_position.x
                    && physical_x
                        < monitor_info.physical_position.x + monitor_info.physical_size.x.to_i32()
                    && physical_y >= monitor_info.physical_position.y
                    && physical_y
                        < monitor_info.physical_position.y + monitor_info.physical_size.y.to_i32()
            })
    }

    /// Get a monitor by its current enumeration index.
    #[must_use]
    pub fn by_index(&self, index: usize) -> Option<&MonitorInfo> {
        self.live
            .iter()
            .map(|monitor| &monitor.monitor_info)
            .find(|monitor_info| monitor_info.index == index)
    }

    /// Return whether no monitors are available.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.live.is_empty() }

    /// Get the first monitor in current enumeration order.
    ///
    /// # Panics
    ///
    /// Panics if no monitors exist.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "fail fast - no monitors means unrecoverable state"
    )]
    pub fn first(&self) -> &MonitorInfo {
        &self
            .live
            .first()
            .expect("Monitors::first() requires at least one monitor")
            .monitor_info
    }

    /// Find the monitor a window is on, using the window center.
    ///
    /// All inputs are physical pixels in winit's monitor coordinate space.
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

    /// Find the monitor at a position, or the closest monitor to that position.
    ///
    /// # Panics
    ///
    /// Panics if no monitors exist.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "fail fast - no monitors means unrecoverable state"
    )]
    pub fn closest_to(&self, physical_x: i32, physical_y: i32) -> &MonitorInfo {
        if let Some(monitor_info) = self.at(physical_x, physical_y) {
            return monitor_info;
        }

        self.live
            .iter()
            .map(|monitor| &monitor.monitor_info)
            .min_by_key(|monitor_info| {
                let physical_right =
                    monitor_info.physical_position.x + monitor_info.physical_size.x.to_i32();
                let physical_bottom =
                    monitor_info.physical_position.y + monitor_info.physical_size.y.to_i32();

                let dx = if physical_x < monitor_info.physical_position.x {
                    monitor_info.physical_position.x - physical_x
                } else if physical_x >= physical_right {
                    physical_x - physical_right + 1
                } else {
                    0
                };

                let dy = if physical_y < monitor_info.physical_position.y {
                    monitor_info.physical_position.y - physical_y
                } else if physical_y >= physical_bottom {
                    physical_y - physical_bottom + 1
                } else {
                    0
                };

                dx * dx + dy * dy
            })
            .expect("Monitors::closest_to() requires at least one monitor")
    }

    fn unique_by_id(&self, id: MonitorId) -> Option<&MonitorSnapshot> {
        let mut matches = self
            .live
            .iter()
            .filter(|monitor| monitor.monitor_info.identity == MonitorIdentity::Verified(id));
        let monitor = matches.next()?;
        matches.next().is_none().then_some(monitor)
    }

    #[cfg(test)]
    pub(super) fn from_test_monitors(
        monitors: impl IntoIterator<Item = (Entity, MonitorInfo)>,
    ) -> Self {
        let live = monitors
            .into_iter()
            .map(|(entity, monitor_info)| MonitorSnapshot {
                instance_id: entity.into(),
                entity,
                monitor_info,
            })
            .collect();
        Self { live }
    }
}

#[derive(Clone, Copy, Debug)]
struct MonitorSnapshot {
    instance_id:  MonitorInstanceId,
    entity:       Entity,
    monitor_info: MonitorInfo,
}

fn monitor_deltas(
    previous: &Monitors,
    rebuilt: &Monitors,
) -> (Vec<MonitorSnapshot>, Vec<MonitorSnapshot>) {
    let connected = rebuilt
        .live
        .iter()
        .filter(|monitor| {
            !previous
                .live
                .iter()
                .any(|previous| previous.instance_id == monitor.instance_id)
        })
        .copied()
        .collect();
    let disconnected = previous
        .live
        .iter()
        .filter(|monitor| {
            !rebuilt
                .live
                .iter()
                .any(|rebuilt| rebuilt.instance_id == monitor.instance_id)
        })
        .copied()
        .collect();
    (connected, disconnected)
}

fn build_monitors(
    monitors: &Query<(Entity, &Monitor)>,
    winit_monitors: &WinitMonitors,
    identity_registry: &mut MonitorIdentityRegistry,
    configuration: MonitorConfigurationState,
    platform: Platform,
) -> Monitors {
    for (entity, _) in monitors.iter() {
        let instance_id = entity.into();
        identity_registry.identity(
            instance_id,
            configuration,
            || {
                let handle = winit_monitors
                    .find_entity(entity)
                    .ok_or(MonitorIdentificationError::MissingMonitorHandle)?;
                identity::qualified_evidence(&handle, platform)
            },
            platform,
        );
    }
    identity_registry.observe_configuration(configuration);

    let live = monitors
        .iter()
        .enumerate()
        .map(|(index, (entity, monitor))| {
            let instance_id = entity.into();
            let identity = identity_registry
                .cached_identity(instance_id)
                .unwrap_or(MonitorIdentity::Unverified);
            MonitorSnapshot {
                instance_id,
                entity,
                monitor_info: MonitorInfo {
                    identity,
                    index,
                    scale: monitor.scale_factor,
                    physical_position: monitor.physical_position,
                    physical_size: monitor.physical_size(),
                },
            }
        })
        .collect();

    Monitors { live }
}

/// Initialize the [`Monitors`] resource at startup.
pub(super) fn init_monitors(
    mut commands: Commands,
    monitors: Query<(Entity, &Monitor)>,
    winit_monitors: Res<WinitMonitors>,
    mut identity_registry: ResMut<MonitorIdentityRegistry>,
    configuration: Res<MonitorConfiguration>,
    platform: Res<Platform>,
    _: NonSendMarker,
) {
    let monitors_resource = build_monitors(
        &monitors,
        &winit_monitors,
        &mut identity_registry,
        configuration.state(),
        *platform,
    );
    debug!(
        "[init_monitors] Found {} monitors",
        monitors_resource.iter().len()
    );
    for monitor in monitors_resource.iter() {
        debug!(
            "[init_monitors] Monitor {}: entity={:?} identity={:?} position=({}, {}) size={}x{} scale={}",
            monitor.monitor_info.index,
            monitor.entity,
            monitor.monitor_info.identity,
            monitor.monitor_info.physical_position.x,
            monitor.monitor_info.physical_position.y,
            monitor.monitor_info.physical_size.x,
            monitor.monitor_info.physical_size.y,
            monitor.monitor_info.scale
        );
    }
    commands.insert_resource(monitors_resource);
}

/// Refresh [`Monitors`] after a native display-configuration notification or a
/// monitor lifetime change, and emit one raw event per changed lifetime.
pub(super) fn update_monitors(
    mut commands: Commands,
    monitors: Query<(Entity, &Monitor)>,
    winit_monitors: Res<WinitMonitors>,
    previous: Res<Monitors>,
    added: Query<Entity, Added<Monitor>>,
    mut removed: RemovedComponents<Monitor>,
    mut identity_registry: ResMut<MonitorIdentityRegistry>,
    configuration: Res<MonitorConfiguration>,
    platform: Res<Platform>,
    frame_count: Res<FrameCount>,
    current_monitor_query: Query<Option<&CurrentMonitor>, With<PrimaryWindow>>,
    _: NonSendMarker,
) {
    let configuration = configuration.state();
    let configuration_changed = identity_registry.configuration_changed(configuration);
    let monitor_was_removed = removed.read().count() > 0;
    if added.is_empty() && !monitor_was_removed && !configuration_changed {
        return;
    }

    for previous_monitor in &previous.live {
        if monitors.get(previous_monitor.entity).is_err() {
            identity_registry.disconnect(previous_monitor.instance_id);
        }
    }

    let rebuilt = build_monitors(
        &monitors,
        &winit_monitors,
        &mut identity_registry,
        configuration,
        *platform,
    );
    let (connected, disconnected) = monitor_deltas(&previous, &rebuilt);

    if let Some(current_monitor) = current_monitor_query.iter().next().flatten() {
        debug!(
            "[update_monitors] frame={} Monitors changed, now {} monitors, current_monitor_index={} current_monitor_scale={}",
            frame_count.0,
            rebuilt.iter().len(),
            current_monitor.monitor_info.index,
            current_monitor.monitor_info.scale,
        );
    } else {
        debug!(
            "[update_monitors] frame={} Monitors changed, now {} monitors, current_monitor=None",
            frame_count.0,
            rebuilt.iter().len(),
        );
    }

    commands.insert_resource(rebuilt);
    for monitor in connected {
        commands.trigger(MonitorConnected {
            entity:  monitor.entity,
            monitor: monitor.monitor_info,
        });
    }
    for monitor in disconnected {
        commands.trigger(MonitorDisconnected {
            former_entity: monitor.entity,
            monitor:       monitor.monitor_info,
        });
    }
}

#[cfg(test)]
mod tests {
    use bevy::reflect::TypePath;

    use super::*;

    const ENTITY_1: Entity = Entity::from_bits(1);
    const ENTITY_2: Entity = Entity::from_bits(2);
    const ENTITY_3: Entity = Entity::from_bits(3);
    const ENTITY_4: Entity = Entity::from_bits(4);

    fn monitor(identity: MonitorIdentity, index: usize, position: IVec2) -> MonitorInfo {
        MonitorInfo {
            identity,
            index,
            scale: 2.0,
            physical_position: position,
            physical_size: UVec2::new(2560, 1440),
        }
    }

    fn verified(raw: u64) -> MonitorIdentity { MonitorIdentity::Verified(MonitorId::from_raw(raw)) }

    #[test]
    fn reflected_type_paths_preserve_monitor_namespace() {
        assert_eq!(
            [
                <CurrentMonitor as TypePath>::type_path(),
                <MonitorId as TypePath>::type_path(),
                <MonitorIdentity as TypePath>::type_path(),
                <MonitorInfo as TypePath>::type_path(),
                <Monitors as TypePath>::type_path(),
                <MonitorConnected as TypePath>::type_path(),
                <MonitorDisconnected as TypePath>::type_path(),
            ],
            [
                "bevy_clerestory::monitors::CurrentMonitor",
                "bevy_clerestory::monitors::MonitorId",
                "bevy_clerestory::monitors::MonitorIdentity",
                "bevy_clerestory::monitors::MonitorInfo",
                "bevy_clerestory::monitors::Monitors",
                "bevy_clerestory::monitors::MonitorConnected",
                "bevy_clerestory::monitors::MonitorDisconnected",
            ]
        );
    }

    #[test]
    fn exact_identity_lookup_returns_metadata_and_entity() {
        let id = MonitorId::from_raw(7);
        let monitors = Monitors::from_test_monitors([
            (ENTITY_1, monitor(verified(7), 0, IVec2::ZERO)),
            (
                ENTITY_2,
                monitor(MonitorIdentity::Unverified, 1, IVec2::new(2560, 0)),
            ),
        ]);

        assert_eq!(monitors.by_id(id), monitors.by_index(0));
        assert_eq!(monitors.entity_by_id(id), Some(ENTITY_1));
        assert_eq!(monitors.iter().len(), 2);
    }

    #[test]
    fn identity_lookup_never_uses_geometry_index_or_first_monitor() {
        let monitors = Monitors::from_test_monitors([
            (
                ENTITY_1,
                monitor(MonitorIdentity::Unverified, 0, IVec2::ZERO),
            ),
            (ENTITY_2, monitor(verified(8), 1, IVec2::new(2560, 0))),
        ]);

        assert!(monitors.by_id(MonitorId::from_raw(7)).is_none());
        assert!(monitors.entity_by_id(MonitorId::from_raw(7)).is_none());
    }

    #[test]
    fn identity_lookup_rejects_multiple_live_exact_matches() {
        let id = MonitorId::from_raw(7);
        let monitors = Monitors::from_test_monitors([
            (ENTITY_1, monitor(verified(7), 0, IVec2::ZERO)),
            (ENTITY_2, monitor(verified(7), 1, IVec2::new(2560, 0))),
        ]);

        assert!(monitors.by_id(id).is_none());
        assert!(monitors.entity_by_id(id).is_none());
    }

    #[test]
    fn raw_deltas_include_verified_and_unverified_entities() {
        let previous = Monitors::from_test_monitors([
            (ENTITY_1, monitor(verified(1), 0, IVec2::ZERO)),
            (
                ENTITY_2,
                monitor(MonitorIdentity::Unverified, 1, IVec2::new(2560, 0)),
            ),
        ]);
        let rebuilt = Monitors::from_test_monitors([
            (ENTITY_3, monitor(verified(3), 0, IVec2::ZERO)),
            (
                ENTITY_4,
                monitor(MonitorIdentity::Unverified, 1, IVec2::new(2560, 0)),
            ),
        ]);

        let (connected, disconnected) = monitor_deltas(&previous, &rebuilt);
        let connected_verified = MonitorConnected {
            entity:  connected[0].entity,
            monitor: connected[0].monitor_info,
        };
        let connected_unverified = MonitorConnected {
            entity:  connected[1].entity,
            monitor: connected[1].monitor_info,
        };
        let disconnected_verified = MonitorDisconnected {
            former_entity: disconnected[0].entity,
            monitor:       disconnected[0].monitor_info,
        };
        let disconnected_unverified = MonitorDisconnected {
            former_entity: disconnected[1].entity,
            monitor:       disconnected[1].monitor_info,
        };

        assert_eq!(connected_verified.entity, ENTITY_3);
        assert_eq!(connected_verified.monitor.identity, verified(3));
        assert_eq!(connected_unverified.entity, ENTITY_4);
        assert_eq!(
            connected_unverified.monitor.identity,
            MonitorIdentity::Unverified
        );
        assert_eq!(disconnected_verified.former_entity, ENTITY_1);
        assert_eq!(disconnected_verified.monitor.identity, verified(1));
        assert_eq!(disconnected_unverified.former_entity, ENTITY_2);
        assert_eq!(
            disconnected_unverified.monitor.identity,
            MonitorIdentity::Unverified
        );
    }
}
