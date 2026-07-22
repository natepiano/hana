//! Monitor topology snapshots and raw lifetime events.

#[cfg(all(test, feature = "monitor-probe"))]
mod example_probe {
    pub(super) mod constants {
        pub(crate) const FIELD_MONITOR: &str = "monitor";
        pub(crate) const FIELD_MONITOR_ENTITY: &str = "monitor_entity";
        pub(crate) const FIELD_TOPOLOGY_REVISION: &str = "topology_revision";
        pub(crate) const FIELD_TRANSITION: &str = "transition";
        pub(crate) const KIND_MONITOR_CONNECTED: &str = "monitor-connected";
        pub(crate) const KIND_MONITOR_DISCONNECTED: &str = "monitor-disconnected";
        pub(crate) const KIND_MONITOR_TOPOLOGY: &str = "monitor-topology";
        pub(crate) const KIND_RECOVERY_ACCEPTED: &str = "recovery-accepted";
        pub(crate) const MONITOR_PROBE_TARGET: &str = "bevy_clerestory::monitor_probe";
        pub(crate) const PRODUCER_MONITOR_CONNECTED: &str = "observer::MonitorConnected";
        pub(crate) const PRODUCER_MONITOR_DISCONNECTED: &str = "observer::MonitorDisconnected";
        pub(crate) const RECOVERY_PROBE_TARGET: &str = "bevy_clerestory::recovery_probe";
        pub(crate) const TRACE_FIELD_FRAME_COUNT: &str = "frame_count";
        pub(crate) const TRACE_FIELD_PRODUCER_SCHEDULE: &str = "producer_schedule";
        pub(crate) const TRANSITION_CREATED: &str = "created";
        pub(crate) const TRANSITION_REMOVED: &str = "removed";
    }

    pub(super) mod trace {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/restore_after_reconnect/trace.rs"
        ));
    }
}

#[cfg(test)]
use std::collections::HashMap;
use std::collections::HashSet;

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::Monitor;
use bevy::winit::WinitMonitors;
#[cfg(feature = "monitor-probe")]
use bevy_diagnostic::FrameCount;
use bevy_kana::ToI32;
use winit::monitor::MonitorHandle;

use super::current_monitor;
use super::identity;
use super::identity::MonitorConfiguration;
use super::identity::MonitorConfigurationState;
use super::identity::MonitorId;
use super::identity::MonitorIdentificationError;
use super::identity::MonitorIdentity;
use super::identity::MonitorIdentityRegistry;
use super::identity::MonitorInstanceId;
#[cfg(test)]
use super::identity::QualifiedEvidence;
#[cfg(feature = "monitor-probe")]
use super::monitor_probe;
#[cfg(feature = "monitor-probe")]
use super::monitor_probe::FormerIdentityProbe;
#[cfg(feature = "monitor-probe")]
use super::monitor_probe::TopologyChangeKind;
#[cfg(feature = "monitor-probe")]
use super::monitor_probe::TopologyProbeRecord;
#[cfg(feature = "monitor-probe")]
use super::monitor_probe::TopologyProducerSchedule;
use crate::Platform;

/// Entity-free information about one live monitor.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
#[type_path = "bevy_clerestory::monitors"]
pub struct MonitorInfo {
    /// Verified physical-panel identity, when qualified evidence is available.
    pub identity:          MonitorIdentity,
    /// Index in Bevy's cached `WinitMonitors` order for this monitor lifetime.
    pub index:             usize,
    /// Scale factor supplied by the current Bevy monitor entity.
    pub scale:             f64,
    /// Top-left corner supplied by the current Bevy monitor entity.
    pub physical_position: IVec2,
    /// Monitor dimensions in pixels supplied by the current Bevy monitor entity.
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

/// Monotonic version of the installed entity and identity topology.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Resource, Reflect)]
#[reflect(Resource)]
#[type_path = "bevy_clerestory::monitors"]
pub struct MonitorTopologyRevision(u64);

impl MonitorTopologyRevision {
    /// Return the installed revision number.
    #[must_use]
    pub const fn get(self) -> u64 { self.0 }

    const fn next(self) -> Self { Self(self.0 + 1) }

