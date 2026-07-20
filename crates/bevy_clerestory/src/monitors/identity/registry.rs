use std::collections::HashMap;

use bevy::prelude::*;
use thiserror::Error;

use super::MonitorConfigurationState;
use super::MonitorId;
use super::MonitorIdentity;
use super::configuration::MonitorConfigurationGeneration;
use super::native::QualifiedEvidence;
use crate::Platform;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum OperatingSystemQueryError {
    #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
    #[error("display-configuration query failed")]
    DisplayConfiguration,
    #[error("monitor configuration-notification registration failed")]
    ConfigurationNotificationRegistration,
    #[cfg(any(test, target_os = "windows", all(unix, not(target_os = "macos"))))]
    #[error("monitor configuration-notification stream failed")]
    ConfigurationNotificationStream,
    #[error("monitor configuration-notification removal failed")]
    ConfigurationNotificationRemoval,
    #[cfg(target_os = "windows")]
    #[error("monitor device-interface query failed")]
    MonitorDeviceInterface,
    #[cfg(any(test, target_os = "windows", all(unix, not(target_os = "macos"))))]
    #[error("stable monitor identity-property query failed")]
    StableIdentityProperty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum MonitorIdentificationError {
    #[error("monitor has no native winit monitor handle")]
    MissingMonitorHandle,
    #[error("operating-system monitor query failed: {0}")]
    OperatingSystemQuery(#[from] OperatingSystemQueryError),
    #[cfg(any(test, target_os = "windows", all(unix, not(target_os = "macos"))))]
    #[error(
        "stable physical monitor identity data is missing, incomplete, malformed, or a placeholder"
    )]
    InvalidStableIdentity,
    #[error("physical monitor identity is permanently ambiguous")]
    AmbiguousPhysicalIdentity,
    #[error("one monitor instance reported contradictory physical identities")]
    ContradictoryPhysicalIdentity,
    #[error("process-local monitor identity token space is exhausted")]
    TokenExhausted,
    #[error("platform cannot expose stable physical monitor identity")]
    StablePhysicalIdentityUnavailable,
    #[error("monitor configuration generation is exhausted")]
    ConfigurationGenerationExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MonitorInstanceId(Entity);

impl From<Entity> for MonitorInstanceId {
    fn from(entity: Entity) -> Self { Self(entity) }
}

#[derive(Debug, Resource)]
pub struct MonitorIdentityRegistry {
    configuration:    Option<MonitorConfigurationState>,
    evidence_indices: HashMap<QualifiedEvidence, usize>,
    evidence_records: Vec<EvidenceRecord>,
    instances:        HashMap<MonitorInstanceId, InstanceIdentity>,
    next_id:          Option<u64>,
}

#[cfg(feature = "monitor-probe")]
#[derive(Clone, Copy, Debug)]
pub struct MonitorIdentityProbe {
    evidence: EvidenceProbe,
}

#[cfg(feature = "monitor-probe")]
impl MonitorIdentityProbe {
    pub const fn unavailable() -> Self {
        Self {
            evidence: EvidenceProbe::Unavailable,
        }
    }

    pub fn evidence_fields(
        &self,
        configuration: MonitorConfigurationState,
    ) -> (&'static str, Option<u64>) {
        match self.evidence {
            EvidenceProbe::Observed { generation }
                if matches!(
                    configuration,
                    MonitorConfigurationState::Ready(current_generation)
                        if current_generation == generation
                ) =>
            {
                ("observed-current-generation", Some(generation.get()))
            },
            EvidenceProbe::Observed { generation } | EvidenceProbe::Retained { generation } => {
                ("retained-earlier-generation", Some(generation.get()))
            },
            EvidenceProbe::Unavailable => ("unavailable", None),
        }
    }
}

impl Default for MonitorIdentityRegistry {
    fn default() -> Self {
        Self {
            configuration:    None,
            evidence_indices: HashMap::new(),
            evidence_records: Vec::new(),
            instances:        HashMap::new(),
            next_id:          Some(0),
        }
    }
}

impl MonitorIdentityRegistry {
    pub fn configuration_changed(&self, state: MonitorConfigurationState) -> bool {
        self.configuration != Some(state)
    }

    pub const fn observe_configuration(&mut self, state: MonitorConfigurationState) {
        self.configuration = Some(state);
    }

    pub fn active_instances(&self) -> impl Iterator<Item = MonitorInstanceId> + '_ {
        self.instances.keys().copied()
    }

    pub(super) fn cached_identity(
        &self,
        instance_id: MonitorInstanceId,
    ) -> Option<MonitorIdentity> {
        self.instances
            .get(&instance_id)
            .map(|instance| instance.identity)
    }

    pub(super) fn monitor_handle_missing(
        &mut self,
        instance_id: MonitorInstanceId,
        configuration: MonitorConfigurationState,
    ) {
        self.cache_read_failure(
            instance_id,
            configuration,
            MonitorIdentificationError::MissingMonitorHandle,
        );
    }

    pub fn identity<F>(
        &mut self,
        instance_id: MonitorInstanceId,
        configuration: MonitorConfigurationState,
        load_evidence: F,
        platform: Platform,
    ) -> MonitorIdentity
    where
        F: FnOnce() -> Result<QualifiedEvidence, MonitorIdentificationError>,
    {
        if self
            .instances
            .get(&instance_id)
            .is_some_and(|instance| instance.configuration == configuration)
        {
            return self.instances[&instance_id].identity;
        }
        let generation = match configuration {
            MonitorConfigurationState::Ready(generation) => generation,
            MonitorConfigurationState::Unavailable(error) => {
                return self.cache_read_failure(instance_id, configuration, error);
            },
        };
        if platform.is_wayland() {
            return self.cache_read_failure(
                instance_id,
                configuration,
                MonitorIdentificationError::StablePhysicalIdentityUnavailable,
            );
        }

        match load_evidence() {
            Ok(evidence) => self.accept_evidence(instance_id, generation, evidence),
            Err(error) => self.cache_read_failure(instance_id, configuration, error),
        }
    }

    #[cfg(feature = "monitor-probe")]
    pub fn probe(&self, instance_id: MonitorInstanceId) -> MonitorIdentityProbe {
        let evidence =
            self.instances
                .get(&instance_id)
                .map_or(EvidenceProbe::Unavailable, |instance| {
                    match (&instance.evidence, instance.provenance) {
                        (Some(_), EvidenceProvenance::Observed { generation }) => {
                            EvidenceProbe::Observed { generation }
                        },
                        (Some(_), EvidenceProvenance::Retained { generation }) => {
                            EvidenceProbe::Retained { generation }
                        },
                        _ => EvidenceProbe::Unavailable,
                    }
                });
        MonitorIdentityProbe { evidence }
    }

    pub fn disconnect(&mut self, instance_id: MonitorInstanceId) {
        let Some(instance_identity) = self.instances.remove(&instance_id) else {
            return;
        };
        let Some(evidence) = instance_identity.evidence else {
            return;
        };
        let Some(record) = self.record_mut(&evidence) else {
            return;
        };
        if let EvidenceStatus::Verified {
            id,
            active_instance: Some(active_instance),
        } = record.status
            && active_instance == instance_id
        {
            record.status = EvidenceStatus::Verified {
                id,
                active_instance: None,
            };
        }
    }

    fn accept_evidence(
        &mut self,
        instance_id: MonitorInstanceId,
        generation: MonitorConfigurationGeneration,
        evidence: QualifiedEvidence,
    ) -> MonitorIdentity {
        if let Some(previous) = self.instances.get(&instance_id)
            && previous.evidence.as_ref() != Some(&evidence)
        {
            let previous_evidence = previous.evidence.clone();
            if let Some(previous_evidence) = previous_evidence {
                self.mark_ambiguous(&previous_evidence);
            }
            self.mark_ambiguous(&evidence);
            return self.cache_unverified(
                instance_id,
                generation,
                evidence,
                MonitorIdentificationError::ContradictoryPhysicalIdentity,
            );
        }

        let Some(record_index) = self.record_index(&evidence) else {
            let Some(id) = self.allocate_id() else {
                return self.cache_unverified(
                    instance_id,
                    generation,
                    evidence,
                    MonitorIdentificationError::TokenExhausted,
                );
            };
            self.install_record(
                evidence.clone(),
                EvidenceStatus::Verified {
                    id,
                    active_instance: Some(instance_id),
                },
            );
            return self.cache_verified(instance_id, generation, evidence, id);
        };

        match self.evidence_records[record_index].status {
            EvidenceStatus::Verified {
                id,
                active_instance: None,
            } => {
                self.evidence_records[record_index].status = EvidenceStatus::Verified {
                    id,
                    active_instance: Some(instance_id),
                };
                self.cache_verified(instance_id, generation, evidence, id)
            },
            EvidenceStatus::Verified {
                id,
                active_instance: Some(active_instance),
            } if active_instance == instance_id => {
                self.cache_verified(instance_id, generation, evidence, id)
            },
            EvidenceStatus::Verified {
                active_instance: Some(active_instance),
                ..
            } => {
                self.evidence_records[record_index].status = EvidenceStatus::Ambiguous;
                if let Some(active_identity) = self.instances.get_mut(&active_instance) {
                    active_identity.identity = MonitorIdentity::Unverified;
                    active_identity.error =
                        Some(MonitorIdentificationError::AmbiguousPhysicalIdentity);
                }
                warn!(
                    "[MonitorIdentityRegistry] duplicate physical-panel evidence for {active_instance:?} and {instance_id:?}; evidence is permanently ambiguous"
                );
                self.cache_unverified(
                    instance_id,
                    generation,
                    evidence,
                    MonitorIdentificationError::AmbiguousPhysicalIdentity,
                )
            },
            EvidenceStatus::Ambiguous => self.cache_unverified(
                instance_id,
                generation,
                evidence,
                MonitorIdentificationError::AmbiguousPhysicalIdentity,
            ),
        }
    }

    fn allocate_id(&mut self) -> Option<MonitorId> {
        let raw = self.next_id?;
        self.next_id = raw.checked_add(1);
        Some(MonitorId::from_raw(raw))
    }

    fn cache_verified(
        &mut self,
        instance_id: MonitorInstanceId,
        generation: MonitorConfigurationGeneration,
        evidence: QualifiedEvidence,
        id: MonitorId,
    ) -> MonitorIdentity {
        let identity = MonitorIdentity::Verified(id);
        self.instances.insert(
            instance_id,
            InstanceIdentity {
                evidence: Some(evidence),
                evidence_generation: Some(generation),
                provenance: EvidenceProvenance::observed(generation),
                identity,
                error: None,
                configuration: MonitorConfigurationState::Ready(generation),
            },
        );
        identity
    }

    fn cache_unverified(
        &mut self,
        instance_id: MonitorInstanceId,
        generation: MonitorConfigurationGeneration,
        evidence: QualifiedEvidence,
        error: MonitorIdentificationError,
    ) -> MonitorIdentity {
        log_identification_error(instance_id, error);
        self.instances.insert(
            instance_id,
            InstanceIdentity {
                evidence:            Some(evidence),
                evidence_generation: Some(generation),
                provenance:          EvidenceProvenance::observed(generation),
                identity:            MonitorIdentity::Unverified,
                error:               Some(error),
                configuration:       MonitorConfigurationState::Ready(generation),
            },
        );
        MonitorIdentity::Unverified
    }

    fn cache_read_failure(
        &mut self,
        instance_id: MonitorInstanceId,
        configuration: MonitorConfigurationState,
        error: MonitorIdentificationError,
    ) -> MonitorIdentity {
        log_identification_error(instance_id, error);
        if let Some(instance) = self.instances.get_mut(&instance_id) {
            instance.identity = MonitorIdentity::Unverified;
            instance.error = Some(error);
            instance.configuration = configuration;
            instance.provenance = instance.evidence_generation.map_or(
                EvidenceProvenance::Unavailable,
                EvidenceProvenance::retained,
            );
        } else {
            self.instances.insert(
                instance_id,
                InstanceIdentity {
                    evidence: None,
                    evidence_generation: None,
                    provenance: EvidenceProvenance::Unavailable,
                    identity: MonitorIdentity::Unverified,
                    error: Some(error),
                    configuration,
                },
            );
        }
        MonitorIdentity::Unverified
    }

    fn mark_ambiguous(&mut self, evidence: &QualifiedEvidence) {
        let active_instance = if let Some(record) = self.record_mut(evidence) {
            let active_instance = match record.status {
                EvidenceStatus::Verified {
                    active_instance, ..
                } => active_instance,
                EvidenceStatus::Ambiguous => None,
            };
            record.status = EvidenceStatus::Ambiguous;
            active_instance
        } else {
            self.install_record(evidence.clone(), EvidenceStatus::Ambiguous);
            None
        };
        if let Some(active_identity) =
            active_instance.and_then(|instance_id| self.instances.get_mut(&instance_id))
        {
            active_identity.identity = MonitorIdentity::Unverified;
            active_identity.error = Some(MonitorIdentificationError::AmbiguousPhysicalIdentity);
        }
    }

    fn record_index(&self, evidence: &QualifiedEvidence) -> Option<usize> {
        self.evidence_indices.get(evidence).copied()
    }

    fn record_mut(&mut self, evidence: &QualifiedEvidence) -> Option<&mut EvidenceRecord> {
        let index = self.record_index(evidence)?;
        let record = self.evidence_records.get_mut(index)?;
        debug_assert_eq!(
            record.evidence, *evidence,
            "evidence index must remain exact"
        );
        Some(record)
    }

    fn install_record(&mut self, evidence: QualifiedEvidence, status: EvidenceStatus) -> usize {
        let index = self.evidence_records.len();
        self.evidence_records.push(EvidenceRecord {
            evidence: evidence.clone(),
            status,
        });
        let replaced = self.evidence_indices.insert(evidence, index);
        debug_assert!(replaced.is_none(), "evidence records are append-only");
        index
    }
}

