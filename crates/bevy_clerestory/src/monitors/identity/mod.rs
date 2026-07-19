mod configuration;
mod edid;
mod native;
mod registry;

use bevy::prelude::*;
pub(super) use configuration::MonitorConfiguration;
pub(super) use configuration::MonitorConfigurationState;
pub(super) use native::qualified_evidence;
pub(super) use registry::MonitorIdentificationError;
pub(super) use registry::MonitorIdentityRegistry;
pub(super) use registry::MonitorInstanceId;
pub(super) use registry::OperatingSystemQueryError;

/// Opaque process-local token for one complete, verified physical-panel identity.
///
/// A `MonitorId` is valid only for the lifetime of the current `App`. It is not
/// derived from an evidence hash and must not be persisted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
#[type_path = "bevy_clerestory::monitors"]
pub struct MonitorId(u64);

impl MonitorId {
    pub(super) const fn from_raw(raw: u64) -> Self { Self(raw) }
}

/// Public physical-panel identity state for a monitor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
#[type_path = "bevy_clerestory::monitors"]
pub enum MonitorIdentity {
    /// Complete panel evidence has one process-lifetime [`MonitorId`].
    Verified(MonitorId),
    /// Panel evidence is unavailable, insufficient, contradictory, or ambiguous.
    Unverified,
}