    #[cfg(test)]
    pub(crate) const fn from_test_raw(revision: u64) -> Self { Self(revision) }
}

/// Last installed monitor topology in Bevy's cached `WinitMonitors` order.
#[derive(Resource, Reflect)]
#[reflect(Resource)]
#[type_path = "bevy_clerestory::monitors"]
pub struct Monitors {
    #[reflect(ignore)]
    pub(super) live: Vec<MonitorSnapshot>,
}

/// A monitor entity lifetime started.
///
/// The event is emitted for verified and unverified monitors. The `entity`
/// field is valid only while the monitor remains present in [`Monitors`].
#[derive(Event, Debug, Clone, Reflect)]
#[reflect(Event)]
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
#[reflect(Event)]
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
    pub(crate) fn from_test_monitors(
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MonitorSnapshot {
    pub(super) instance_id:  MonitorInstanceId,
    pub(super) entity:       Entity,
    pub(super) monitor_info: MonitorInfo,
}

struct ScannedMonitor {
    entity:            Entity,
    cached_index:      Option<usize>,
    handle:            Option<MonitorHandle>,
    index:             usize,
    scale:             f64,
    physical_position: IVec2,
    physical_size:     UVec2,
}

pub(super) struct MonitorChanges {
    pub(super) connected:        Vec<MonitorSnapshot>,
    pub(super) disconnected:     Vec<MonitorSnapshot>,
    #[cfg(feature = "monitor-probe")]
    pub(super) identity_changed: Vec<MonitorSnapshot>,
}

struct MonitorBuild {
    monitors:               Monitors,
    #[cfg(feature = "monitor-probe")]
    former_identity_probes: Vec<FormerIdentityProbe>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TopologyProducerActivity {
    pub topology_scans:       usize,
    pub component_reads:      usize,
    pub cached_order_lookups: usize,
    pub identity_requests:    usize,
    pub handle_lookups:       usize,
    pub evidence_loads:       usize,
}

#[cfg(test)]
#[derive(Default, Resource)]
pub(super) struct InjectedMonitorEvidence {
    evidence:     HashMap<Entity, Result<&'static [u8], MonitorIdentificationError>>,
    pub activity: TopologyProducerActivity,
}

#[cfg(test)]
#[derive(Default, Resource)]
pub(super) struct InjectedWinitMonitorOrder {
    entities: Vec<Entity>,
}

#[cfg(test)]
impl InjectedWinitMonitorOrder {
    fn connect(&mut self, entity: Entity) { self.entities.push(entity); }

    fn disconnect(&mut self, entity: Entity) { self.entities.retain(|cached| *cached != entity); }

    fn replace(&mut self, entities: impl IntoIterator<Item = Entity>) {
        self.entities = entities.into_iter().collect();
    }
}

fn monitor_changes(previous: &Monitors, rebuilt: &Monitors) -> MonitorChanges {
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

    #[cfg(feature = "monitor-probe")]
    let identity_changed = rebuilt
        .live
        .iter()
        .filter(|monitor| {
            previous.live.iter().any(|previous| {
                previous.instance_id == monitor.instance_id
                    && previous.monitor_info.identity != monitor.monitor_info.identity
            })
        })
        .copied()
        .collect();

    MonitorChanges {
        connected,
        disconnected,
        #[cfg(feature = "monitor-probe")]
        identity_changed,
    }
}

fn installed_topology_changed(previous: &Monitors, rebuilt: &Monitors) -> bool {
    previous.live.len() != rebuilt.live.len()
        || previous.live.iter().any(|previous_monitor| {
            rebuilt
                .live
                .iter()
                .find(|rebuilt_monitor| rebuilt_monitor.instance_id == previous_monitor.instance_id)
                .is_none_or(|rebuilt_monitor| {
                    rebuilt_monitor.entity != previous_monitor.entity
                        || rebuilt_monitor.monitor_info.identity
                            != previous_monitor.monitor_info.identity
                        || rebuilt_monitor.monitor_info.index != previous_monitor.monitor_info.index
                })
        })
}

fn assign_cached_monitor_order(
    scanned: &mut [ScannedMonitor],
    winit_monitors: &WinitMonitors,
    #[cfg(test)] mut injected_evidence: Option<&mut InjectedMonitorEvidence>,
    #[cfg(test)] injected_order: Option<&InjectedWinitMonitorOrder>,
) {
    #[cfg(test)]
    if let Some(injected_order) = injected_order {
        for monitor in &mut *scanned {
            if let Some(injected_evidence) = injected_evidence.as_deref_mut() {
                injected_evidence.activity.cached_order_lookups += 1;
            }
            monitor.cached_index = injected_order
                .entities
                .iter()
                .position(|entity| *entity == monitor.entity);
        }
        assign_unassociated_indices(scanned, injected_order.entities.len());
        return;
    }

    let cached_handles: Vec<_> = (0..).map_while(|index| winit_monitors.nth(index)).collect();
    for monitor in &mut *scanned {
        #[cfg(test)]
        if let Some(injected_evidence) = injected_evidence.as_deref_mut() {
            injected_evidence.activity.cached_order_lookups += 1;
        }
        monitor.handle = winit_monitors.find_entity(monitor.entity);
        monitor.cached_index = monitor.handle.as_ref().and_then(|handle| {
            cached_handles
                .iter()
                .position(|cached_handle| cached_handle == handle)
        });
    }
    assign_unassociated_indices(scanned, cached_handles.len());
}

fn assign_unassociated_indices(scanned: &mut [ScannedMonitor], cached_monitor_count: usize) {
    scanned.sort_by_key(|monitor| {
        (
            monitor.cached_index.is_none(),
            monitor.cached_index.unwrap_or_default(),
            monitor.entity.to_bits(),
        )
    });

    let mut unassociated_index = cached_monitor_count;
    for monitor in scanned {
        monitor.index = monitor.cached_index.unwrap_or_else(|| {
            let index = unassociated_index;
            unassociated_index += 1;
            index
        });
    }
}

fn identify_monitor(
    monitor: &ScannedMonitor,
    identity_registry: &mut MonitorIdentityRegistry,
    configuration: MonitorConfigurationState,
    platform: Platform,
    #[cfg(test)] injected_evidence: Option<&mut InjectedMonitorEvidence>,
) {
    let instance_id = monitor.entity.into();

    #[cfg(test)]
    if let Some(injected_evidence) = injected_evidence {
        injected_evidence.activity.identity_requests += 1;
        if monitor.cached_index.is_none() {
            injected_evidence.activity.handle_lookups += 1;
            identity::monitor_handle_missing(identity_registry, instance_id, configuration);
            return;
        }
        let evidence = injected_evidence
            .evidence
            .get(&monitor.entity)
            .copied()
            .unwrap_or(Err(MonitorIdentificationError::MissingMonitorHandle));
        identity_registry.identity(
            instance_id,
            configuration,
            || {
                injected_evidence.activity.handle_lookups += 1;
                injected_evidence.activity.evidence_loads += 1;
                evidence.map(|bytes| QualifiedEvidence::Synthetic(bytes.to_vec()))
            },
            platform,
        );
        return;
    }

    if monitor.cached_index.is_none() {
        identity::monitor_handle_missing(identity_registry, instance_id, configuration);
        return;
    }

    identity_registry.identity(
        instance_id,
        configuration,
        || {
            let handle = monitor
                .handle
                .as_ref()
                .ok_or(MonitorIdentificationError::MissingMonitorHandle)?;
            identity::qualified_evidence(handle, platform)
        },
        platform,
    );
}

fn build_monitors(
    monitors: &Query<(Entity, &Monitor)>,
    winit_monitors: &WinitMonitors,
    identity_registry: &mut MonitorIdentityRegistry,
    configuration: MonitorConfigurationState,
    platform: Platform,
    #[cfg(test)] mut injected_evidence: Option<&mut InjectedMonitorEvidence>,
    #[cfg(test)] injected_order: Option<&InjectedWinitMonitorOrder>,
) -> MonitorBuild {
    #[cfg(test)]
    if let Some(injected_evidence) = injected_evidence.as_deref_mut() {
        injected_evidence.activity.topology_scans += 1;
    }

    let mut scanned = Vec::new();
    for (entity, monitor) in monitors.iter() {
        #[cfg(test)]
        if let Some(injected_evidence) = injected_evidence.as_deref_mut() {
            injected_evidence.activity.component_reads += 1;
        }
        scanned.push(ScannedMonitor {
            entity,
            cached_index: None,
            handle: None,
            index: usize::MAX,
            scale: monitor.scale_factor,
            physical_position: monitor.physical_position,
            physical_size: monitor.physical_size(),
        });
    }
    assign_cached_monitor_order(
        &mut scanned,
        winit_monitors,
        #[cfg(test)]
        injected_evidence.as_deref_mut(),
        #[cfg(test)]
        injected_order,
    );

    let live_instances: HashSet<_> = scanned
        .iter()
        .map(|monitor| MonitorInstanceId::from(monitor.entity))
        .collect();
    let disconnected_instances: Vec<_> = identity_registry
        .active_instances()
        .filter(|instance_id| !live_instances.contains(instance_id))
        .collect();
    #[cfg(feature = "monitor-probe")]
    let former_identity_probes = disconnected_instances
        .iter()
        .map(|instance_id| FormerIdentityProbe {
            instance_id:    *instance_id,
            identity_probe: identity_registry.probe(*instance_id),
        })
        .collect();
    for instance_id in disconnected_instances {
        identity_registry.disconnect(instance_id);
    }

    for monitor in &scanned {
        identify_monitor(
            monitor,
            identity_registry,
            configuration,
            platform,
            #[cfg(test)]
            injected_evidence.as_deref_mut(),
        );
    }

    let live = scanned
        .into_iter()
        .map(|monitor| {
            let identity = identity::cached_identity(identity_registry, monitor.entity.into())
                .unwrap_or(MonitorIdentity::Unverified);
            MonitorSnapshot {
                instance_id:  monitor.entity.into(),
                entity:       monitor.entity,
                monitor_info: MonitorInfo {
                    identity,
                    index: monitor.index,
                    scale: monitor.scale,
                    physical_position: monitor.physical_position,
                    physical_size: monitor.physical_size,
                },
            }
        })
        .collect();
    identity_registry.observe_configuration(configuration);

    MonitorBuild {
        monitors: Monitors { live },
        #[cfg(feature = "monitor-probe")]
        former_identity_probes,
    }
}

fn queue_topology_install(
    commands: &mut Commands,
    rebuilt: Monitors,
    revision: MonitorTopologyRevision,
    changes: MonitorChanges,
    #[cfg(feature = "monitor-probe")] probe_records: Vec<TopologyProbeRecord>,
) {
    let topology_is_empty = rebuilt.is_empty();
    commands.queue(move |world: &mut World| {
        world.insert_resource(rebuilt);
        world.insert_resource(revision);
        if topology_is_empty {
            current_monitor::remove_current_monitors_for_empty_topology(world);
        }
        #[cfg(feature = "monitor-probe")]
        for record in probe_records {
            #[cfg(test)]
            monitor_probe::capture_record(world, &record);
            record.emit();
        }
        for monitor in changes.connected {
            world.trigger(MonitorConnected {
                entity:  monitor.entity,
                monitor: monitor.monitor_info,
            });
        }
        for monitor in changes.disconnected {
            world.trigger(MonitorDisconnected {
                former_entity: monitor.entity,
                monitor:       monitor.monitor_info,
            });
        }
    });
}

/// Initialize the [`Monitors`] resource at startup.
pub(super) fn init_monitors(
    mut commands: Commands,
    monitors: Query<(Entity, &Monitor)>,
    winit_monitors: Res<WinitMonitors>,
    mut identity_registry: ResMut<MonitorIdentityRegistry>,
    configuration: Res<MonitorConfiguration>,
    platform: Res<Platform>,
    #[cfg(feature = "monitor-probe")] frame_count: Res<FrameCount>,
    #[cfg(test)] mut injected_evidence: Option<ResMut<InjectedMonitorEvidence>>,
    #[cfg(test)] injected_order: Option<Res<InjectedWinitMonitorOrder>>,
    _: NonSendMarker,
) {
    let configuration = configuration.state();
    let monitor_build = build_monitors(
        &monitors,
        &winit_monitors,
        &mut identity_registry,
        configuration,
        *platform,
        #[cfg(test)]
        injected_evidence.as_deref_mut(),
        #[cfg(test)]
        injected_order.as_deref(),
    );
    debug!(
        "[init_monitors] Found {} monitors",
        monitor_build.monitors.iter().len()
    );
    let revision = MonitorTopologyRevision::default();
    let changes = monitor_changes(&Monitors { live: Vec::new() }, &monitor_build.monitors);
    #[cfg(feature = "monitor-probe")]
    let probe_records = monitor_probe::changed_probe_records(
        &changes,
        &monitor_build.former_identity_probes,
        &identity_registry,
        frame_count.0,
        TopologyProducerSchedule::PreStartup,
        configuration,
        revision,
    );
    queue_topology_install(
        &mut commands,
        monitor_build.monitors,
        revision,
        changes,
        #[cfg(feature = "monitor-probe")]
        probe_records,
    );
}

/// Revalidate and install monitor entity-lifetime topology changes.
pub(super) fn update_monitors(
    mut commands: Commands,
    monitors: Query<(Entity, &Monitor)>,
    winit_monitors: Res<WinitMonitors>,
    added_monitors: Query<(), Added<Monitor>>,
    mut removed_monitors: RemovedComponents<Monitor>,
    previous: Res<Monitors>,
    revision: Res<MonitorTopologyRevision>,
    mut identity_registry: ResMut<MonitorIdentityRegistry>,
    configuration: Res<MonitorConfiguration>,
    platform: Res<Platform>,
    #[cfg(feature = "monitor-probe")] frame_count: Res<FrameCount>,
    #[cfg(test)] mut injected_evidence: Option<ResMut<InjectedMonitorEvidence>>,
    #[cfg(test)] injected_order: Option<Res<InjectedWinitMonitorOrder>>,
    _: NonSendMarker,
) {
    let configuration = configuration.state();
    let configuration_changed = identity_registry.configuration_changed(configuration);
    let monitor_added = !added_monitors.is_empty();
    let monitor_removed = removed_monitors.read().count() != 0;
    if !monitor_added && !monitor_removed && !configuration_changed {
        return;
    }

    let monitor_build = build_monitors(
        &monitors,
        &winit_monitors,
        &mut identity_registry,
        configuration,
        *platform,
        #[cfg(test)]
        injected_evidence.as_deref_mut(),
        #[cfg(test)]
        injected_order.as_deref(),
    );
    let rebuilt = monitor_build.monitors;
    if !installed_topology_changed(&previous, &rebuilt) {
        if configuration_changed {
            #[cfg(feature = "monitor-probe")]
            for record in monitor_probe::current_probe_records(
                &previous,
                &identity_registry,
                frame_count.0,
                TopologyProducerSchedule::Update,
                configuration,
                *revision,
                TopologyChangeKind::RevalidatedUnchanged,
            ) {
                record.emit();
            }
        }
        return;
    }

    let revision = revision.next();
    let changes = monitor_changes(&previous, &rebuilt);
    debug!(
        "[update_monitors] installed revision={} with {} monitors",
        revision.get(),
        rebuilt.iter().len(),
    );
    #[cfg(feature = "monitor-probe")]
    let probe_records = monitor_probe::changed_probe_records(
        &changes,
        &monitor_build.former_identity_probes,
        &identity_registry,
        frame_count.0,
        TopologyProducerSchedule::Update,
        configuration,
        revision,
    );
    queue_topology_install(
        &mut commands,
        rebuilt,
        revision,
        changes,
        #[cfg(feature = "monitor-probe")]
        probe_records,
    );
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    use std::fs;
    use std::path::Path;
    use std::time::Duration;

    use bevy::ecs::reflect::AppTypeRegistry;
    use bevy::ecs::reflect::ReflectEvent;
    #[cfg(feature = "monitor-probe")]
    use bevy::log::tracing_subscriber::Registry;
    #[cfg(feature = "monitor-probe")]
    use bevy::log::tracing_subscriber::prelude::*;
    use bevy::reflect::TypePath;
    use bevy::reflect::TypeRegistry;
    use bevy::time::TimePlugin;
    use bevy::time::TimeUpdateStrategy;
    use bevy::window::MonitorSelection;
    use bevy::window::OnMonitor;
    use bevy::window::PrimaryWindow;
    use bevy::window::WindowMode;
    use bevy::window::WindowPosition;
    use tempfile::NamedTempFile;

    #[cfg(feature = "monitor-probe")]
    use super::example_probe::trace as example_trace;
    #[cfg(feature = "monitor-probe")]
    use super::example_probe::trace::ProbeTrace;
    #[cfg(feature = "monitor-probe")]
    use super::example_probe::trace::TraceRecord;
    #[cfg(feature = "monitor-probe")]
    use super::monitor_probe::InjectedTopologyProbeRecords;
    use super::*;
    use crate::ClerestoryPreStartupSet;
    use crate::ClerestoryUpdateSet;
    use crate::ManagedWindow;
    use crate::ManagedWindowPersistence;
    use crate::WindowRecovery;
    use crate::WindowRestoreMismatch;
    use crate::WindowRestored;
    use crate::constants::SCALE_FACTOR_EPSILON;
    use crate::constants::SETTLE_STABILITY_SECS;
    use crate::managed;
    use crate::managed::ManagedWindowRegistry;
    use crate::monitors::CurrentMonitor;
    use crate::monitors::current_monitor::InjectedCurrentMonitorSource;
    use crate::monitors::identity::OperatingSystemQueryError;
    use crate::persistence::CapturedWindowPlacement;
    use crate::persistence::CapturedWindowStates;
    use crate::persistence::InjectedWindowPositions;
    use crate::persistence::PersistencePlugin;
    use crate::persistence::PersistenceWriteState;
    use crate::persistence::WindowKey;
    use crate::recovery;
    use crate::recovery::FallbackAndReturnPhaseSnapshot;
    use crate::recovery::RecoveryPlugin;
    use crate::recovery::RecoveryRegistrations;
    use crate::restore;
    use crate::restore::NativeWindowReady;
    use crate::restore_window_config::RestoreWindowConfig;

    const ENTITY_1: Entity = Entity::from_bits(1);
    const ENTITY_2: Entity = Entity::from_bits(2);
    const ENTITY_3: Entity = Entity::from_bits(3);
    const ENTITY_4: Entity = Entity::from_bits(4);
    const DEFAULT_SCALE: f64 = 2.0;
    const LOW_DPI_SCALE: f64 = 1.0;
    const MONITOR_HEIGHT: u32 = 1_440;
    const MONITOR_WIDTH: u32 = 2_560;
    const PANEL_A_EVIDENCE: &[u8] = b"panel-a";
    const PANEL_B_EVIDENCE: &[u8] = b"panel-b";
    const RETURNED_MONITOR_POSITION: IVec2 = IVec2::new(3_840, 120);
    const RETURNED_MONITOR_SIZE: UVec2 = UVec2::new(3_840, 2_160);
    #[cfg(feature = "monitor-probe")]
    const RUNTIME_TRACE_FRAME: u32 = 9;
    #[cfg(feature = "monitor-probe")]
    const SCHEDULE_PRE_STARTUP_MONITORS: &str = "PreStartup::init_monitors";
    #[cfg(feature = "monitor-probe")]
    const SCHEDULE_UPDATE_MONITORS: &str = "Update::monitor_topology_producer";

    #[derive(Default, Resource)]
    struct TopologyObservations {
        connected:                      Vec<(Entity, MonitorTopologyRevision)>,
        connected_indices:              Vec<(Entity, usize)>,
        #[cfg(feature = "monitor-probe")]
        probe_records_at_connect:       Vec<usize>,
        disconnected:                   Vec<(Entity, MonitorTopologyRevision)>,
        disconnected_indices:           Vec<(Entity, usize)>,
        current_monitors_at_disconnect: Vec<usize>,
    }

    #[derive(Default, Resource)]
    struct DownstreamObservations {
        restore_current_monitors: usize,
        settle_current_monitors:  usize,
    }

    #[derive(Default, Resource)]
    struct RecoveryObservations {
        restored:   usize,
        mismatched: usize,
    }

    #[derive(Resource)]
    struct ScheduledMonitorAssociation {
        window_entity:  Entity,
        monitor_entity: Entity,
    }

    fn monitor(identity: MonitorIdentity, index: usize, position: IVec2) -> MonitorInfo {
        MonitorInfo {
            identity,
            index,
            scale: DEFAULT_SCALE,
            physical_position: position,
            physical_size: UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
        }
    }

    fn verified(raw: u64) -> MonitorIdentity { MonitorIdentity::Verified(MonitorId::from_raw(raw)) }

    fn monitor_component(position: IVec2, size: UVec2, scale: f64) -> Monitor {
        Monitor {
            name:                    None,
            physical_height:         size.y,
            physical_width:          size.x,
            physical_position:       position,
            refresh_rate_millihertz: None,
            scale_factor:            scale,
            video_modes:             Vec::new(),
        }
    }

    fn observe_connected(
        event: On<MonitorConnected>,
        monitors: Res<Monitors>,
        revision: Res<MonitorTopologyRevision>,
        #[cfg(feature = "monitor-probe")] probe_records: Res<InjectedTopologyProbeRecords>,
        mut observations: ResMut<TopologyObservations>,
    ) {
        let installed = monitors
            .iter()
            .find(|monitor| monitor.entity == event.entity)
            .map(|monitor| *monitor.monitor_info);
        assert_eq!(installed, Some(event.monitor));
        observations.connected.push((event.entity, *revision));
        observations
            .connected_indices
            .push((event.entity, event.monitor.index));
        #[cfg(feature = "monitor-probe")]
        observations
            .probe_records_at_connect
            .push(probe_records.records.len());
    }

    fn observe_disconnected(
        event: On<MonitorDisconnected>,
        monitors: Res<Monitors>,
        revision: Res<MonitorTopologyRevision>,
        windows: Query<&CurrentMonitor, Or<(With<PrimaryWindow>, With<ManagedWindow>)>>,
        mut observations: ResMut<TopologyObservations>,
    ) {
        assert!(
            monitors
                .iter()
                .all(|monitor| monitor.entity != event.former_entity)
        );
        observations
            .disconnected
            .push((event.former_entity, *revision));
        observations
            .disconnected_indices
            .push((event.former_entity, event.monitor.index));
        observations
            .current_monitors_at_disconnect
            .push(windows.iter().count());
    }

    fn observe_restore_input(
        windows: Query<&CurrentMonitor, With<PrimaryWindow>>,
        mut observations: ResMut<DownstreamObservations>,
    ) {
        observations.restore_current_monitors = windows.iter().count();
    }

    fn observe_settle_input(
        windows: Query<&CurrentMonitor, With<PrimaryWindow>>,
        mut observations: ResMut<DownstreamObservations>,
    ) {
        observations.settle_current_monitors = windows.iter().count();
    }

    fn observe_window_restored(
        _: On<WindowRestored>,
        mut observations: ResMut<RecoveryObservations>,
    ) {
        observations.restored += 1;
    }

    fn observe_window_restore_mismatch(
        _: On<WindowRestoreMismatch>,
        mut observations: ResMut<RecoveryObservations>,
    ) {
        observations.mismatched += 1;
    }

    fn insert_scheduled_monitor_association(
        mut commands: Commands,
        association: Res<ScheduledMonitorAssociation>,
    ) {
        commands
            .entity(association.window_entity)
            .insert(OnMonitor(association.monitor_entity));
        commands.remove_resource::<ScheduledMonitorAssociation>();
    }

    fn observed_topology_app() -> App {
        let mut app = App::new();
        app.insert_resource(Platform::X11)
            .insert_resource(WinitMonitors::default())
            .insert_resource(MonitorConfiguration::for_test())
            .init_resource::<MonitorIdentityRegistry>()
            .init_resource::<InjectedMonitorEvidence>()
            .init_resource::<InjectedWinitMonitorOrder>()
            .init_resource::<InjectedCurrentMonitorSource>()
            .init_resource::<TopologyObservations>()
            .init_resource::<DownstreamObservations>()
            .init_resource::<RecoveryObservations>()
            .add_observer(observe_connected)
            .add_observer(observe_disconnected)
            .add_observer(observe_window_restored)
            .add_observer(observe_window_restore_mismatch)
            .add_observer(current_monitor::clear_monitor_selection_inputs)
            .add_observer(current_monitor::install_current_monitor_from_association);
        #[cfg(feature = "monitor-probe")]
        app.init_resource::<FrameCount>()
            .init_resource::<InjectedTopologyProbeRecords>();
        app
    }

    fn add_topology_update_systems(app: &mut App) {
        app.add_systems(
            Update,
            (
                (update_monitors, current_monitor::update_current_monitor).chain(),
                observe_restore_input.after(current_monitor::update_current_monitor),
                observe_settle_input.after(observe_restore_input),
            ),
        );
    }

    fn topology_app() -> App {
        let mut app = observed_topology_app();
        app.insert_resource(Monitors { live: Vec::new() })
            .init_resource::<MonitorTopologyRevision>();
        add_topology_update_systems(&mut app);
        app
    }

    fn phase_seven_topology_app(state_file: &Path) -> App {
        let mut app = observed_topology_app();
        app.insert_resource(ManagedWindowPersistence::RememberAll)
            .insert_resource(RestoreWindowConfig {
                path: state_file.to_path_buf(),
            })
            .init_resource::<ManagedWindowRegistry>()
            .init_resource::<InjectedWindowPositions>()
            .configure_sets(
                PreStartup,
                (
                    ClerestoryPreStartupSet::MonitorsInitialized,
                    ClerestoryPreStartupSet::PersistenceLoaded,
                )
                    .chain(),
            )
            .configure_sets(
                Update,
                (
                    ClerestoryUpdateSet::MonitorTopology,
                    ClerestoryUpdateSet::RecoveryTopology,
                    ClerestoryUpdateSet::CurrentMonitor,
                    ClerestoryUpdateSet::RecoveryWindow,
                    ClerestoryUpdateSet::RestorePreparation,
                    ClerestoryUpdateSet::X11Compensation,
                    ClerestoryUpdateSet::RestoreApplication,
                    ClerestoryUpdateSet::RestoreSettling,
                    ClerestoryUpdateSet::Persistence,
                )
                    .chain(),
            )
            .add_plugins(RecoveryPlugin)
            .add_plugins(PersistencePlugin)
            .add_observer(managed::on_managed_window_added)
            .add_observer(managed::on_managed_window_load)
            .add_observer(restore::mark_native_window_ready)
            .add_systems(
                PreStartup,
                init_monitors.in_set(ClerestoryPreStartupSet::MonitorsInitialized),
            )
            .add_systems(
                Update,
                update_monitors.in_set(ClerestoryUpdateSet::MonitorTopology),
            )
            .add_systems(
                Update,
                current_monitor::update_current_monitor.in_set(ClerestoryUpdateSet::CurrentMonitor),
            )
            .add_systems(
                Last,
                insert_scheduled_monitor_association
                    .run_if(resource_exists::<ScheduledMonitorAssociation>),
            );
        app
    }

    fn startup_topology_app() -> App {
        let mut app = observed_topology_app();
        app.add_systems(PreStartup, init_monitors);
        add_topology_update_systems(&mut app);
        app
    }

    fn spawn_monitor(
        app: &mut App,
        evidence: &'static [u8],
        position: IVec2,
        size: UVec2,
        scale: f64,
    ) -> Entity {
        let entity = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<InjectedMonitorEvidence>()
            .evidence
            .insert(entity, Ok(evidence));
        app.world_mut()
            .resource_mut::<InjectedWinitMonitorOrder>()
            .connect(entity);
        app.world_mut()
            .entity_mut(entity)
            .insert(monitor_component(position, size, scale));
        entity
    }

    fn spawn_monitor_at(
        app: &mut App,
        entity: Entity,
        evidence: &'static [u8],
        position: IVec2,
        size: UVec2,
        scale: f64,
    ) -> Entity {
        let spawned = app.world_mut().spawn_empty_at(entity).is_ok();
        assert!(spawned);
        app.world_mut()
            .resource_mut::<InjectedMonitorEvidence>()
            .evidence
            .insert(entity, Ok(evidence));
        app.world_mut()
            .resource_mut::<InjectedWinitMonitorOrder>()
            .connect(entity);
        app.world_mut()
            .entity_mut(entity)
            .insert(monitor_component(position, size, scale));
        entity
    }

    fn remove_monitor(app: &mut App, entity: Entity) {
        app.world_mut()
            .resource_mut::<InjectedMonitorEvidence>()
            .evidence
            .remove(&entity);
        app.world_mut()
            .resource_mut::<InjectedWinitMonitorOrder>()
            .disconnect(entity);
        app.world_mut().entity_mut(entity).remove::<Monitor>();
    }

    fn set_cached_monitor_order(app: &mut App, entities: impl IntoIterator<Item = Entity>) {
        app.world_mut()
            .resource_mut::<InjectedWinitMonitorOrder>()
            .replace(entities);
    }

    #[cfg(feature = "monitor-probe")]
    fn despawn_monitor(app: &mut App, entity: Entity) {
        app.world_mut()
            .resource_mut::<InjectedMonitorEvidence>()
            .evidence
            .remove(&entity);
        app.world_mut()
            .resource_mut::<InjectedWinitMonitorOrder>()
            .disconnect(entity);
        assert!(app.world_mut().despawn(entity));
    }

    fn despawn_monitor_for_reuse(app: &mut App, entity: Entity) -> Option<Entity> {
        app.world_mut()
            .resource_mut::<InjectedMonitorEvidence>()
            .evidence
            .remove(&entity);
        app.world_mut()
            .resource_mut::<InjectedWinitMonitorOrder>()
            .disconnect(entity);
        app.world_mut().despawn_no_free(entity)
    }

    fn reset_activity(app: &mut App) {
        app.world_mut()
            .resource_mut::<InjectedMonitorEvidence>()
            .activity = TopologyProducerActivity::default();
        app.world_mut()
            .resource_mut::<InjectedCurrentMonitorSource>()
            .reset_activity();
    }

    fn assert_persistence_idle(app: &App) {
        let activity = app.world().resource::<CapturedWindowStates>().activity();
        assert_eq!(activity.file_reads, 0);
        assert_eq!(activity.window_scans, 0);
        assert_eq!(activity.captures, 0);
        assert_eq!(activity.projections, 0);
        assert_eq!(activity.writes, 0);
        assert_eq!(
            app.world().resource::<InjectedMonitorEvidence>().activity,
            TopologyProducerActivity::default()
        );
        assert_eq!(
            app.world()
                .resource::<InjectedCurrentMonitorSource>()
                .lookups,
            0
        );
        assert_eq!(app.world().resource::<InjectedWindowPositions>().lookups, 0);
    }

    fn reset_persistence_activity(app: &mut App) {
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .reset_activity();
        app.world_mut()
            .resource_mut::<InjectedWindowPositions>()
            .reset_activity();
        reset_activity(app);
    }

    fn assert_single_persistence_batch(app: &App) {
        let activity = app.world().resource::<CapturedWindowStates>().activity();
        assert_eq!(activity.window_scans, 1);
        assert_eq!(activity.captures, 1);
        assert_eq!(activity.projections, 1);
        assert_eq!(activity.writes, 1);
    }

    fn assert_pending_registration(app: &App) {
        let registration = recovery::registration_snapshot(app.world());
        assert_eq!(registration.pending, 1);
        assert!(registration.accepted.is_empty());
        assert_eq!(registration.generated, 1);
    }

    fn assert_accepted_registration(app: &App, window_key: &WindowKey, monitor_info: MonitorInfo) {
        let registration = recovery::registration_snapshot(app.world());
        assert_eq!(registration.pending, 0);
        assert_eq!(
            registration.accepted,
            vec![(window_key.clone(), monitor_info)]
        );
        assert_eq!(registration.generated, 1);
    }

    fn assert_initial_managed_fallback(
        app: &App,
        window_entity: Entity,
        window_key: &WindowKey,
        state_file: &Path,
    ) -> Result<(), String> {
        let initial_current_monitor = app.world().get::<CurrentMonitor>(window_entity);
        assert_eq!(
            initial_current_monitor.map(|current_monitor| current_monitor.index),
            Some(0)
        );
        assert!(
            !app.world()
                .entity(window_entity)
                .contains::<NativeWindowReady>()
        );
        let fallback = app
            .world()
            .resource::<CapturedWindowStates>()
            .captured_placement(window_key)
            .ok_or_else(|| "managed window should capture the initial fallback".to_string())?;
        assert_eq!(fallback.monitor_snapshot.index, 0);
        assert!((fallback.monitor_snapshot.scale - DEFAULT_SCALE).abs() < SCALE_FACTOR_EPSILON);
        assert_persisted_monitor(state_file, 0, DEFAULT_SCALE)?;
        assert_single_persistence_batch(app);
        assert_pending_registration(app);
        Ok(())
    }

    fn assert_repaired_monitor_association(
        app: &App,
        window_entity: Entity,
        monitor_info: MonitorInfo,
    ) {
        let current_monitor = app.world().get::<CurrentMonitor>(window_entity);
        assert_eq!(
            current_monitor.map(|current_monitor| current_monitor.monitor_info),
            Some(monitor_info)
        );
        assert!(
            app.world()
                .entity(window_entity)
                .contains::<NativeWindowReady>()
        );
        assert_pending_registration(app);
    }

    fn assert_corrected_managed_capture(
        app: &App,
        window_key: &WindowKey,
        monitor_info: MonitorInfo,
        state_file: &Path,
    ) -> Result<(), String> {
        let captured = app
            .world()
            .resource::<CapturedWindowStates>()
            .captured_placement(window_key)
            .ok_or_else(|| "managed window should be recaptured".to_string())?;
        assert_eq!(captured.monitor_snapshot, monitor_info);
        let projected = app
            .world()
            .resource::<CapturedWindowStates>()
            .project("test");
        let projected = projected
            .get(window_key)
            .ok_or_else(|| "managed window should be projected".to_string())?;
        assert_eq!(projected.monitor, 1);
        assert!((projected.scale - LOW_DPI_SCALE).abs() < SCALE_FACTOR_EPSILON);
        assert_persisted_monitor(state_file, 1, LOW_DPI_SCALE)?;
        assert_single_persistence_batch(app);
        assert_pending_registration(app);
        Ok(())
    }

    fn reset_observations(app: &mut App) {
        let mut observations = app.world_mut().resource_mut::<TopologyObservations>();
        observations.connected.clear();
        observations.connected_indices.clear();
        #[cfg(feature = "monitor-probe")]
        observations.probe_records_at_connect.clear();
        observations.disconnected.clear();
        observations.disconnected_indices.clear();
        observations.current_monitors_at_disconnect.clear();
    }

    #[cfg(feature = "monitor-probe")]
    fn trace_field<'a>(record: &'a TraceRecord, name: &str) -> Option<&'a str> {
        record
            .fields
            .iter()
            .find(|(field_name, _)| field_name == name)
            .map(|(_, value)| value.as_str())
    }

    #[cfg(feature = "monitor-probe")]
    fn run_example_trace_scenario() -> (Entity, Vec<TraceRecord>) {
        let mut app = startup_topology_app();
        let trace = ProbeTrace::default();
        app.insert_resource(trace.clone())
            .add_observer(example_trace::on_monitor_connected)
            .add_observer(example_trace::on_monitor_disconnected);
        let Some(layer) = example_trace::monitor_probe_layer(&mut app) else {
            return (Entity::PLACEHOLDER, Vec::new());
        };
        let subscriber = Registry::default().with(layer);
        let startup_entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );

        bevy::log::tracing::subscriber::with_default(subscriber, || {
            app.update();
            app.world_mut().resource_mut::<FrameCount>().0 = RUNTIME_TRACE_FRAME;
            spawn_monitor(
                &mut app,
                PANEL_B_EVIDENCE,
                IVec2::new(MONITOR_WIDTH.to_i32(), 0),
                UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
                LOW_DPI_SCALE,
            );
            app.update();
        });

        (startup_entity, trace.records())
    }

