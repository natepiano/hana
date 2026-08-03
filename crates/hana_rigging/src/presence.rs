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

/// Reporter observation of whether a unit can be reached in its most recently completed device
/// set.
///
/// `Presence` is an entity component because reporters update it as hardware appears, departs, or
/// becomes unreachable. `Unreachable` is not `Absent`: treating a silent remote node as removed
/// can retire output still attached to a live device.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Component, Reflect)]
#[reflect(Component, PartialEq)]
pub enum Presence {
    /// The reporter observed the unit and can use it, such as a connected display or camera.
    Present,
    /// The reporter established that the unit is gone from its whole current device set.
    Absent,
    /// The reporter cannot determine whether the unit remains available, such as when a remote
    /// node stopped responding or its transport disconnected.
    Unreachable {
        /// Time elapsed since the reporter first observed this unit as unreachable.
        since: Duration,
    },
}

impl Presence {
    /// Report whether two observations are the same kind of reachability, ignoring how long an
    /// `Self::Unreachable` one has lasted.
    ///
    /// Departure detection and the device-entity mirror both need this instead of `==`: the
    /// elapsed time inside `Self::Unreachable` grows on every scan, so value equality would report
    /// a change on every frame and make a once-per-change consumer fire forever. One function
    /// serves both callers so the diff and the mirror can never disagree about what counts as a
    /// presence change.
    pub(crate) const fn is_same_variant(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Present, Self::Present)
                | (Self::Absent, Self::Absent)
                | (Self::Unreachable { .. }, Self::Unreachable { .. })
        )
    }
}

/// Name status a reporter assigns to the unit represented by one `DeviceRecord`.
///
/// The two variants replace `Option<DeviceKey>` because a reporter that can name a unit durably
/// participates in key reconciliation, while a reporter with only operating-system match evidence
/// must not fabricate a durable identity.
#[derive(Clone, PartialEq, Eq, Debug, Reflect)]
pub enum ReportedAs {
    /// The reporter minted a durable key from evidence that can identify this unit across runs.
    Keyed(DeviceKey),
    /// The reporter recognizes the unit but has no durable name, as with a display API report
    /// that can join another reporter through `DeviceRecord::os_id` only.
    ///
    /// Reconciliation keeps this evidence only when it joins a keyed record. A report with no
    /// keyed match creates no device entity and cannot expose a fabricated `DeviceKey`.
    MatchEvidenceOnly,
}

/// How a reporter reaches one unit: directly, or through another device it already names.
///
/// The variants replace an optional parent key because a root and a child are different reports,
/// not a value and its absence. The field is called a parent rather than a transport: a transport
/// would promise a bus or protocol, which a kernel that performs no input or output must never
/// model, while the value has always been the `DeviceKey` of another reported device.
#[derive(Clone, PartialEq, Eq, Debug, Reflect)]
pub enum ReportedParent {
    /// The reporter reaches this unit directly, as with a display panel enumerated by the window
    /// system. A root sits at the top of one reporter's forest.
    Root,
    /// The reporter reaches this unit through the device named by this key, as a camera reached
    /// through the capture card it is plugged into.
    ///
    /// Presence is conjunctive down the chain: reconciliation never reports a child as more
    /// reachable than the device it hangs off, because a capture card that stopped responding
    /// cannot deliver frames from a camera behind it.
    ChildOf(DeviceKey),
}

/// One unit in a reporter's completed whole-set report.
///
/// `DeviceRecord` carries observed evidence but no `crate::IdentityVerdict` or `crate::DeviceId`.
/// Reconciliation creates those conclusions after comparing this report with saved keys; allowing
/// a reporter to supply either would let it assert identity for a unit that supplied no evidence.
pub struct DeviceRecord {
    /// Durable naming status for this report, including the evidence-only case with no device key.
    pub reported_as:  ReportedAs,
    /// Parent link that places a child device below the device it is reached through, and names
    /// every directly reached unit a root.
    pub parent:       ReportedParent,
    /// Reporter observation of whether this unit is present, absent, or unreachable.
    pub presence:     Presence,
    /// Reporter observation of exclusive ownership, independent from whether the unit is present.
    pub claim:        Claim,
    /// Component values that describe what this unit can do from this reporter's perspective.
    pub capabilities: Capabilities,
    /// Serial evidence supplied by the unit or the reason no serial value was available to the
    /// reporter.
    pub serial:       ReportedSerial,
    /// Process-local operating-system handle that can join reports without becoming persisted
    /// identity.
    pub os_id:        OsDeviceId,
    /// Observed attachment location that reconciliation compares when a saved unit was displaced.
    pub attachment:   AttachmentPath,
    /// Vendor, product, and model evidence the reporter uses for synthesized identity and
    /// diagnostics.
    pub descriptor:   DeviceDescriptor,
}

