use bevy::prelude::*;

use super::registration::WindowRecovery;
use crate::WindowKey;
use crate::constants::RECOVERY_ACCEPTANCE_PRODUCER;
use crate::constants::RECOVERY_PROBE_TARGET;
use crate::monitors::MonitorInfo;

pub(super) struct RecoveryAcceptanceProbeRecord<'a> {
    pub(super) frame_count:    u32,
    pub(super) window_key:     &'a WindowKey,
    pub(super) entity:         Entity,
    pub(super) monitor_entity: Entity,
    pub(super) monitor:        MonitorInfo,
    pub(super) policy:         WindowRecovery,
}

impl RecoveryAcceptanceProbeRecord<'_> {
    pub(super) fn emit(&self) {
        tracing::info!(
            target: RECOVERY_PROBE_TARGET,
            frame_count = u64::from(self.frame_count),
            producer_schedule = RECOVERY_ACCEPTANCE_PRODUCER,
            window_key = ?self.window_key,
            window = ?self.entity,
            monitor_entity = ?self.monitor_entity,
            monitor = ?self.monitor,
            recovery_policy = ?self.policy,
            "accepted recovery registration"
        );
    }
}