    #[cfg(feature = "monitor-probe")]
    fn assert_startup_trace(startup: &TraceRecord) {
        assert_eq!(startup.frame_count, 0);
        assert_eq!(startup.producer, SCHEDULE_PRE_STARTUP_MONITORS);
        assert_eq!(
            startup.kind,
            super::example_probe::constants::KIND_MONITOR_TOPOLOGY
        );
        assert_eq!(
            trace_field(startup, "configuration_state"),
            Some("\"ready\"")
        );
        assert_eq!(
            trace_field(startup, "configuration_generation"),
            Some("Some(0)")
        );
        assert_eq!(trace_field(startup, "topology_revision"), Some("0"));
        assert_eq!(
            trace_field(startup, "evidence_provenance"),
            Some("\"observed-current-generation\"")
        );
        assert_eq!(
            trace_field(startup, "topology_change"),
            Some("\"connected\"")
        );
    }

    fn assert_capture_snapshot(
        placement: &CapturedWindowPlacement,
        physical_position: IVec2,
        scale: f64,
    ) {
        assert_eq!(
            placement.monitor_snapshot.physical_position,
            physical_position
        );
        assert!((placement.monitor_snapshot.scale - scale).abs() < SCALE_FACTOR_EPSILON);
    }

    fn installed_monitor(app: &App, entity: Entity) -> Option<MonitorInfo> {
        app.world()
            .resource::<Monitors>()
            .iter()
            .find(|monitor| monitor.entity == entity)
            .map(|monitor| *monitor.monitor_info)
    }