#[derive(Clone, Debug)]
struct InstanceIdentity {
    evidence:            Option<QualifiedEvidence>,
    evidence_generation: Option<MonitorConfigurationGeneration>,
    provenance:          EvidenceProvenance,
    identity:            MonitorIdentity,
    error:               Option<MonitorIdentificationError>,
    configuration:       MonitorConfigurationState,
}

#[derive(Clone, Copy, Debug)]
enum EvidenceProvenance {
    Observed {
        #[cfg(feature = "monitor-probe")]
        generation: MonitorConfigurationGeneration,
    },
    Retained {
        #[cfg(feature = "monitor-probe")]
        generation: MonitorConfigurationGeneration,
    },
    Unavailable,
}

impl EvidenceProvenance {
    #[cfg(feature = "monitor-probe")]
    const fn observed(generation: MonitorConfigurationGeneration) -> Self {
        Self::Observed { generation }
    }

    #[cfg(not(feature = "monitor-probe"))]
    const fn observed(_: MonitorConfigurationGeneration) -> Self { Self::Observed {} }

    #[cfg(feature = "monitor-probe")]
    const fn retained(generation: MonitorConfigurationGeneration) -> Self {
        Self::Retained { generation }
    }

    #[cfg(not(feature = "monitor-probe"))]
    const fn retained(_: MonitorConfigurationGeneration) -> Self { Self::Retained {} }
}

