use std::time::Duration;

use bevy::ecs::reflect::ReflectComponent;
use bevy::prelude::Component;
use bevy::prelude::Reflect;

use crate::AttachmentPath;
use crate::Capabilities;
use crate::Claim;
use crate::DeviceDescriptor;
use crate::DeviceKey;
use crate::OsDeviceId;
use crate::ReportedSerial;

/// Provider report of whether a unit can be reached in its most recently completed device set.
///
/// `Presence` is an entity component because providers update it as hardware appears, departs, or
/// becomes unreachable. `Unreachable` is not `Absent`: treating a silent remote node as removed
/// can retire output still attached to a live device.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Component, Reflect)]
#[reflect(Component)]
pub enum Presence {
    /// The provider observed the unit and can use it, such as a connected display or camera.
    Present,
    /// The provider established that the unit is gone from its whole current device set.
    Absent,
    /// The provider cannot determine whether the unit remains available, such as when a remote
    /// node stopped responding or its transport disconnected.
    Unreachable {
        /// Time elapsed since the provider first observed this unit as unreachable.
        since: Duration,
    },
}

/// Name status a provider assigns to the unit represented by one `DeviceRecord`.
///
/// The two variants replace `Option<DeviceKey>` because a provider that can name a unit durably
/// participates in key reconciliation, while a provider with only operating-system match evidence
/// must not fabricate a durable identity.
#[derive(Clone, PartialEq, Eq, Debug, Reflect)]
pub enum ReportedAs {
    /// The provider minted a durable key from evidence that can identify this unit across runs.
    Keyed(DeviceKey),
    /// The provider recognizes the unit but has no durable name, as with a display API report
    /// that can join another provider through `DeviceRecord::os_id` only.
    ///
    /// Reconciliation keeps this evidence only when it joins a keyed record. A report with no
    /// keyed match creates no device entity and cannot expose a fabricated `DeviceKey`.
    MatchEvidenceOnly,
}

/// One unit in a provider's completed whole-set report.
///
/// `DeviceRecord` carries observed evidence but no `crate::IdentityVerdict` or `crate::DeviceId`.
/// Reconciliation creates those conclusions after comparing this report with saved keys; allowing
/// a provider to supply either would let it assert identity for a unit that supplied no evidence.
pub struct DeviceRecord {
    /// Durable naming status for this report, including the evidence-only case with no device key.
    pub reported_as:  ReportedAs,
    /// Parent link that places authored devices below their interface and child devices below
    /// their host. A root has exactly one absence state, so `None` does not erase a policy
    /// distinction.
    pub transport:    Option<DeviceKey>,
    /// Provider observation of whether this unit is present, absent, or unreachable.
    pub presence:     Presence,
    /// Provider observation of exclusive ownership, independent from whether the unit is present.
    pub claim:        Claim,
    /// Component values that describe what this unit can do from this provider's perspective.
    pub capabilities: Capabilities,
    /// Serial evidence supplied by the unit or the reason no serial value was available.
    pub serial:       ReportedSerial,
    /// Process-local operating-system handle that can join reports without becoming persisted
    /// identity.
    pub os_id:        OsDeviceId,
    /// Observed attachment location that reconciliation compares when a saved unit was displaced.
    pub attachment:   AttachmentPath,
    /// Vendor, product, and model evidence used for synthesized identity and diagnostics.
    pub descriptor:   DeviceDescriptor,
}

/// Provider result for one opportunity to enumerate its devices.
///
/// Providers return `DeviceScan::Unchanged` on frames where no scan ran; reconciliation retains
/// the last complete set. A completed scan always contains the provider's whole current set, so a
/// missing record is meaningful evidence of departure.
pub enum DeviceScan {
    /// The provider did not scan after its prior report, as when a camera enumeration interval has
    /// not elapsed or display configuration has not changed.
    Unchanged,
    /// The provider scanned and supplied every currently visible device record.
    Complete(DeviceSet),
}

/// Whole current device set reported by one provider after it completes a scan.
pub struct DeviceSet {
    /// Process-local provider handle that identifies which registered provider produced this set.
    pub provider: ProviderId,
    /// Every currently visible device from the provider, with absent devices omitted. Parent links
    /// form a forest that reconciliation ingests from roots toward children.
    pub devices:  Vec<DeviceRecord>,
    /// Per-provider monotonic count that reconciliation folds into a global rigging revision.
    pub revision: ProviderRevision,
}

/// Opaque process-local handle that the provider registry issues in registration order.
///
/// `ProviderId` has no public constructor because providers receive it from the provider registry;
/// permitting a provider to mint one would let it overwrite another provider's whole device set.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect)]
pub struct ProviderId(pub(crate) u32);

/// Monotonic counter attached to each completed scan from one provider.
///
/// A `ProviderRevision` is per provider, not a topology counter: camera, HID, and display
/// providers advance independently before reconciliation combines their latest complete reports.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Reflect)]
pub struct ProviderRevision(u64);

impl ProviderRevision {
    /// Wrap a provider's current completed-scan counter without assigning it global meaning.
    #[must_use]
    pub const fn new(value: u64) -> Self { Self(value) }

    /// Return the provider-owned count for diagnostics and provider-side monotonic updates.
    #[must_use]
    pub const fn get(self) -> u64 { self.0 }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::DeviceRecord;
    use super::Presence;
    use super::ReportedAs;
    use crate::AttachmentPath;
    use crate::Capabilities;
    use crate::Claim;
    use crate::DeviceDescriptor;
    use crate::OsDeviceId;
    use crate::ReportedSerial;

    #[test]
    fn unreachable_presence_retains_when_the_provider_lost_contact() {
        let since = Duration::from_secs(6);
        let presence = Presence::Unreachable { since };

        assert_eq!(presence, Presence::Unreachable { since });
    }

    #[test]
    fn evidence_only_record_has_no_reported_device_key() {
        let device_record = DeviceRecord {
            reported_as:  ReportedAs::MatchEvidenceOnly,
            transport:    None,
            presence:     Presence::Present,
            claim:        Claim::NotApplicable,
            capabilities: Capabilities::new(),
            serial:       ReportedSerial::NotExposedByUnit,
            os_id:        OsDeviceId::PlatformReportedNothing,
            attachment:   AttachmentPath::PlatformHasNoConcept,
            descriptor:   DeviceDescriptor::PlatformReportedNothing,
        };

        assert!(matches!(
            device_record.reported_as,
            ReportedAs::MatchEvidenceOnly
        ));
        assert!(device_record.transport.is_none());
    }
}