    fn spawn_accepted_automatic_primary(
        app: &mut App,
        target_entity: Entity,
        target: MonitorInfo,
        window_mode: WindowMode,
        captured_position: Option<IVec2>,
    ) -> Entity {
        let fills_monitor = matches!(window_mode, WindowMode::BorderlessFullscreen(_));
        let window_position = if fills_monitor {
            target.physical_position
        } else {
            target.physical_position + IVec2::ONE
        };
        let mut window = Window {
            mode: window_mode,
            position: WindowPosition::At(window_position),
            ..default()
        };
        if fills_monitor {
            window
                .resolution
                .set_physical_resolution(target.physical_size.x, target.physical_size.y);
        }
        let window_entity = app
            .world_mut()
            .spawn((
                window,
                PrimaryWindow,
                OnMonitor(target_entity),
                WindowRecovery::FallbackAndReturn,
            ))
            .id();
        app.world_mut()
            .resource_mut::<InjectedCurrentMonitorSource>()
            .set_position(window_entity, Some(window_position));
        app.world_mut()
            .resource_mut::<InjectedWindowPositions>()
            .set(window_entity, captured_position);
        app.world_mut().flush();
        app.update();
        app.update();
        assert_accepted_registration(app, &WindowKey::Primary, target);
        window_entity
    }