/// Reporter result for one scheduled attempt to enumerate its devices.
///
/// Not calling a reporter is the unchanged case: cadence and activation state record why it was
/// not due. A completed scan always contains the reporter's whole current set, so a missing record
/// is meaningful evidence of departure.
pub enum DeviceScan {
    /// The reporter scanned and supplied every currently visible device record.
    ///
    /// The reporter registry stamps this list with the registry-issued `ReporterId` and its next
    /// `ReporterRevision`; reporter implementations cannot supply either value themselves.
    Complete(Vec<DeviceRecord>),
    /// Enumeration failed before the reporter could establish its whole current set.
    ///
    /// The registry retains the preceding complete set and revision, because treating an I/O
    /// failure like an empty device list would falsely report every connected camera or display as
    /// departed.
    Failed(crate::DeviceAccessError),
}

/// Whole current device set the reporter registry prepared after one completed scan.
pub struct DeviceSet {
    /// Process-local reporter handle the reporter registry entry assigned to this set's reporter.
    pub reporter: ReporterId,
    /// Every currently visible device from the reporter, with absent devices omitted. Parent links
    /// form a forest that reconciliation ingests from roots toward children.
    pub devices:  Vec<DeviceRecord>,
    /// Per-reporter monotonic count the reporter registry entry advances for reconciliation.
    pub revision: ReporterRevision,
}

/// Opaque process-local handle that the reporter registry issues in registration order.
///
/// `ReporterId` has no public constructor because reporters receive it from the reporter registry;
/// permitting a reporter to mint one would let it overwrite another reporter's whole device set.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect)]
#[reflect(opaque)]
pub struct ReporterId(pub(crate) u32);

/// Monotonic counter attached to each completed scan from one reporter.
///
/// A `ReporterRevision` is per reporter, not a topology counter: camera, HID, and display
/// reporters advance independently before reconciliation combines their latest complete reports.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Reflect)]
#[reflect(opaque)]
pub struct ReporterRevision(u64);

impl ReporterRevision {
    /// Wrap a reporter registry entry's current completed-scan count without assigning it global
    /// meaning.
    #[must_use]
    pub(crate) const fn new(value: u64) -> Self { Self(value) }

    /// Return the reporter registry entry's count for diagnostics.
    #[must_use]
    pub const fn get(self) -> u64 { self.0 }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::reflect::FromReflect;
    use bevy::reflect::tuple_struct::DynamicTupleStruct;

    use super::DeviceRecord;
    use super::Presence;
    use super::ReportedAs;
    use super::ReportedParent;
    use super::ReporterId;
    use super::ReporterRevision;
    use crate::AttachmentPath;
    use crate::Capabilities;
    use crate::Claim;
    use crate::DeviceDescriptor;
    use crate::OsDeviceId;
    use crate::ReportedSerial;

    #[test]
    fn unreachable_presence_retains_when_the_reporter_lost_contact() {
        let since = Duration::from_secs(6);
        let presence = Presence::Unreachable { since };

        assert_eq!(presence, Presence::Unreachable { since });
    }

    #[test]
    fn evidence_only_record_has_no_reported_device_key() {
        let device_record = DeviceRecord {
            reported_as:  ReportedAs::MatchEvidenceOnly,
            parent:       ReportedParent::Root,
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
        assert_eq!(device_record.parent, ReportedParent::Root);
    }

    #[test]
    fn reflection_cannot_construct_reporter_id() {
        let mut dynamic_reporter_id = DynamicTupleStruct::default();
        dynamic_reporter_id.insert(0_u32);

        assert!(ReporterId::from_reflect(&dynamic_reporter_id).is_none());
    }

    #[test]
    fn reflection_cannot_construct_reporter_revision() {
        let mut dynamic_reporter_revision = DynamicTupleStruct::default();
        dynamic_reporter_revision.insert(0_u64);

        assert!(ReporterRevision::from_reflect(&dynamic_reporter_revision).is_none());
    }
}