#[cfg(feature = "monitor-probe")]
#[derive(Clone, Copy, Debug)]
enum EvidenceProbe {
    Observed {
        generation: MonitorConfigurationGeneration,
    },
    Retained {
        generation: MonitorConfigurationGeneration,
    },
    Unavailable,
}

#[derive(Debug)]
struct EvidenceRecord {
    evidence: QualifiedEvidence,
    status:   EvidenceStatus,
}

#[derive(Clone, Copy, Debug)]
enum EvidenceStatus {
    Verified {
        id:              MonitorId,
        active_instance: Option<MonitorInstanceId>,
    },
    Ambiguous,
}

fn log_identification_error(instance_id: MonitorInstanceId, error: MonitorIdentificationError) {
    match error {
        MonitorIdentificationError::TokenExhausted
        | MonitorIdentificationError::ConfigurationGenerationExhausted => {
            error!("[MonitorIdentityRegistry] monitor {instance_id:?} is unverified: {error}");
        },
        _ => {
            warn!("[MonitorIdentityRegistry] monitor {instance_id:?} is unverified: {error}");
        },
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    const INSTANCE_1: MonitorInstanceId = MonitorInstanceId(Entity::from_bits(1));
    const INSTANCE_2: MonitorInstanceId = MonitorInstanceId(Entity::from_bits(2));
    const INSTANCE_3: MonitorInstanceId = MonitorInstanceId(Entity::from_bits(3));
    const INSTANCE_4: MonitorInstanceId = MonitorInstanceId(Entity::from_bits(4));
    const PANEL_A_EVIDENCE: &[u8] = b"panel-a";
    const PANEL_B_EVIDENCE: &[u8] = b"panel-b";

    fn evidence(bytes: &[u8]) -> QualifiedEvidence { QualifiedEvidence::Synthetic(bytes.to_vec()) }

    fn state(generation: u64) -> MonitorConfigurationState {
        MonitorConfigurationState::Ready(generation.into())
    }

    #[test]
    fn cached_instance_does_not_invoke_identifier_loader_in_same_generation() {
        let mut registry = MonitorIdentityRegistry::default();
        let loader_calls = Cell::new(0);

        let first = registry.identity(
            INSTANCE_1,
            state(0),
            || {
                loader_calls.set(loader_calls.get() + 1);
                Ok(evidence(PANEL_A_EVIDENCE))
            },
            Platform::X11,
        );
        let cached = registry.identity(
            INSTANCE_1,
            state(0),
            || {
                loader_calls.set(loader_calls.get() + 1);
                Ok(evidence(PANEL_A_EVIDENCE))
            },
            Platform::X11,
        );

        assert_eq!(first, MonitorIdentity::Verified(MonitorId::from_raw(0)));
        assert_eq!(cached, first);
        assert_eq!(loader_calls.get(), 1);
        assert_eq!(registry.evidence_records.len(), 1);
        assert_eq!(registry.next_id, Some(1));
    }

    #[test]
    fn unchanged_unavailable_state_does_not_invoke_identifier_loader() {
        let mut registry = MonitorIdentityRegistry::default();
        let loader_calls = Cell::new(0);
        let unavailable = MonitorConfigurationState::Unavailable(
            OperatingSystemQueryError::ConfigurationNotificationStream.into(),
        );

        for _ in 0..2 {
            registry.identity(
                INSTANCE_1,
                unavailable,
                || {
                    loader_calls.set(loader_calls.get() + 1);
                    Ok(evidence(PANEL_A_EVIDENCE))
                },
                Platform::X11,
            );
        }

        assert_eq!(loader_calls.get(), 0);
        assert_eq!(registry.evidence_records.len(), 0);
        assert_eq!(registry.next_id, Some(0));
    }

    #[test]
    fn unchanged_evidence_in_new_generation_retains_token() {
        let mut registry = MonitorIdentityRegistry::default();
        let first = registry.identity(
            INSTANCE_1,
            state(0),
            || Ok(evidence(PANEL_A_EVIDENCE)),
            Platform::X11,
        );
        let revalidated = registry.identity(
            INSTANCE_1,
            state(1),
            || Ok(evidence(PANEL_A_EVIDENCE)),
            Platform::X11,
        );

        assert_eq!(revalidated, first);
    }

    #[test]
    fn contradictory_revalidation_never_reassigns_either_token() {
        let mut registry = MonitorIdentityRegistry::default();
        let original = registry.identity(
            INSTANCE_1,
            state(0),
            || Ok(evidence(b"before")),
            Platform::X11,
        );
        let contradiction = registry.identity(
            INSTANCE_1,
            state(1),
            || Ok(evidence(b"after")),
            Platform::X11,
        );
        registry.disconnect(INSTANCE_1);

        let old_reconnected = registry.identity(
            INSTANCE_2,
            state(1),
            || Ok(evidence(b"before")),
            Platform::X11,
        );
        let new_reconnected = registry.identity(
            INSTANCE_3,
            state(1),
            || Ok(evidence(b"after")),
            Platform::X11,
        );

        assert_eq!(original, MonitorIdentity::Verified(MonitorId::from_raw(0)));
        assert_eq!(contradiction, MonitorIdentity::Unverified);
        assert_eq!(old_reconnected, MonitorIdentity::Unverified);
        assert_eq!(new_reconnected, MonitorIdentity::Unverified);
    }

    #[test]
    fn query_failure_downgrades_without_tainting_successful_evidence() {
        let mut registry = MonitorIdentityRegistry::default();
        let original = registry.identity(
            INSTANCE_1,
            state(0),
            || Ok(evidence(PANEL_A_EVIDENCE)),
            Platform::X11,
        );
        let failed = registry.identity(
            INSTANCE_1,
            state(1),
            || Err(OperatingSystemQueryError::StableIdentityProperty.into()),
            Platform::X11,
        );
        assert!(matches!(
            registry.evidence_records[0].status,
            EvidenceStatus::Verified { .. }
        ));
        let retried = registry.identity(
            INSTANCE_1,
            state(2),
            || Ok(evidence(PANEL_A_EVIDENCE)),
            Platform::X11,
        );

        assert_eq!(failed, MonitorIdentity::Unverified);
        assert_eq!(retried, original);
        assert_eq!(
            registry.instances[&INSTANCE_1].error, None,
            "a successful retry clears the generation-local query error"
        );
    }

    #[test]
    fn tokens_are_monotonic_and_never_reassigned() {
        let mut registry = MonitorIdentityRegistry::default();
        let first = registry.accept_evidence(INSTANCE_1, 0_u64.into(), evidence(PANEL_A_EVIDENCE));
        registry.disconnect(INSTANCE_1);
        let reconnected =
            registry.accept_evidence(INSTANCE_2, 0_u64.into(), evidence(PANEL_A_EVIDENCE));
        let second = registry.accept_evidence(INSTANCE_3, 0_u64.into(), evidence(PANEL_B_EVIDENCE));

        assert_eq!(first, MonitorIdentity::Verified(MonitorId::from_raw(0)));
        assert_eq!(reconnected, first);
        assert_eq!(second, MonitorIdentity::Verified(MonitorId::from_raw(1)));
    }

    #[test]
    fn token_exhaustion_returns_unverified_without_reuse() {
        let mut registry = MonitorIdentityRegistry {
            next_id: Some(u64::MAX),
            ..Default::default()
        };

        let last = registry.accept_evidence(INSTANCE_1, 0_u64.into(), evidence(b"last-token"));
        let exhausted = registry.accept_evidence(INSTANCE_2, 0_u64.into(), evidence(b"no-token"));

        assert_eq!(
            last,
            MonitorIdentity::Verified(MonitorId::from_raw(u64::MAX))
        );
        assert_eq!(exhausted, MonitorIdentity::Unverified);
        assert_eq!(registry.evidence_records.len(), 1);
    }

    #[test]
    fn complete_evidence_participates_in_equality() {
        let mut registry = MonitorIdentityRegistry::default();
        let first = registry.accept_evidence(INSTANCE_1, 0_u64.into(), evidence(b"same-prefix-a"));
        let second = registry.accept_evidence(INSTANCE_2, 0_u64.into(), evidence(b"same-prefix-b"));

        assert_ne!(first, second);
    }

    #[test]
    fn old_evidence_uses_exact_index_after_large_history() {
        const HISTORY_LENGTH: u64 = 4_096;

        let mut registry = MonitorIdentityRegistry::default();
        for raw in 0..HISTORY_LENGTH {
            let instance_id = MonitorInstanceId(Entity::from_bits(raw + 1));
            let evidence = evidence(&raw.to_le_bytes());
            registry.accept_evidence(instance_id, 0_u64.into(), evidence);
            registry.disconnect(instance_id);
        }

        let oldest = evidence(&0_u64.to_le_bytes());
        assert_eq!(registry.evidence_indices.get(&oldest), Some(&0));
        assert_eq!(registry.record_index(&oldest), Some(0));
        assert_eq!(
            registry.accept_evidence(INSTANCE_1, 1_u64.into(), oldest),
            MonitorIdentity::Verified(MonitorId::from_raw(0))
        );
        assert_eq!(
            Some(registry.evidence_records.len()),
            usize::try_from(HISTORY_LENGTH).ok()
        );
    }

    #[test]
    fn duplicate_evidence_stays_ambiguous_after_disconnect() {
        let mut registry = MonitorIdentityRegistry::default();
        registry.accept_evidence(INSTANCE_1, 0_u64.into(), evidence(b"duplicate"));
        let duplicate = registry.accept_evidence(INSTANCE_2, 0_u64.into(), evidence(b"duplicate"));
        registry.disconnect(INSTANCE_2);
        let later = registry.accept_evidence(INSTANCE_3, 0_u64.into(), evidence(b"duplicate"));
        let unrelated = registry.accept_evidence(INSTANCE_4, 0_u64.into(), evidence(b"unrelated"));

        assert_eq!(duplicate, MonitorIdentity::Unverified);
        assert_eq!(
            registry.instances[&INSTANCE_1].identity,
            MonitorIdentity::Unverified
        );
        assert_eq!(later, MonitorIdentity::Unverified);
        assert_eq!(unrelated, MonitorIdentity::Verified(MonitorId::from_raw(1)));
    }

    #[test]
    fn identification_errors_have_stable_diagnostic_messages() {
        assert_eq!(
            MonitorIdentificationError::MissingMonitorHandle.to_string(),
            "monitor has no native winit monitor handle"
        );
        assert_eq!(
            MonitorIdentificationError::from(OperatingSystemQueryError::StableIdentityProperty)
                .to_string(),
            "operating-system monitor query failed: stable monitor identity-property query failed"
        );
        assert_eq!(
            MonitorIdentificationError::InvalidStableIdentity.to_string(),
            "stable physical monitor identity data is missing, incomplete, malformed, or a placeholder"
        );
        assert_eq!(
            MonitorIdentificationError::AmbiguousPhysicalIdentity.to_string(),
            "physical monitor identity is permanently ambiguous"
        );
        assert_eq!(
            MonitorIdentificationError::ContradictoryPhysicalIdentity.to_string(),
            "one monitor instance reported contradictory physical identities"
        );
        assert_eq!(
            MonitorIdentificationError::TokenExhausted.to_string(),
            "process-local monitor identity token space is exhausted"
        );
        assert_eq!(
            MonitorIdentificationError::StablePhysicalIdentityUnavailable.to_string(),
            "platform cannot expose stable physical monitor identity"
        );
    }
}