    fn set_window_monitor(
        app: &mut App,
        window_entity: Entity,
        monitor_info: MonitorInfo,
        window_mode: WindowMode,
    ) {
        let fills_monitor = matches!(window_mode, WindowMode::BorderlessFullscreen(_));
        let window_position = if fills_monitor {
            monitor_info.physical_position
        } else {
            monitor_info.physical_position + IVec2::ONE
        };
        let changed = app
            .world_mut()
            .entity_mut(window_entity)
            .get_mut::<Window>()
            .map(|mut window| {
                window.mode = window_mode;
                window.position = WindowPosition::At(window_position);
                if fills_monitor {
                    window.resolution.set_physical_resolution(
                        monitor_info.physical_size.x,
                        monitor_info.physical_size.y,
                    );
                }
            });
        assert!(changed.is_some());
        app.world_mut()
            .resource_mut::<InjectedCurrentMonitorSource>()
            .set_position(window_entity, Some(window_position));
    }

    fn assert_primary_capture_state(
        app: &App,
        expected_placement: &CapturedWindowPlacement,
        expected_write_state: PersistenceWriteState,
    ) {
        let states = app.world().resource::<CapturedWindowStates>();
        assert_eq!(
            states.captured_placement(&WindowKey::Primary),
            Some(expected_placement),
        );
        assert_eq!(
            states
                .entry(&WindowKey::Primary)
                .map(|state| state.persistence),
            Some(expected_write_state),
        );
    }

