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
    evidence_records: Vec<EvidenceRecord>,
    instances:        HashMap<MonitorInstanceId, InstanceIdentity>,
    next_id:          Option<u64>,
}

impl Default for MonitorIdentityRegistry {
    fn default() -> Self {
        Self {
            configuration:    None,
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
        let generation = match configuration {
            MonitorConfigurationState::Ready(generation) => generation,
            MonitorConfigurationState::Unavailable(error) => {
                return self.cache_read_failure(instance_id, None, error);
            },
        };
        if self
            .instances
            .get(&instance_id)
            .is_some_and(|instance| instance.generation == Some(generation))
        {
            return self.instances[&instance_id].identity;
        }
        if platform.is_wayland() {
            return self.cache_read_failure(
                instance_id,
                Some(generation),
                MonitorIdentificationError::StablePhysicalIdentityUnavailable,
            );
        }

        match load_evidence() {
            Ok(evidence) => self.accept_evidence(instance_id, generation, evidence),
            Err(error) => self.cache_read_failure(instance_id, Some(generation), error),
        }
    }

    pub fn cached_identity(&self, instance_id: MonitorInstanceId) -> Option<MonitorIdentity> {
        self.instances
            .get(&instance_id)
            .map(|instance_identity| instance_identity.identity)
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
            self.evidence_records.push(EvidenceRecord {
                evidence: evidence.clone(),
                status:   EvidenceStatus::Verified {
                    id,
                    active_instance: Some(instance_id),
                },
            });
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
                identity,
                error: None,
                generation: Some(generation),
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
                evidence:   Some(evidence),
                identity:   MonitorIdentity::Unverified,
                error:      Some(error),
                generation: Some(generation),
            },
        );
        MonitorIdentity::Unverified
    }

    fn cache_read_failure(
        &mut self,
        instance_id: MonitorInstanceId,
        generation: Option<MonitorConfigurationGeneration>,
        error: MonitorIdentificationError,
    ) -> MonitorIdentity {
        log_identification_error(instance_id, error);
        if let Some(instance) = self.instances.get_mut(&instance_id) {
            instance.identity = MonitorIdentity::Unverified;
            instance.error = Some(error);
            instance.generation = generation;
        } else {
            self.instances.insert(
                instance_id,
                InstanceIdentity {
                    evidence: None,
                    identity: MonitorIdentity::Unverified,
                    error: Some(error),
                    generation,
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
            self.evidence_records.push(EvidenceRecord {
                evidence: evidence.clone(),
                status:   EvidenceStatus::Ambiguous,
            });
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
        self.evidence_records
            .iter()
            .position(|record| record.evidence == *evidence)
    }

    fn record_mut(&mut self, evidence: &QualifiedEvidence) -> Option<&mut EvidenceRecord> {
        self.evidence_records
            .iter_mut()
            .find(|record| record.evidence == *evidence)
    }
}

#[derive(Clone, Debug)]
struct InstanceIdentity {
    evidence:   Option<QualifiedEvidence>,
    identity:   MonitorIdentity,
    error:      Option<MonitorIdentificationError>,
    generation: Option<MonitorConfigurationGeneration>,
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
                Ok(evidence(b"panel-a"))
            },
            Platform::X11,
        );
        let cached = registry.identity(
            INSTANCE_1,
            state(0),
            || {
                loader_calls.set(loader_calls.get() + 1);
                Ok(evidence(b"panel-a"))
            },
            Platform::X11,
        );

        assert_eq!(first, MonitorIdentity::Verified(MonitorId::from_raw(0)));
        assert_eq!(cached, first);
        assert_eq!(loader_calls.get(), 1);
    }

    #[test]
    fn unchanged_evidence_in_new_generation_retains_token() {
        let mut registry = MonitorIdentityRegistry::default();
        let first = registry.identity(
            INSTANCE_1,
            state(0),
            || Ok(evidence(b"panel-a")),
            Platform::X11,
        );
        let revalidated = registry.identity(
            INSTANCE_1,
            state(1),
            || Ok(evidence(b"panel-a")),
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
            || Ok(evidence(b"panel-a")),
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
            || Ok(evidence(b"panel-a")),
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
        let first = registry.accept_evidence(INSTANCE_1, 0_u64.into(), evidence(b"panel-a"));
        registry.disconnect(INSTANCE_1);
        let reconnected = registry.accept_evidence(INSTANCE_2, 0_u64.into(), evidence(b"panel-a"));
        let second = registry.accept_evidence(INSTANCE_3, 0_u64.into(), evidence(b"panel-b"));

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
