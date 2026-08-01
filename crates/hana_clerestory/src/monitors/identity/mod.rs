mod configuration;
mod edid;
mod native;
mod registry;

use bevy::prelude::*;
pub(super) use configuration::MonitorConfiguration;
pub(super) use configuration::MonitorConfigurationState;
#[cfg(test)]
pub(super) use native::QualifiedEvidence;
pub(super) use native::qualified_evidence;
pub(super) use registry::MonitorIdentificationError;
#[cfg(feature = "monitor-probe")]
pub(super) use registry::MonitorIdentityProbe;
pub(crate) use registry::MonitorIdentityRegistry;
pub(super) use registry::MonitorInstanceId;
pub(super) use registry::OperatingSystemQueryError;
use serde::Deserialize;
use serde::Serialize;

use crate::constants::FNV_1A_OFFSET_BASIS;
use crate::constants::FNV_1A_PRIME;

/// Stable fingerprint of one physical panel, derived only from the panel's own identity data:
/// EDID bytes on Windows and X11, the `ColorSync` display UUID on macOS.
///
/// Unlike [`MonitorId`], which is handed out per process and means nothing outside it, the same
/// panel produces the same fingerprint on every launch. That is what makes it safe to write into
/// a state file: a monitor-relative saved position is only meaningful if the monitor it was
/// measured from can still be recognised after displays are replugged, docked, or renumbered by a
/// driver update.
///
/// Two identical panels that report no serial number are indistinguishable to their operating
/// system and therefore share a fingerprint. That is a property of the evidence, not of this
/// type, and it is why a fingerprint match is treated as a strong hint rather than proof — the
/// saved position is still range-checked against the monitor it resolves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct PanelFingerprint(u64);

impl PanelFingerprint {
    /// FNV-1a over the panel's evidence bytes.
    ///
    /// Deliberately not `DefaultHasher`: its output is explicitly not guaranteed stable across
    /// Rust releases, so a toolchain bump would silently stop every saved fingerprint from
    /// matching and quietly turn identity-based restore back into index-based restore. FNV-1a is
    /// fixed by its specification, short enough to read, and needs no dependency.
    #[must_use]
    pub(super) fn from_evidence_bytes(bytes: &[u8]) -> Self {
        let mut hash = FNV_1A_OFFSET_BASIS;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_1A_PRIME);
        }
        Self(hash)
    }
}

/// Whether a monitor's physical panel can be recognised again in a later run.
///
/// Deliberately not `Option<PanelFingerprint>`. The two states are not "a value" and "no value";
/// they are "this panel identifies itself" and "this panel cannot be told apart from any other",
/// and those compare differently: two anonymous panels are **not** the same panel, whereas
/// `Option`'s derived equality says `None == None`. Writing the rule into the type means a
/// comparison cannot accidentally treat two unidentifiable displays as one and anchor a saved
/// position to whichever happened to enumerate first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum PanelIdentity {
    /// The panel reported evidence unique to it. Stable across runs, reboots and replugs.
    Fingerprinted(PanelFingerprint),
    /// No usable panel evidence. Wayland withholds it, a virtual display may synthesize none,
    /// and two identical panels reporting no serial number are indistinguishable to the
    /// operating system. A position saved against such a monitor falls back to the saved index.
    #[default]
    Anonymous,
}

impl PanelIdentity {
    /// Whether two identities name the same physical panel.
    ///
    /// [`Self::Anonymous`] never matches anything, including itself: failing to identify two
    /// panels is not evidence that they are the same one.
    #[must_use]
    pub(crate) fn is_same_panel(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Fingerprinted(left), Self::Fingerprinted(right)) if left == right
        )
    }
}

/// Opaque process-local token for one complete, verified physical-panel identity.
///
/// A `MonitorId` is valid only for the lifetime of the current `App`. It is not
/// derived from an evidence hash and must not be persisted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
#[type_path = "hana_clerestory::monitors"]
pub struct MonitorId(u64);

impl MonitorId {
    pub(super) const fn from_raw(raw: u64) -> Self { Self(raw) }

    #[cfg(test)]
    pub(crate) const fn from_test_raw(raw: u64) -> Self { Self(raw) }
}

/// Public physical-panel identity state for a monitor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
#[type_path = "hana_clerestory::monitors"]
pub enum MonitorIdentity {
    /// Complete panel evidence has one process-lifetime [`MonitorId`].
    Verified(MonitorId),
    /// Panel evidence is unavailable, insufficient, contradictory, or ambiguous.
    Unverified,
}

pub(super) fn cached_identity(
    registry: &MonitorIdentityRegistry,
    instance_id: MonitorInstanceId,
) -> Option<MonitorIdentity> {
    registry.cached_identity(instance_id)
}

pub(crate) fn panel_identity(
    registry: &MonitorIdentityRegistry,
    identity: MonitorIdentity,
) -> PanelIdentity {
    match identity {
        MonitorIdentity::Verified(monitor_id) => registry.panel_identity(monitor_id),
        MonitorIdentity::Unverified => PanelIdentity::Anonymous,
    }
}

pub(super) fn monitor_handle_missing(
    registry: &mut MonitorIdentityRegistry,
    instance_id: MonitorInstanceId,
    configuration: MonitorConfigurationState,
) {
    registry.monitor_handle_missing(instance_id, configuration);
}