    fn enter_on_fallback(
        app: &mut App,
        window_entity: Entity,
        target_entity: Entity,
        fallback_entity: Entity,
        fallback: MonitorInfo,
    ) -> Result<MonitorInfo, String> {
        set_window_monitor(
            app,
            window_entity,
            fallback,
            WindowMode::BorderlessFullscreen(MonitorSelection::Index(fallback.index)),
        );
        remove_monitor(app, target_entity);
        app.update();
        let fallback = installed_monitor(app, fallback_entity)
            .ok_or_else(|| "fallback monitor should remain installed".to_string())?;

        let recovery = recovery::fallback_and_return_snapshot(app.world(), &WindowKey::Primary);
        assert_eq!(
            recovery.map(|snapshot| snapshot.phase),
            Some(FallbackAndReturnPhaseSnapshot::FallbackSettling),
        );
        assert_eq!(
            app.world()
                .get::<CurrentMonitor>(window_entity)
                .map(|current_monitor| current_monitor.monitor_info),
            Some(fallback),
        );

        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            SETTLE_STABILITY_SECS,
        )));
        app.update();
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        let recovery = recovery::fallback_and_return_snapshot(app.world(), &WindowKey::Primary);
        assert_eq!(
            recovery.map(|snapshot| snapshot.phase),
            Some(FallbackAndReturnPhaseSnapshot::OnFallback),
        );
        Ok(fallback)
    }

    fn assert_persisted_monitor(path: &Path, index: usize, scale: f64) -> Result<(), String> {
        let persisted = fs::read_to_string(path).map_err(|error| error.to_string())?;
        assert!(persisted.contains(&format!("monitor_scale: {scale:.1}")));
        assert!(persisted.contains(&format!("monitor_index: {index}")));
        Ok(())
    }

    #[cfg(feature = "monitor-probe")]
    fn assert_connected_trace(connected: &TraceRecord, startup_entity: Entity) {
        let startup_entity = format!("{startup_entity:?}");
        assert_eq!(connected.frame_count, 0);
        assert_eq!(
            connected.producer,
            super::example_probe::constants::PRODUCER_MONITOR_CONNECTED
        );
        assert_eq!(
            connected.kind,
            super::example_probe::constants::KIND_MONITOR_CONNECTED
        );
        assert_eq!(
            trace_field(
                connected,
                super::example_probe::constants::FIELD_MONITOR_ENTITY
            ),
            Some(startup_entity.as_str())
        );
        assert_eq!(
            trace_field(
                connected,
                super::example_probe::constants::FIELD_TOPOLOGY_REVISION
            ),
            Some("0")
        );
        assert!(trace_field(connected, super::example_probe::constants::FIELD_MONITOR).is_some());
    }

    #[cfg(feature = "monitor-probe")]
    fn assert_runtime_trace(runtime: &TraceRecord) {
        assert_eq!(runtime.frame_count, RUNTIME_TRACE_FRAME);
        assert_eq!(runtime.producer, SCHEDULE_UPDATE_MONITORS);
        assert_eq!(
            runtime.kind,
            super::example_probe::constants::KIND_MONITOR_TOPOLOGY
        );
        assert_eq!(trace_field(runtime, "topology_revision"), Some("1"));
        assert_eq!(
            trace_field(runtime, "topology_change"),
            Some("\"connected\"")
        );
    }

    fn assert_returned_monitor_order(
        app: &App,
        returned: Entity,
        survivor: Entity,
        original_identity: Option<MonitorIdentity>,
    ) {
        let monitors = app.world().resource::<Monitors>();
        assert_eq!(
            monitors
                .iter()
                .map(|monitor| (monitor.entity, monitor.monitor_info.index))
                .collect::<Vec<_>>(),
            [(returned, 0), (survivor, 1)]
        );
        assert_eq!(
            monitors.by_index(0).map(|monitor| monitor.identity),
            original_identity
        );
        assert_eq!(
            monitors.by_index(0).map(|monitor| (
                monitor.physical_position,
                monitor.physical_size,
                monitor.scale,
            )),
            Some((
                RETURNED_MONITOR_POSITION,
                RETURNED_MONITOR_SIZE,
                DEFAULT_SCALE,
            ))
        );
        assert_eq!(
            app.world()
                .resource::<TopologyObservations>()
                .connected_indices,
            [(returned, 0)]
        );
    }

    #[test]
    fn reflected_type_paths_preserve_monitor_namespace() {
        assert_eq!(
            [
                <CurrentMonitor as TypePath>::type_path(),
                <MonitorId as TypePath>::type_path(),
                <MonitorIdentity as TypePath>::type_path(),
                <MonitorInfo as TypePath>::type_path(),
                <Monitors as TypePath>::type_path(),
                <MonitorTopologyRevision as TypePath>::type_path(),
                <MonitorConnected as TypePath>::type_path(),
                <MonitorDisconnected as TypePath>::type_path(),
            ],
            [
                "bevy_clerestory::monitors::CurrentMonitor",
                "bevy_clerestory::monitors::MonitorId",
                "bevy_clerestory::monitors::MonitorIdentity",
                "bevy_clerestory::monitors::MonitorInfo",
                "bevy_clerestory::monitors::Monitors",
                "bevy_clerestory::monitors::MonitorTopologyRevision",
                "bevy_clerestory::monitors::MonitorConnected",
                "bevy_clerestory::monitors::MonitorDisconnected",
            ]
        );
    }

    fn has_reflect_event<T: 'static>(registry: &TypeRegistry) -> bool {
        registry
            .get(TypeId::of::<T>())
            .and_then(|registration| registration.data::<ReflectEvent>())
            .is_some()
    }

    #[test]
    fn monitor_lifetime_events_auto_register_reflected_event_type_data() {
        let app = App::new();
        let registry = app.world().resource::<AppTypeRegistry>().read();

        assert!(has_reflect_event::<MonitorConnected>(&registry));
        assert!(has_reflect_event::<MonitorDisconnected>(&registry));
    }

    #[test]
    fn exact_identity_lookup_returns_metadata_and_entity() {
        let id = MonitorId::from_raw(7);
        let monitors = Monitors::from_test_monitors([
            (ENTITY_1, monitor(verified(7), 0, IVec2::ZERO)),
            (
                ENTITY_2,
                monitor(
                    MonitorIdentity::Unverified,
                    1,
                    IVec2::new(MONITOR_WIDTH.to_i32(), 0),
                ),
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
            (
                ENTITY_2,
                monitor(verified(8), 1, IVec2::new(MONITOR_WIDTH.to_i32(), 0)),
            ),
        ]);

        assert!(monitors.by_id(MonitorId::from_raw(7)).is_none());
        assert!(monitors.entity_by_id(MonitorId::from_raw(7)).is_none());
    }

    #[test]
    fn identity_lookup_rejects_multiple_live_exact_matches() {
        let id = MonitorId::from_raw(7);
        let monitors = Monitors::from_test_monitors([
            (ENTITY_1, monitor(verified(7), 0, IVec2::ZERO)),
            (
                ENTITY_2,
                monitor(verified(7), 1, IVec2::new(MONITOR_WIDTH.to_i32(), 0)),
            ),
        ]);

        assert!(monitors.by_id(id).is_none());
        assert!(monitors.entity_by_id(id).is_none());
    }

    #[test]
    fn raw_deltas_preserve_snapshot_order_for_verified_and_unverified_entities() {
        let previous = Monitors::from_test_monitors([
            (ENTITY_2, monitor(verified(2), 0, IVec2::ZERO)),
            (
                ENTITY_1,
                monitor(
                    MonitorIdentity::Unverified,
                    1,
                    IVec2::new(MONITOR_WIDTH.to_i32(), 0),
                ),
            ),
        ]);
        let rebuilt = Monitors::from_test_monitors([
            (ENTITY_4, monitor(verified(4), 0, IVec2::ZERO)),
            (
                ENTITY_3,
                monitor(
                    MonitorIdentity::Unverified,
                    1,
                    IVec2::new(MONITOR_WIDTH.to_i32(), 0),
                ),
            ),
        ]);

        let changes = monitor_changes(&previous, &rebuilt);

        assert_eq!(changes.connected.len(), 2);
        assert_eq!(changes.disconnected.len(), 2);
        assert_eq!(
            changes
                .connected
                .iter()
                .map(|monitor| monitor.entity)
                .collect::<Vec<_>>(),
            [ENTITY_4, ENTITY_3]
        );
        assert_eq!(changes.connected[0].monitor_info.identity, verified(4));
        assert_eq!(
            changes
                .disconnected
                .iter()
                .map(|monitor| monitor.entity)
                .collect::<Vec<_>>(),
            [ENTITY_2, ENTITY_1]
        );
        assert_eq!(changes.disconnected[0].monitor_info.identity, verified(2));
    }

    #[test]
    fn startup_installs_before_observer_and_emits_connect_once_at_revision_zero() {
        let mut app = startup_topology_app();
        let entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );

        app.update();

        let observations = app.world().resource::<TopologyObservations>();
        assert_eq!(
            observations.connected,
            [(entity, MonitorTopologyRevision(0))]
        );
        assert!(observations.disconnected.is_empty());
        assert_eq!(
            app.world()
                .resource::<Monitors>()
                .iter()
                .next()
                .map(|live| live.entity),
            Some(entity)
        );
        #[cfg(feature = "monitor-probe")]
        {
            assert_eq!(observations.probe_records_at_connect, [1]);
            let captured = app.world().resource::<InjectedTopologyProbeRecords>();
            assert_eq!(captured.records.len(), 1);
            let record = &captured.records[0];
            assert_eq!(record.frame_count, 0);
            assert_eq!(record.schedule, "PreStartup::init_monitors");
            assert_eq!(record.configuration_state, "ready");
            assert_eq!(record.configuration_generation, Some(0));
            assert_eq!(record.revision, 0);
            assert_eq!(record.evidence_provenance, "observed-current-generation");
            assert_eq!(record.evidence_generation, Some(0));
            assert_eq!(record.entity, entity);
            assert_eq!(record.change, "connected");
        }

        reset_activity(&mut app);
        app.update();

        let observations = app.world().resource::<TopologyObservations>();
        assert_eq!(
            observations.connected,
            [(entity, MonitorTopologyRevision(0))]
        );
        assert_eq!(
            app.world().resource::<InjectedMonitorEvidence>().activity,
            TopologyProducerActivity::default()
        );
        assert_eq!(
            app.world()
                .resource::<InjectedCurrentMonitorSource>()
                .lookups,
            0
        );
    }

    #[cfg(feature = "monitor-probe")]
    #[test]
    fn example_trace_factory_orders_real_startup_observer_and_runtime_records() {
        let (startup_entity, records) = run_example_trace_scenario();
        assert_eq!(records.len(), 4);
        assert_eq!(
            records
                .iter()
                .map(|record| record.sequence)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert_startup_trace(&records[0]);
        assert_connected_trace(&records[1], startup_entity);
        assert_runtime_trace(&records[2]);
        assert!(records
            .windows(2)
            .all(|records| records[0].timestamp_unix_micros <= records[1].timestamp_unix_micros));
    }

    #[test]
    fn empty_startup_installs_revision_zero_without_connect() {
        let mut app = startup_topology_app();

        app.update();

        assert!(app.world().resource::<Monitors>().is_empty());
        assert_eq!(
            *app.world().resource::<MonitorTopologyRevision>(),
            MonitorTopologyRevision(0)
        );
        let observations = app.world().resource::<TopologyObservations>();
        assert!(observations.connected.is_empty());
        assert!(observations.disconnected.is_empty());
        #[cfg(feature = "monitor-probe")]
        assert!(
            app.world()
                .resource::<InjectedTopologyProbeRecords>()
                .records
                .is_empty()
        );
    }

    #[test]
    fn post_startup_addition_installs_before_observer_and_advances_revision() {
        let mut app = startup_topology_app();
        app.update();
        let entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );

        app.update();

        let observations = app.world().resource::<TopologyObservations>();
        assert_eq!(
            observations.connected,
            [(entity, MonitorTopologyRevision(1))]
        );
        assert!(observations.disconnected.is_empty());
        assert_eq!(
            app.world()
                .resource::<Monitors>()
                .iter()
                .next()
                .map(|live| live.entity),
            Some(entity)
        );
    }

    #[test]
    fn removal_installs_empty_topology_before_observer() {
        let mut app = topology_app();
        let entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        app.update();
        reset_observations(&mut app);

        remove_monitor(&mut app, entity);
        app.update();

        let observations = app.world().resource::<TopologyObservations>();
        assert_eq!(
            observations.disconnected,
            [(entity, MonitorTopologyRevision(2))]
        );
        assert_eq!(observations.current_monitors_at_disconnect, [0]);
        assert!(app.world().resource::<Monitors>().is_empty());
    }

    #[test]
    fn reconnect_uses_new_entity_metadata_and_retains_stable_identity() {
        let mut app = topology_app();
        let original = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            LOW_DPI_SCALE,
        );
        app.update();
        let original_identity = app.world().resource::<Monitors>().first().identity;

        remove_monitor(&mut app, original);
        app.update();
        let returned = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            RETURNED_MONITOR_POSITION,
            RETURNED_MONITOR_SIZE,
            DEFAULT_SCALE,
        );
        app.update();

        let live = app.world().resource::<Monitors>().iter().next();
        assert_eq!(live.map(|monitor| monitor.entity), Some(returned));
        assert_eq!(
            live.map(|monitor| monitor.monitor_info.identity),
            Some(original_identity)
        );
        assert_eq!(
            live.map(|monitor| monitor.monitor_info.physical_position),
            Some(RETURNED_MONITOR_POSITION)
        );
        assert_eq!(
            live.map(|monitor| monitor.monitor_info.physical_size),
            Some(RETURNED_MONITOR_SIZE)
        );
        assert_eq!(
            live.map(|monitor| monitor.monitor_info.scale),
            Some(DEFAULT_SCALE)
        );
    }

    #[test]
    fn cached_winit_indices_cover_despawn_reconnect_and_entity_reuse() {
        let mut app = topology_app();
        let original = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            LOW_DPI_SCALE,
        );
        let survivor_position = IVec2::new(MONITOR_WIDTH.to_i32(), 0);
        let survivor = spawn_monitor(
            &mut app,
            PANEL_B_EVIDENCE,
            survivor_position,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        set_cached_monitor_order(&mut app, [survivor, original]);

        app.update();

        let monitors = app.world().resource::<Monitors>();
        assert_eq!(
            monitors
                .iter()
                .map(|monitor| (monitor.entity, monitor.monitor_info.index))
                .collect::<Vec<_>>(),
            [(survivor, 0), (original, 1)]
        );
        assert_eq!(
            monitors
                .by_index(0)
                .map(|monitor| monitor.physical_position),
            Some(survivor_position)
        );
        let original_identity = monitors
            .iter()
            .find(|monitor| monitor.entity == original)
            .map(|monitor| monitor.monitor_info.identity);
        let connected_indices = &app
            .world()
            .resource::<TopologyObservations>()
            .connected_indices;
        assert_eq!(connected_indices.len(), 2);
        assert!(connected_indices.contains(&(original, 1)));
        assert!(connected_indices.contains(&(survivor, 0)));
        reset_observations(&mut app);

        let reusable_entity = despawn_monitor_for_reuse(&mut app, original);
        assert!(reusable_entity.is_some());
        let Some(reusable_entity) = reusable_entity else {
            return;
        };
        app.update();

        assert_eq!(
            app.world()
                .resource::<TopologyObservations>()
                .disconnected_indices,
            [(original, 1)]
        );
        reset_observations(&mut app);

        let returned = spawn_monitor_at(
            &mut app,
            reusable_entity,
            PANEL_A_EVIDENCE,
            RETURNED_MONITOR_POSITION,
            RETURNED_MONITOR_SIZE,
            DEFAULT_SCALE,
        );
        assert_eq!(returned.index(), original.index());
        assert_ne!(returned.generation(), original.generation());
        set_cached_monitor_order(&mut app, [returned, survivor]);
        let mut entity_order = [returned, survivor];
        entity_order.sort_by_key(|entity| entity.to_bits());
        assert_ne!(entity_order, [returned, survivor]);

        app.update();

        assert_returned_monitor_order(&app, returned, survivor, original_identity);
    }

    #[test]
    fn missing_cached_handle_uses_deterministic_unassociated_index() {
        let mut app = topology_app();
        let missing = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        let cached_position = IVec2::new(MONITOR_WIDTH.to_i32(), 0);
        let cached = spawn_monitor(
            &mut app,
            PANEL_B_EVIDENCE,
            cached_position,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        set_cached_monitor_order(&mut app, [cached]);

        app.update();

        let monitors = app.world().resource::<Monitors>();
        assert_eq!(
            monitors
                .iter()
                .map(|monitor| (monitor.entity, monitor.monitor_info.index))
                .collect::<Vec<_>>(),
            [(cached, 0), (missing, 1)]
        );
        assert_eq!(
            monitors.by_index(1).map(|monitor| monitor.identity),
            Some(MonitorIdentity::Unverified)
        );
        assert_eq!(
            monitors
                .by_index(1)
                .map(|monitor| monitor.physical_position),
            Some(IVec2::ZERO)
        );
    }

    #[test]
    fn changed_evidence_on_new_entity_cannot_inherit_verified_identity() {
        let mut app = topology_app();
        let original = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        app.update();
        let original_identity = app.world().resource::<Monitors>().first().identity;

        remove_monitor(&mut app, original);
        app.update();
        spawn_monitor(
            &mut app,
            PANEL_B_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        app.update();

        assert_ne!(
            app.world().resource::<Monitors>().first().identity,
            original_identity
        );
    }

    #[test]
    fn generation_identity_change_updates_mapping_without_raw_events() {
        let mut app = topology_app();
        let entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        app.update();
        reset_observations(&mut app);
        app.world_mut()
            .resource_mut::<InjectedMonitorEvidence>()
            .evidence
            .insert(entity, Ok(PANEL_B_EVIDENCE));
        app.world()
            .resource::<MonitorConfiguration>()
            .advance_for_test();

        app.update();

        assert_eq!(
            app.world().resource::<Monitors>().first().identity,
            MonitorIdentity::Unverified
        );
        assert_eq!(
            *app.world().resource::<MonitorTopologyRevision>(),
            MonitorTopologyRevision(2)
        );
        let observations = app.world().resource::<TopologyObservations>();
        assert!(observations.connected.is_empty());
        assert!(observations.disconnected.is_empty());
    }

    #[test]
    fn revalidation_projects_final_ambiguity_independent_of_cached_order() {
        for changed_monitor_first in [false, true] {
            let mut app = topology_app();
            let panel_a = spawn_monitor(
                &mut app,
                PANEL_A_EVIDENCE,
                IVec2::ZERO,
                UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
                DEFAULT_SCALE,
            );
            let panel_b = spawn_monitor(
                &mut app,
                PANEL_B_EVIDENCE,
                IVec2::new(MONITOR_WIDTH.to_i32(), 0),
                UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
                DEFAULT_SCALE,
            );
            set_cached_monitor_order(&mut app, [panel_a, panel_b]);
            app.update();
            let verified_ids: Vec<_> = app
                .world()
                .resource::<Monitors>()
                .iter()
                .filter_map(|monitor| match monitor.monitor_info.identity {
                    MonitorIdentity::Verified(id) => Some(id),
                    MonitorIdentity::Unverified => None,
                })
                .collect();
            assert_eq!(verified_ids.len(), 2);

            app.world_mut()
                .resource_mut::<InjectedMonitorEvidence>()
                .evidence
                .insert(panel_b, Ok(PANEL_A_EVIDENCE));
            if changed_monitor_first {
                set_cached_monitor_order(&mut app, [panel_b, panel_a]);
            }
            app.world()
                .resource::<MonitorConfiguration>()
                .advance_for_test();

            app.update();

            let monitors = app.world().resource::<Monitors>();
            assert!(
                monitors
                    .iter()
                    .all(|monitor| monitor.monitor_info.identity == MonitorIdentity::Unverified)
            );
            assert!(
                verified_ids
                    .into_iter()
                    .all(|id| monitors.by_id(id).is_none())
            );
        }
    }

    #[cfg(feature = "monitor-probe")]
    #[test]
    fn removal_with_generation_advance_reports_retained_evidence() {
        let mut app = topology_app();
        let entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        app.update();
        app.world_mut()
            .resource_mut::<InjectedTopologyProbeRecords>()
            .records
            .clear();
        despawn_monitor(&mut app, entity);
        app.world()
            .resource::<MonitorConfiguration>()
            .advance_for_test();

        app.update();

        let captured = app.world().resource::<InjectedTopologyProbeRecords>();
        assert_eq!(captured.records.len(), 1);
        assert_eq!(captured.records[0].configuration_generation, Some(1));
        assert_eq!(captured.records[0].evidence_generation, Some(0));
        assert_eq!(
            captured.records[0].evidence_provenance,
            "retained-earlier-generation"
        );
        assert_eq!(captured.records[0].entity, entity);
        assert_eq!(captured.records[0].change, "disconnected");
    }

    #[test]
    fn identical_generation_revalidation_keeps_revision_and_raw_events() {
        let mut app = topology_app();
        spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        app.update();
        reset_observations(&mut app);
        reset_activity(&mut app);
        app.world()
            .resource::<MonitorConfiguration>()
            .advance_for_test();

        app.update();

        assert_eq!(
            *app.world().resource::<MonitorTopologyRevision>(),
            MonitorTopologyRevision(1)
        );
        let observations = app.world().resource::<TopologyObservations>();
        assert!(observations.connected.is_empty());
        assert!(observations.disconnected.is_empty());
        assert_eq!(
            app.world().resource::<InjectedMonitorEvidence>().activity,
            TopologyProducerActivity {
                topology_scans:       1,
                component_reads:      1,
                cached_order_lookups: 1,
                identity_requests:    1,
                handle_lookups:       1,
                evidence_loads:       1,
            }
        );
    }

    #[test]
    fn same_entity_metadata_change_without_lifetime_signal_is_not_observed() {
        let mut app = topology_app();
        let entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            LOW_DPI_SCALE,
        );
        app.update();
        reset_activity(&mut app);
        let changed = app
            .world_mut()
            .entity_mut(entity)
            .get_mut::<Monitor>()
            .map(|mut monitor| {
                monitor.physical_position = IVec2::new(MONITOR_WIDTH.to_i32(), 0);
                monitor.scale_factor = DEFAULT_SCALE;
            });
        assert!(changed.is_some());

        app.update();

        assert_eq!(
            app.world().resource::<Monitors>().first().physical_position,
            IVec2::ZERO
        );
        assert!(
            (app.world().resource::<Monitors>().first().scale - LOW_DPI_SCALE).abs()
                < SCALE_FACTOR_EPSILON
        );
        assert_eq!(
            app.world().resource::<InjectedMonitorEvidence>().activity,
            TopologyProducerActivity::default()
        );
    }

    #[test]
    fn persistence_schedule_is_idle_and_rebases_from_returned_monitor_metadata()
    -> Result<(), String> {
        let state_file = NamedTempFile::new().map_err(|error| error.to_string())?;
        let installed_position = IVec2::new(-1_920, 0);
        let window_position = IVec2::new(-1_760, 90);
        let same_entity_edit = IVec2::new(-3_840, -200);
        let mut app = topology_app();
        app.insert_resource(ManagedWindowPersistence::RememberAll)
            .insert_resource(RestoreWindowConfig {
                path: state_file.path().to_path_buf(),
            })
            .init_resource::<InjectedWindowPositions>()
            .add_plugins(PersistencePlugin);
        let monitor_entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            installed_position,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            LOW_DPI_SCALE,
        );
        let mut window = Window {
            position: WindowPosition::At(window_position),
            ..default()
        };
        window.resolution.set(800.0, 600.0);
        let window_entity = app.world_mut().spawn((window, PrimaryWindow)).id();
        app.world_mut()
            .resource_mut::<InjectedCurrentMonitorSource>()
            .set_position(window_entity, Some(window_position));
        app.world_mut()
            .resource_mut::<InjectedWindowPositions>()
            .set(window_entity, Some(window_position));

        app.update();

        let installed_capture = app
            .world()
            .resource::<CapturedWindowStates>()
            .captured_placement(&WindowKey::Primary)
            .cloned()
            .ok_or_else(|| "primary window should have captured placement".to_string())?;
        assert_capture_snapshot(&installed_capture, installed_position, LOW_DPI_SCALE);

        reset_persistence_activity(&mut app);

        app.update();

        assert_persistence_idle(&app);

        let edited = app
            .world_mut()
            .entity_mut(monitor_entity)
            .get_mut::<Monitor>()
            .map(|mut monitor| {
                monitor.physical_position = same_entity_edit;
                monitor.scale_factor = DEFAULT_SCALE;
            });
        assert!(edited.is_some());
        app.update();

        assert_persistence_idle(&app);
        let retained_capture = app
            .world()
            .resource::<CapturedWindowStates>()
            .captured_placement(&WindowKey::Primary);
        assert_eq!(
            retained_capture.map(|placement| placement.monitor_snapshot),
            Some(installed_capture.monitor_snapshot)
        );

        app.world_mut().entity_mut(window_entity).remove::<Window>();
        app.world_mut().flush();
        remove_monitor(&mut app, monitor_entity);
        app.update();
        let returned_entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            RETURNED_MONITOR_POSITION,
            RETURNED_MONITOR_SIZE,
            DEFAULT_SCALE,
        );
        app.update();

        let returned_monitor = installed_monitor(&app, returned_entity)
            .ok_or_else(|| "returned monitor should be installed".to_string())?;
        assert_eq!(
            returned_monitor.physical_position,
            RETURNED_MONITOR_POSITION
        );
        assert!((returned_monitor.scale - DEFAULT_SCALE).abs() < SCALE_FACTOR_EPSILON);
        assert_eq!(
            installed_capture.rebased_physical_position(&returned_monitor),
            Some(RETURNED_MONITOR_POSITION + IVec2::new(320, 180))
        );
        Ok(())
    }

    #[test]
    fn fallback_identity_revalidation_preserves_registered_target() -> Result<(), String> {
        let state_file = NamedTempFile::new().map_err(|error| error.to_string())?;
        let fallback_position = IVec2::new(MONITOR_WIDTH.to_i32(), 0);
        let mut app = phase_seven_topology_app(state_file.path());
        app.add_plugins(TimePlugin)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        let target_entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        let fallback_entity = spawn_monitor(
            &mut app,
            PANEL_B_EVIDENCE,
            fallback_position,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        app.update();
        let target = installed_monitor(&app, target_entity)
            .ok_or_else(|| "target monitor should be installed".to_string())?;
        let fallback = installed_monitor(&app, fallback_entity)
            .ok_or_else(|| "fallback monitor should be installed".to_string())?;
        let window_entity = spawn_accepted_automatic_primary(
            &mut app,
            target_entity,
            target,
            WindowMode::BorderlessFullscreen(MonitorSelection::Index(target.index)),
            None,
        );
        let fallback = enter_on_fallback(
            &mut app,
            window_entity,
            target_entity,
            fallback_entity,
            fallback,
        )?;

        let original_placement = app
            .world()
            .resource::<CapturedWindowStates>()
            .captured_placement(&WindowKey::Primary)
            .cloned()
            .ok_or_else(|| "fallback recovery should retain captured placement".to_string())?;
        assert_primary_capture_state(&app, &original_placement, PersistenceWriteState::Frozen);
        set_window_monitor(&mut app, window_entity, fallback, WindowMode::Windowed);
        app.update();
        let recovery = recovery::fallback_and_return_snapshot(app.world(), &WindowKey::Primary);
        assert_eq!(
            recovery.map(|snapshot| (snapshot.phase, snapshot.fallback_monitor)),
            Some((FallbackAndReturnPhaseSnapshot::OnFallback, Some(fallback))),
        );
        assert_primary_capture_state(&app, &original_placement, PersistenceWriteState::Frozen);
        let revision_before_revalidation = *app.world().resource::<MonitorTopologyRevision>();
        app.world_mut()
            .resource_mut::<InjectedMonitorEvidence>()
            .evidence
            .insert(
                fallback_entity,
                Err(OperatingSystemQueryError::StableIdentityProperty.into()),
            );
        app.world()
            .resource::<MonitorConfiguration>()
            .advance_for_test();

        app.update();

        let revalidated = installed_monitor(&app, fallback_entity)
            .ok_or_else(|| "revalidated monitor should remain installed".to_string())?;
        assert_ne!(revalidated.identity, fallback.identity);
        assert_ne!(
            *app.world().resource::<MonitorTopologyRevision>(),
            revision_before_revalidation,
        );
        assert_eq!(
            app.world()
                .get::<CurrentMonitor>(window_entity)
                .map(|current_monitor| current_monitor.monitor_info),
            Some(revalidated),
        );
        let recovery = recovery::fallback_and_return_snapshot(app.world(), &WindowKey::Primary);
        assert_eq!(
            recovery.map(|snapshot| {
                (
                    snapshot.phase,
                    snapshot.fallback_monitor,
                    snapshot.intent_count,
                )
            }),
            Some((
                FallbackAndReturnPhaseSnapshot::OnFallback,
                Some(revalidated),
                0,
            )),
        );
        assert_primary_capture_state(&app, &original_placement, PersistenceWriteState::Frozen);
        Ok(())
    }

    #[test]
    fn deleted_link_classifies_scheduled_loss_then_queues_one_internal_return() -> Result<(), String>
    {
        let state_file = NamedTempFile::new().map_err(|error| error.to_string())?;
        let fallback_position = IVec2::new(MONITOR_WIDTH.to_i32(), 0);
        let mut app = phase_seven_topology_app(state_file.path());
        let target_entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        spawn_monitor(
            &mut app,
            PANEL_B_EVIDENCE,
            fallback_position,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        app.update();
        let target = installed_monitor(&app, target_entity)
            .ok_or_else(|| "target monitor should be installed".to_string())?;
        let window_position = target.physical_position + IVec2::ONE;
        let window_entity = spawn_accepted_automatic_primary(
            &mut app,
            target_entity,
            target,
            WindowMode::Windowed,
            Some(window_position),
        );
        let retained_placement = app
            .world()
            .resource::<CapturedWindowStates>()
            .captured_placement(&WindowKey::Primary)
            .cloned()
            .ok_or_else(|| "registered window should have captured placement".to_string())?;

        app.world_mut().entity_mut(window_entity).remove::<Window>();
        app.world_mut().flush();
        let recovery = recovery::fallback_and_return_snapshot(app.world(), &WindowKey::Primary);
        assert_eq!(
            recovery.map(|snapshot| (snapshot.phase, snapshot.fallback_monitor)),
            Some((FallbackAndReturnPhaseSnapshot::RemovalPending, None)),
        );
        assert_primary_capture_state(&app, &retained_placement, PersistenceWriteState::Frozen);

        remove_monitor(&mut app, target_entity);
        app.update();
        let recovery = recovery::fallback_and_return_snapshot(app.world(), &WindowKey::Primary);
        assert_eq!(
            recovery
                .map(|snapshot| { (snapshot.phase, snapshot.fallback_monitor, snapshot.intent) }),
            Some((FallbackAndReturnPhaseSnapshot::FallbackSettling, None, None,)),
        );
        let replacement = app
            .world()
            .resource::<RecoveryRegistrations>()
            .by_key(&WindowKey::Primary)
            .and_then(|registration| registration.entity)
            .ok_or_else(|| "fallback replacement should be bound".to_string())?;
        assert_ne!(replacement, window_entity);
        assert!(app.world().get::<Window>(replacement).is_some());
        assert!(app.world().get::<PrimaryWindow>(replacement).is_some());
        assert!(app.world().get::<PrimaryWindow>(window_entity).is_none());

        let returned_entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            RETURNED_MONITOR_POSITION,
            RETURNED_MONITOR_SIZE,
            DEFAULT_SCALE,
        );
        app.update();
        let returned = installed_monitor(&app, returned_entity)
            .ok_or_else(|| "returned target should be installed".to_string())?;
        assert_eq!(returned.identity, target.identity);
        let revision = *app.world().resource::<MonitorTopologyRevision>();
        let recovery = recovery::fallback_and_return_snapshot(app.world(), &WindowKey::Primary);
        assert_eq!(
            recovery.map(|snapshot| {
                (
                    snapshot.phase,
                    snapshot.fallback_monitor,
                    snapshot.intent_count,
                    snapshot
                        .intent
                        .map(|intent| (intent.entity, intent.monitor, intent.revision)),
                )
            }),
            Some((
                FallbackAndReturnPhaseSnapshot::FallbackSettling,
                None,
                1,
                Some((Some(replacement), returned, revision)),
            )),
        );
        assert_primary_capture_state(&app, &retained_placement, PersistenceWriteState::Frozen);
        let observations = app.world().resource::<RecoveryObservations>();
        assert_eq!(observations.restored, 0);
        assert_eq!(observations.mismatched, 0);
        Ok(())
    }

    #[test]
    fn on_monitor_association_repairs_initial_managed_fallback_before_persistence()
    -> Result<(), String> {
        let state_file = NamedTempFile::new().map_err(|error| error.to_string())?;
        let second_monitor_position = IVec2::new(MONITOR_WIDTH.to_i32(), 0);
        let window_position = second_monitor_position + IVec2::new(120, 80);
        let window_key = WindowKey::Managed("secondary".to_string());
        let mut app = phase_seven_topology_app(state_file.path());
        spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        let second_monitor_entity = spawn_monitor(
            &mut app,
            PANEL_B_EVIDENCE,
            second_monitor_position,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            LOW_DPI_SCALE,
        );
        app.update();
        reset_persistence_activity(&mut app);

        let window_entity = app
            .world_mut()
            .spawn((
                Window::default(),
                ManagedWindow {
                    name: "secondary".to_string(),
                },
                WindowRecovery::ApplicationControlled,
            ))
            .id();
        app.world_mut()
            .resource_mut::<InjectedCurrentMonitorSource>()
            .set_position(window_entity, None);
        app.world_mut()
            .resource_mut::<InjectedWindowPositions>()
            .set(window_entity, Some(window_position));

        app.update();
        assert_initial_managed_fallback(&app, window_entity, &window_key, state_file.path())?;

        reset_persistence_activity(&mut app);

        app.insert_resource(ScheduledMonitorAssociation {
            window_entity,
            monitor_entity: second_monitor_entity,
        });
        app.update();

        let installed_monitor = installed_monitor(&app, second_monitor_entity)
            .ok_or_else(|| "associated monitor should be installed".to_string())?;
        assert_repaired_monitor_association(&app, window_entity, installed_monitor);

        app.update();
        assert_corrected_managed_capture(&app, &window_key, installed_monitor, state_file.path())?;
        assert_eq!(
            app.world().resource::<InjectedMonitorEvidence>().activity,
            TopologyProducerActivity::default()
        );
        let native_current_monitor_lookups = app
            .world()
            .resource::<InjectedCurrentMonitorSource>()
            .lookups;
        assert_eq!(native_current_monitor_lookups, 0);

        app.update();
        assert_accepted_registration(&app, &window_key, installed_monitor);

        reset_persistence_activity(&mut app);
        app.update();
        app.update();

        assert_persistence_idle(&app);
        assert_accepted_registration(&app, &window_key, installed_monitor);
        Ok(())
    }

    #[test]
    fn idle_update_performs_no_topology_identity_or_window_lookup() {
        let mut app = topology_app();
        spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        let window_entity = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.world_mut()
            .resource_mut::<InjectedCurrentMonitorSource>()
            .set_position(window_entity, Some(IVec2::ZERO));
        app.update();
        reset_activity(&mut app);

        app.update();

        assert_eq!(
            app.world().resource::<InjectedMonitorEvidence>().activity,
            TopologyProducerActivity::default()
        );
        assert_eq!(
            app.world()
                .resource::<InjectedCurrentMonitorSource>()
                .lookups,
            0
        );
    }

    #[test]
    fn relevant_window_change_refreshes_current_monitor_once() {
        let mut app = topology_app();
        spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        let second_position = IVec2::new(MONITOR_WIDTH.to_i32(), 0);
        spawn_monitor(
            &mut app,
            PANEL_B_EVIDENCE,
            second_position,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        let window_entity = app
            .world_mut()
            .spawn((
                Window {
                    position: WindowPosition::At(IVec2::ZERO),
                    ..Default::default()
                },
                PrimaryWindow,
            ))
            .id();
        app.world_mut()
            .resource_mut::<InjectedCurrentMonitorSource>()
            .set_position(window_entity, Some(IVec2::ZERO));
        app.update();
        reset_activity(&mut app);
        let changed = app
            .world_mut()
            .entity_mut(window_entity)
            .get_mut::<Window>()
            .map(|mut window| {
                window.position = WindowPosition::At(second_position);
                window.mode = WindowMode::Windowed;
            });
        assert!(changed.is_some());
        app.world_mut()
            .resource_mut::<InjectedCurrentMonitorSource>()
            .set_position(window_entity, Some(second_position));

        app.update();

        let expected_monitor = app
            .world()
            .resource::<Monitors>()
            .at(second_position.x, second_position.y)
            .copied();
        assert_eq!(
            expected_monitor.map(|monitor| monitor.physical_position),
            Some(second_position)
        );
        assert_eq!(
            app.world()
                .resource::<InjectedCurrentMonitorSource>()
                .lookups,
            1
        );
        assert_eq!(
            app.world()
                .entity(window_entity)
                .get::<CurrentMonitor>()
                .map(|current_monitor| current_monitor.monitor_info),
            expected_monitor
        );
    }

    #[test]
    fn empty_topology_clears_current_monitor_before_restore_and_settle() {
        let mut app = topology_app();
        let monitor_entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        let window_entity = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.world_mut()
            .resource_mut::<InjectedCurrentMonitorSource>()
            .set_position(window_entity, Some(IVec2::ZERO));
        app.update();
        assert!(
            app.world()
                .entity(window_entity)
                .contains::<CurrentMonitor>()
        );
        remove_monitor(&mut app, monitor_entity);

        app.update();

        assert!(
            !app.world()
                .entity(window_entity)
                .contains::<CurrentMonitor>()
        );
        let observations = app.world().resource::<DownstreamObservations>();
        assert_eq!(observations.restore_current_monitors, 0);
        assert_eq!(observations.settle_current_monitors, 0);
        assert_eq!(
            app.world()
                .resource::<TopologyObservations>()
                .current_monitors_at_disconnect,
            [0]
        );
    }

    #[test]
    fn cached_instance_skips_native_evidence_during_another_addition() {
        let mut app = topology_app();
        spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        app.update();
        spawn_monitor(
            &mut app,
            PANEL_B_EVIDENCE,
            IVec2::new(MONITOR_WIDTH.to_i32(), 0),
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        reset_activity(&mut app);

        app.update();

        assert_eq!(
            app.world().resource::<InjectedMonitorEvidence>().activity,
            TopologyProducerActivity {
                topology_scans:       1,
                component_reads:      2,
                cached_order_lookups: 2,
                identity_requests:    2,
                handle_lookups:       1,
                evidence_loads:       1,
            }
        );
    }

    #[test]
    fn configuration_failure_is_injected_without_physical_display_setup() {
        let mut app = topology_app();
        let entity = spawn_monitor(
            &mut app,
            PANEL_A_EVIDENCE,
            IVec2::ZERO,
            UVec2::new(MONITOR_WIDTH, MONITOR_HEIGHT),
            DEFAULT_SCALE,
        );
        app.update();
        app.world_mut()
            .resource_mut::<InjectedMonitorEvidence>()
            .evidence
            .insert(
                entity,
                Err(OperatingSystemQueryError::StableIdentityProperty.into()),
            );
        app.world()
            .resource::<MonitorConfiguration>()
            .advance_for_test();

        app.update();

        assert_eq!(
            app.world().resource::<Monitors>().first().identity,
            MonitorIdentity::Unverified
        );
    }
}
