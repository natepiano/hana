use bevy::prelude::*;

use super::identity::MonitorConfigurationState;
use super::identity::MonitorIdentity;
use super::identity::MonitorIdentityProbe;
use super::identity::MonitorIdentityRegistry;
use super::identity::MonitorInstanceId;
use super::topology::MonitorChanges;
use super::topology::MonitorTopologyRevision;
use super::topology::Monitors;
use crate::constants::MONITOR_PROBE_TARGET;

#[derive(Clone, Debug)]
pub(super) struct TopologyProbeRecord {
    frame_count:   u32,
    schedule:      TopologyProducerSchedule,
    configuration: MonitorConfigurationState,
    revision:      MonitorTopologyRevision,
    instance_id:   MonitorInstanceId,
    evidence:      MonitorIdentityProbe,
    identity:      MonitorIdentity,
    entity:        Entity,
    entity_state:  MonitorEntityState,
    change:        TopologyChangeKind,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CapturedTopologyProbeRecord {
    pub(super) frame_count:              u32,
    pub(super) schedule:                 &'static str,
    pub(super) configuration_state:      &'static str,
    pub(super) configuration_generation: Option<u64>,
    pub(super) revision:                 u64,
    pub(super) evidence_provenance:      &'static str,
    pub(super) evidence_generation:      Option<u64>,
    pub(super) entity:                   Entity,
    pub(super) change:                   &'static str,
}

#[cfg(test)]
#[derive(Default, Resource)]
pub(super) struct InjectedTopologyProbeRecords {
    pub(super) records: Vec<CapturedTopologyProbeRecord>,
}

pub(super) struct FormerIdentityProbe {
    pub(super) instance_id:    MonitorInstanceId,
    pub(super) identity_probe: MonitorIdentityProbe,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum TopologyProducerSchedule {
    PreStartup,
    Update,
}

impl TopologyProducerSchedule {
    const fn label(self) -> &'static str {
        match self {
            Self::PreStartup => "PreStartup::init_monitors",
            Self::Update => "Update::monitor_topology_producer",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum TopologyChangeKind {
    Connected,
    Disconnected,
    IdentityChanged,
    RevalidatedUnchanged,
}

impl TopologyChangeKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Disconnected => "disconnected",
            Self::IdentityChanged => "identity-changed",
            Self::RevalidatedUnchanged => "revalidated-unchanged",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum MonitorEntityState {
    Current,
    Former,
}

impl MonitorEntityState {
    const fn label(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Former => "former",
        }
    }
}

impl TopologyProbeRecord {
    pub(super) fn emit(&self) {
        let (configuration_state, configuration_generation) = self.configuration.probe_fields();
        let (evidence_provenance, evidence_generation) =
            self.evidence.evidence_fields(self.configuration);
        let verified_monitor_id = match self.identity {
            MonitorIdentity::Verified(id) => Some(id),
            MonitorIdentity::Unverified => None,
        };
        tracing::info!(
            target: MONITOR_PROBE_TARGET,
            frame_count = u64::from(self.frame_count),
            producer_schedule = self.schedule.label(),
            configuration = ?self.configuration,
            configuration_state,
            configuration_generation = ?configuration_generation,
            topology_revision = self.revision.get(),
            monitor_instance = ?self.instance_id,
            evidence_provenance,
            evidence_generation = ?evidence_generation,
            monitor_identity = ?self.identity,
            verified_monitor_id = ?verified_monitor_id,
            monitor_entity = ?self.entity,
            monitor_entity_state = self.entity_state.label(),
            topology_change = self.change.label(),
            "installed monitor topology"
        );
    }
}

#[cfg(test)]
pub(super) fn capture_record(world: &mut World, record: &TopologyProbeRecord) {
    let (configuration_state, configuration_generation) = record.configuration.probe_fields();
    let (evidence_provenance, evidence_generation) =
        record.evidence.evidence_fields(record.configuration);
    if let Some(mut captured) = world.get_resource_mut::<InjectedTopologyProbeRecords>() {
        captured.records.push(CapturedTopologyProbeRecord {
            frame_count: record.frame_count,
            schedule: record.schedule.label(),
            configuration_state,
            configuration_generation,
            revision: record.revision.get(),
            evidence_provenance,
            evidence_generation,
            entity: record.entity,
            change: record.change.label(),
        });
    }
}

pub(super) fn changed_probe_records(
    changes: &MonitorChanges,
    former_identity_probes: &[FormerIdentityProbe],
    identity_registry: &MonitorIdentityRegistry,
    frame_count: u32,
    schedule: TopologyProducerSchedule,
    configuration: MonitorConfigurationState,
    revision: MonitorTopologyRevision,
) -> Vec<TopologyProbeRecord> {
    let mut records = Vec::new();
    records.extend(changes.connected.iter().map(|monitor| TopologyProbeRecord {
        frame_count,
        schedule,
        configuration,
        revision,
        instance_id: monitor.instance_id,
        evidence: identity_registry.probe(monitor.instance_id),
        identity: monitor.monitor_info.identity,
        entity: monitor.entity,
        entity_state: MonitorEntityState::Current,
        change: TopologyChangeKind::Connected,
    }));
    records.extend(
        changes
            .identity_changed
            .iter()
            .map(|monitor| TopologyProbeRecord {
                frame_count,
                schedule,
                configuration,
                revision,
                instance_id: monitor.instance_id,
                evidence: identity_registry.probe(monitor.instance_id),
                identity: monitor.monitor_info.identity,
                entity: monitor.entity,
                entity_state: MonitorEntityState::Current,
                change: TopologyChangeKind::IdentityChanged,
            }),
    );
    records.extend(changes.disconnected.iter().map(|monitor| {
        let evidence = former_identity_probes
            .iter()
            .find(|probe| probe.instance_id == monitor.instance_id)
            .map_or_else(MonitorIdentityProbe::unavailable, |probe| {
                probe.identity_probe
            });
        TopologyProbeRecord {
            frame_count,
            schedule,
            configuration,
            revision,
            instance_id: monitor.instance_id,
            evidence,
            identity: monitor.monitor_info.identity,
            entity: monitor.entity,
            entity_state: MonitorEntityState::Former,
            change: TopologyChangeKind::Disconnected,
        }
    }));
    records
}

pub(super) fn current_probe_records(
    monitors: &Monitors,
    identity_registry: &MonitorIdentityRegistry,
    frame_count: u32,
    schedule: TopologyProducerSchedule,
    configuration: MonitorConfigurationState,
    revision: MonitorTopologyRevision,
    change: TopologyChangeKind,
) -> Vec<TopologyProbeRecord> {
    monitors
        .live
        .iter()
        .map(|monitor| TopologyProbeRecord {
            frame_count,
            schedule,
            configuration,
            revision,
            instance_id: monitor.instance_id,
            evidence: identity_registry.probe(monitor.instance_id),
            identity: monitor.monitor_info.identity,
            entity: monitor.entity,
            entity_state: MonitorEntityState::Current,
            change,
        })
        .collect()
}
