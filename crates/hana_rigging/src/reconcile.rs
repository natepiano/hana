use std::any::TypeId;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::time::Duration;
use std::time::Instant;

use bevy::prelude::Res;
use bevy::prelude::ResMut;
use bevy::reflect::PartialReflect;
use bevy::time::Real;
use bevy::time::Time;

use crate::Claim;
use crate::DeviceKey;
use crate::DeviceRecord;
use crate::Devices;
use crate::DiscoveryCadence;
use crate::OsDeviceId;
use crate::Presence;
use crate::ReconciledDeviceState;
use crate::RegisteredSchemes;
use crate::ReportedAs;
use crate::ReportedId;
use crate::ReportedParent;
use crate::ReporterId;
use crate::RiggingLimits;
use crate::RiggingRevision;
use crate::registration::RegisteredReporter;
use crate::registration::ReporterContribution;
use crate::registration::Reporters;

/// Merge every contributing reporter's latest whole set into one device set, once per tick.
///
/// The system itself only reads the frame's real-time clock and hands the resources to
/// `reconcile_devices`, which is where the settled-frame return lives.
pub(crate) fn reconcile(
    mut reporters: ResMut<Reporters>,
    mut devices: ResMut<Devices>,
    mut rigging_revision: ResMut<RiggingRevision>,
    rigging_limits: Res<RiggingLimits>,
    registered_schemes: Res<RegisteredSchemes>,
    time: Res<Time<Real>>,
) {
    reconcile_devices(
        &mut reporters,
        &mut devices,
        &mut rigging_revision,
        &rigging_limits,
        &registered_schemes,
        FrameClockReading::from(&*time),
    );
}

/// The real-time reading the freshness lease measures reporter silence against.
///
/// Real time rather than the game clock, because hardware does not pause when the application
/// does: a paused app must not conclude an hour later that a monitor is still fresh.
#[derive(Clone, Copy, Debug)]
enum FrameClockReading {
    /// The real-time clock has advanced at least once, so silence can be measured against it.
    Measurable(Instant),
    /// The real-time clock has not advanced past application startup, so no elapsed time exists to
    /// judge and no reporter is stale this frame.
    NotYetAdvanced,
}

impl From<&Time<Real>> for FrameClockReading {
    fn from(time: &Time<Real>) -> Self {
        time.last_update()
            .map_or(Self::NotYetAdvanced, Self::Measurable)
    }
}

/// Whether the freshness lease has anything left to apply to the devices already retained.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FreshnessLeaseWork {
    /// Every retained device already reflects how fresh its reporters are, so the lease alone is
    /// no reason to merge again.
    Settled,
    /// At least one still-reachable device belongs to a reporter that went silent past its lease.
    MarksDevicesUnreachable,
}

/// How long each reporter may stay silent this frame, so freshness is answered one reporter at a
/// time instead of by collecting the silent ones.
///
/// Answering per reporter rather than building a list is what keeps a settled frame free of
/// allocation while a reporter stays wedged: a permanently silent reporter is judged again every
/// frame, and judging it costs nothing.
#[derive(Clone, Copy)]
struct FreshnessLease<'a> {
    rigging_limits: &'a RiggingLimits,
    clock:          FrameClockReading,
}

impl FreshnessLease<'_> {
    /// Report how much one reporter's records still count as evidence that its devices are there.
    ///
    /// A reporter that has not completed a first scan is not silent — it has never promised
    /// anything yet — and a reporter that declared no cadence may stay quiet indefinitely without
    /// being late.
    fn freshness_of(&self, registered_reporter: &RegisteredReporter<'_>) -> ReporterFreshness {
        let FrameClockReading::Measurable(now) = self.clock else {
            return ReporterFreshness::Fresh;
        };
        let ReporterContribution::Completed { completed_at, .. } =
            &registered_reporter.contribution
        else {
            return ReporterFreshness::Fresh;
        };
        let ReporterFreshnessLease::Expires(lease) =
            freshness_lease(registered_reporter.cadence, self.rigging_limits)
        else {
            return ReporterFreshness::Fresh;
        };

        let silence = now.saturating_duration_since(*completed_at);
        if silence > lease {
            ReporterFreshness::SilentFor(silence)
        } else {
            ReporterFreshness::Fresh
        }
    }

    /// Report the freshness of the reporter a retained device names as a contributor.
    ///
    /// A contributor the registry no longer lists cannot be judged silent: the reporter is gone,
    /// not late, and the next merge drops it from the device anyway.
    fn freshness_by_id(&self, reporters: &Reporters, reporter: ReporterId) -> ReporterFreshness {
        reporters
            .registered_reporters()
            .find(|registered_reporter| registered_reporter.reporter == reporter)
            .map_or(ReporterFreshness::Fresh, |registered_reporter| {
                self.freshness_of(&registered_reporter)
            })
    }
}

/// How much a reporter's silence is worth trusting when its records reach the merge.
#[derive(Clone, Copy)]
enum ReporterFreshness {
    /// The reporter is inside its lease, so its records read as it reported them.
    Fresh,
    /// The reporter has been quiet this long past its lease, so the kernel stops treating its
    /// records as evidence that the devices are still reachable.
    SilentFor(Duration),
}

/// How silent a reporter may be before the kernel stops trusting its devices.
enum ReporterFreshnessLease {
    /// The reporter declared no cadence, so silence proves nothing: it runs when the application
    /// asks and can stay quiet indefinitely without being late.
    NoDeclaredCadence,
    /// The reporter promised a run within this interval, so exceeding it plus the configured grace
    /// means the reporter is wedged rather than idle.
    Expires(Duration),
}

/// How reachable one presence observation is, so a child can be folded against its parent.
///
/// The order is the whole conjunctive rule: reconciliation keeps whichever of the two is less
/// reachable, because a device is never more reachable than the device it hangs off.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Reachability {
    /// The unit is established as gone.
    Departed,
    /// Whether the unit remains available cannot be determined.
    Uncertain,
    /// The unit was observed and can be used.
    Reachable,
}

impl Reachability {
    const fn from_presence(presence: Presence) -> Self {
        match presence {
            Presence::Present => Self::Reachable,
            Presence::Unreachable { .. } => Self::Uncertain,
            Presence::Absent => Self::Departed,
        }
    }
}

/// What one device's contributors say about its reachability, kept apart by whether the reporter
/// that said so is still inside its freshness lease.
///
/// A reporter past its lease has withdrawn its evidence, not reported an absence, so its records
/// never lower a device that a fresh contributor still reports. Silence becomes the device's own
/// presence only once every contributor to it has gone quiet.
#[derive(Clone, Copy)]
enum CoReportedPresence {
    /// Every contributor merged so far is past its lease: the least reachable presence they last
    /// reported, and how long the most recent of them has been silent.
    WithdrawnBySilence {
        last_reported: Presence,
        silence:       Duration,
    },
    /// At least one contributor is inside its lease: the least reachable presence those fresh
    /// contributors report.
    StillReported(Presence),
}

impl CoReportedPresence {
    const fn from_contribution(freshness: ReporterFreshness, reported: Presence) -> Self {
        match freshness {
            ReporterFreshness::Fresh => Self::StillReported(reported),
            ReporterFreshness::SilentFor(silence) => Self::WithdrawnBySilence {
                last_reported: reported,
                silence,
            },
        }
    }

    /// Add one contributor's report to what the device's contributors say so far.
    ///
    /// Merging is idempotent, so the first record may seed the merged device and be merged again
    /// without counting twice.
    fn merge(self, contribution: Self) -> Self {
        match (self, contribution) {
            (Self::StillReported(first), Self::StillReported(second)) => {
                Self::StillReported(least_reachable(first, second))
            },
            (Self::StillReported(reported), Self::WithdrawnBySilence { .. })
            | (Self::WithdrawnBySilence { .. }, Self::StillReported(reported)) => {
                Self::StillReported(reported)
            },
            (
                Self::WithdrawnBySilence {
                    last_reported: first,
                    silence: first_silence,
                },
                Self::WithdrawnBySilence {
                    last_reported: second,
                    silence: second_silence,
                },
            ) => Self::WithdrawnBySilence {
                last_reported: least_reachable(first, second),
                silence:       first_silence.min(second_silence),
            },
        }
    }

    /// Report the presence the device carries once every contributor has been merged.
    const fn settled(self) -> Presence {
        match self {
            Self::StillReported(presence) => presence,
            Self::WithdrawnBySilence {
                last_reported,
                silence,
            } => least_reachable(last_reported, Presence::Unreachable { since: silence }),
        }
    }
}

/// Every record reported under one durable key, before the merge draws any conclusion from them.
struct MergedDevice<'a> {
    parent:       ReportedParent,
    presence:     CoReportedPresence,
    claim:        Claim,
    contributors: Vec<ReporterId>,
    capabilities: CoReportView<'a>,
}

/// Capability declarations borrowed from every contributing reporter, grouped by component type.
///
/// A borrowed view rather than a built collection: `Box<dyn Reflect>` is not clonable in Bevy
/// 0.19, and consuming a declaration out of a retained set would destroy evidence a reporter that
/// did not re-scan this frame still needs. It points at data the reporter registry already holds,
/// so each site that needs one builds its own instead of storing it in a resource.
type CoReportView<'a> = HashMap<TypeId, Vec<&'a dyn PartialReflect>>;

fn reconcile_devices(
    reporters: &mut Reporters,
    devices: &mut Devices,
    rigging_revision: &mut RiggingRevision,
    rigging_limits: &RiggingLimits,
    registered_schemes: &RegisteredSchemes,
    clock: FrameClockReading,
) {
    let changed_reporters = reporters.take_changed_reporters();
    // `DiscoveryStatus` holds each failure's error, and a failed scan retains the preceding whole
    // set and revision, so a failure changes no device state here. Draining keeps the queue from
    // growing; the attempt lifecycle aborts what a failure invalidates.
    drop(reporters.take_reporter_failures());

    // The lease feeds the merge instead of editing the merged result, because the merge rebuilds
    // every device from the retained sets: a presence written before it would be overwritten, and
    // a presence written after it would have to be re-derived on every following frame.
    let freshness_lease = FreshnessLease {
        rigging_limits,
        clock,
    };

    if changed_reporters.is_empty()
        && lease_work(devices, reporters, freshness_lease) == FreshnessLeaseWork::Settled
    {
        return;
    }

    ingest(reporters, devices, registered_schemes, freshness_lease);

    if !changed_reporters.is_empty() {
        rigging_revision.advance();
    }
}

/// Report whether an expired lease would still change any retained device.
///
/// This is what keeps a frame with no completed reporter free of work: a reporter that stayed
/// silent for an hour is judged once, and every following frame sees its devices already
/// unreachable and merges nothing.
///
/// A device only has lease work left when *every* contributor to it has gone silent. A silent
/// reporter withdraws its evidence rather than reporting absence, so a device another reporter
/// still reports keeps the presence that reporter gives it.
fn lease_work(
    devices: &Devices,
    reporters: &Reporters,
    freshness_lease: FreshnessLease<'_>,
) -> FreshnessLeaseWork {
    let every_reporter_fresh = reporters.registered_reporters().all(|registered_reporter| {
        matches!(
            freshness_lease.freshness_of(&registered_reporter),
            ReporterFreshness::Fresh
        )
    });
    if every_reporter_fresh {
        return FreshnessLeaseWork::Settled;
    }

    let marks_devices_unreachable = devices.states().any(|reconciled_device_state| {
        Reachability::from_presence(reconciled_device_state.presence) == Reachability::Reachable
            && !reconciled_device_state.contributors.is_empty()
            && reconciled_device_state
                .contributors
                .iter()
                .all(|contributor| {
                    matches!(
                        freshness_lease.freshness_by_id(reporters, *contributor),
                        ReporterFreshness::SilentFor(_)
                    )
                })
    });

    if marks_devices_unreachable {
        FreshnessLeaseWork::MarksDevicesUnreachable
    } else {
        FreshnessLeaseWork::Settled
    }
}

/// Keep whichever of two observations claims less reachability.
const fn least_reachable(first: Presence, second: Presence) -> Presence {
    if (Reachability::from_presence(second) as u8) < (Reachability::from_presence(first) as u8) {
        second
    } else {
        first
    }
}

fn freshness_lease(
    cadence: &DiscoveryCadence,
    rigging_limits: &RiggingLimits,
) -> ReporterFreshnessLease {
    match cadence {
        DiscoveryCadence::OnDemand => ReporterFreshnessLease::NoDeclaredCadence,
        DiscoveryCadence::EventDriven { backstop } => {
            ReporterFreshnessLease::Expires(*backstop + rigging_limits.report_grace)
        },
        DiscoveryCadence::Periodic { interval } => {
            ReporterFreshnessLease::Expires(*interval + rigging_limits.report_grace)
        },
    }
}

/// Which keyed device one reported platform handle names.
///
/// A handle two reporters attached to different keys names no device: joining an evidence-only
/// record to whichever key happened to be ingested last is exactly the plausible fallback that
/// exact-match identity exists to forbid.
enum HandleOwner {
    /// Every keyed record carrying this handle reported the same key.
    OneKey(DeviceKey),
    /// Keyed records disagree about which key this handle belongs to.
    SeveralKeys,
}

/// One record that carried no key, held with what it would contribute to the keyed device its
/// platform handle names.
struct EvidenceOnlyReport<'a> {
    reporter:      ReporterId,
    os_id:         &'a ReportedId,
    device_record: &'a DeviceRecord,
    presence:      CoReportedPresence,
}

/// Merge every retained whole set into one device set and hand it to the registry.
fn ingest(
    reporters: &Reporters,
    devices: &mut Devices,
    registered_schemes: &RegisteredSchemes,
    freshness_lease: FreshnessLease<'_>,
) {
    let mut merged: HashMap<DeviceKey, MergedDevice<'_>> = HashMap::new();
    // First-seen order, so the handles the registry issues to new keys depend on the reporters'
    // own report order rather than on hash iteration order, which varies between runs.
    let mut ingest_order: Vec<DeviceKey> = Vec::new();
    let mut keyed_by_os_id: HashMap<&ReportedId, HandleOwner> = HashMap::new();
    let mut evidence_only: Vec<EvidenceOnlyReport<'_>> = Vec::new();
    let mut duplicate_keys = HashSet::new();
    let mut unregistered_schemes = HashSet::new();

    // One pass over every retained record. Keyed records group through one `HashMap::entry`, which
    // is what makes co-report merging linear in device count rather than a pairwise join across
    // reporter lists; the same pass collects the evidence-only records the join below needs.
    for registered_reporter in reporters.registered_reporters() {
        let freshness = freshness_lease.freshness_of(&registered_reporter);
        let reporter = registered_reporter.reporter;
        let ReporterContribution::Completed { device_set, .. } = registered_reporter.contribution
        else {
            continue;
        };

        // A silent reporter's records still describe the devices it named; what they stop being is
        // evidence that those devices are reachable right now.
        let reported_presence = |device_record: &DeviceRecord| {
            CoReportedPresence::from_contribution(freshness, device_record.presence)
        };

        for device_record in &device_set.devices {
            match &device_record.reported_as {
                ReportedAs::Keyed(key) => {
                    if let Err(unregistered_scheme) = registered_schemes.validate(key) {
                        unregistered_schemes.insert(unregistered_scheme.scheme().clone());
                        continue;
                    }
                    if let OsDeviceId::Reported(os_id) = &device_record.os_id {
                        record_handle_owner(&mut keyed_by_os_id, os_id, key);
                    }
                    merge_keyed_record(
                        merged.entry(key.clone()).or_insert_with(|| {
                            ingest_order.push(key.clone());
                            MergedDevice {
                                parent:       device_record.parent.clone(),
                                presence:     reported_presence(device_record),
                                claim:        device_record.claim.clone(),
                                contributors: Vec::new(),
                                capabilities: HashMap::new(),
                            }
                        }),
                        reporter,
                        device_record,
                        reported_presence(device_record),
                        &mut duplicate_keys,
                        key,
                    );
                },
                // An evidence-only record carries no key, so it can only join a keyed record
                // through a handle the platform actually reported. Two records that each reported
                // no handle compare equal, which is exactly the plausible-fallback join that
                // exact-match identity exists to prevent, so every other variant joins nothing.
                ReportedAs::MatchEvidenceOnly => {
                    if let OsDeviceId::Reported(os_id) = &device_record.os_id {
                        evidence_only.push(EvidenceOnlyReport {
                            reporter,
                            os_id,
                            device_record,
                            presence: reported_presence(device_record),
                        });
                    }
                },
            }
        }
    }

    // A joined record is a co-report of the device its handle names, so it merges through the same
    // path a keyed record does: its presence, claim, and capability declarations all count.
    for evidence_only_report in evidence_only {
        let Some(HandleOwner::OneKey(key)) = keyed_by_os_id.get(evidence_only_report.os_id) else {
            continue;
        };
        let Some(merged_device) = merged.get_mut(key) else {
            continue;
        };
        merge_keyed_record(
            merged_device,
            evidence_only_report.reporter,
            evidence_only_report.device_record,
            evidence_only_report.presence,
            &mut duplicate_keys,
            key,
        );
    }

    devices.replace_reconciled(
        fold_presence_roots_first(&merged, ingest_order),
        duplicate_keys,
        unregistered_schemes,
    );
}

/// Record which key a reported platform handle belongs to, or that reporters disagree about it.
fn record_handle_owner<'a>(
    keyed_by_os_id: &mut HashMap<&'a ReportedId, HandleOwner>,
    os_id: &'a ReportedId,
    key: &DeviceKey,
) {
    match keyed_by_os_id.entry(os_id) {
        Entry::Vacant(vacant) => {
            vacant.insert(HandleOwner::OneKey(key.clone()));
        },
        Entry::Occupied(mut occupied) => {
            if !matches!(occupied.get(), HandleOwner::OneKey(owner) if owner == key) {
                occupied.insert(HandleOwner::SeveralKeys);
            }
        },
    }
}

/// Add one keyed record to the device its key names.
fn merge_keyed_record<'a>(
    merged_device: &mut MergedDevice<'a>,
    reporter: ReporterId,
    device_record: &'a DeviceRecord,
    reported_presence: CoReportedPresence,
    duplicate_keys: &mut HashSet<DeviceKey>,
    key: &DeviceKey,
) {
    if merged_device.contributors.contains(&reporter) {
        duplicate_keys.insert(key.clone());
    } else {
        merged_device.contributors.push(reporter);
    }

    merged_device.presence = merged_device.presence.merge(reported_presence);
    if claim_restriction(&device_record.claim) > claim_restriction(&merged_device.claim) {
        merged_device.claim = device_record.claim.clone();
    }
    if matches!(merged_device.parent, ReportedParent::Root) {
        merged_device.parent = device_record.parent.clone();
    }

    for capability in device_record.capabilities.declarations() {
        merged_device
            .capabilities
            .entry(capability.as_any().type_id())
            .or_default()
            .push(capability.as_partial_reflect());
    }
}

/// How much a reported claim restricts this process, so co-reported claims merge to the most
/// restrictive report.
///
/// One reporter seeing an idle camera does not make it idle when another reporter watched a second
/// application open it; authorizing capture on the optimistic report would fail at the driver.
const fn claim_restriction(claim: &Claim) -> u8 {
    match claim {
        Claim::NotApplicable | Claim::Free => 0,
        Claim::Held => 1,
        Claim::Contended { .. } => 2,
        Claim::Blocked { .. } => 3,
    }
}

/// Order the merged devices roots first and fold each child's presence against its parent.
///
/// Reporters may list a child before the device it hangs off, so the order the records arrived in
/// cannot be trusted. Following the parent links first means a child is never folded against a
/// parent whose own presence has not been settled.
fn fold_presence_roots_first(
    merged: &HashMap<DeviceKey, MergedDevice<'_>>,
    ingest_order: Vec<DeviceKey>,
) -> Vec<ReconciledDeviceState> {
    let mut reconciled: Vec<ReconciledDeviceState> = Vec::with_capacity(merged.len());
    let mut folded: HashMap<DeviceKey, Presence> = HashMap::with_capacity(merged.len());
    let mut pending = ingest_order;

    while !pending.is_empty() {
        let mut deferred = Vec::new();
        let mut settled_any = false;

        for key in pending {
            let merged_device = &merged[&key];
            let parent_presence = match &merged_device.parent {
                ReportedParent::Root => None,
                ReportedParent::ChildOf(parent_key) => {
                    if !merged.contains_key(parent_key) {
                        // The whole set omits the device this one hangs off, so the kernel cannot
                        // see whether it can still be reached through it. Established departure is
                        // the reporter's to declare; the kernel only records uncertainty.
                        Some(Presence::Unreachable {
                            since: Duration::ZERO,
                        })
                    } else if let Some(parent_presence) = folded.get(parent_key) {
                        Some(*parent_presence)
                    } else {
                        deferred.push(key);
                        continue;
                    }
                },
            };

            let merged_presence = merged_device.presence.settled();
            let presence = parent_presence.map_or(merged_presence, |parent_presence| {
                if Reachability::from_presence(parent_presence)
                    < Reachability::from_presence(merged_presence)
                {
                    parent_presence
                } else {
                    merged_presence
                }
            });
            folded.insert(key.clone(), presence);
            reconciled.push(ReconciledDeviceState {
                key: key.clone(),
                parent: merged_device.parent.clone(),
                presence,
                claim: merged_device.claim.clone(),
                contributors: merged_device.contributors.clone(),
                declared: merged_device.capabilities.keys().copied().collect(),
                disputed: disputed_capabilities(&merged_device.capabilities),
            });
            settled_any = true;
        }

        if !settled_any {
            // Every remaining device names a parent inside a cycle no reporter can resolve.
            // Settling them as uncertain keeps a malformed forest from stalling reconciliation.
            for key in deferred {
                let merged_device = &merged[&key];
                reconciled.push(ReconciledDeviceState {
                    key:          key.clone(),
                    parent:       merged_device.parent.clone(),
                    presence:     Presence::Unreachable {
                        since: Duration::ZERO,
                    },
                    claim:        merged_device.claim.clone(),
                    contributors: merged_device.contributors.clone(),
                    declared:     merged_device.capabilities.keys().copied().collect(),
                    disputed:     disputed_capabilities(&merged_device.capabilities),
                });
            }
            break;
        }

        pending = deferred;
    }

    reconciled
}

/// Report which capability component types the contributors disagree about.
///
/// Equality is `PartialReflect::reflect_partial_eq` across the references under one type. A type
/// whose reflected comparison cannot answer counts as disputed: unavailable equality evidence is
/// not agreement.
fn disputed_capabilities(capabilities: &CoReportView<'_>) -> HashSet<TypeId> {
    capabilities
        .iter()
        .filter(|(_, declarations)| {
            declarations.windows(2).any(|pair| {
                pair.first().is_some_and(|first| {
                    pair.get(1)
                        .is_some_and(|second| first.reflect_partial_eq(*second) != Some(true))
                })
            })
        })
        .map(|(type_id, _)| *type_id)
        .collect()
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::alloc::GlobalAlloc;
    use std::alloc::Layout;
    use std::alloc::System;
    use std::cell::Cell;
    use std::time::Duration;
    use std::time::Instant;

    use bevy::app::App;

    use super::FrameClockReading;
    use super::reconcile_devices;
    use crate::AttachmentPath;
    use crate::Capabilities;
    use crate::Claim;
    use crate::DeviceDescriptor;
    use crate::DeviceIdSource;
    use crate::DeviceKey;
    use crate::DeviceKind;
    use crate::DeviceRecord;
    use crate::DeviceReporter;
    use crate::DeviceResolution;
    use crate::DeviceScan;
    use crate::DeviceStateLookup;
    use crate::Devices;
    use crate::DiscoveryCadence;
    use crate::DiscoveryWork;
    use crate::MainThreadDiscoveryJob;
    use crate::OsDeviceId;
    use crate::Presence;
    use crate::RegisteredSchemes;
    use crate::ReportedAs;
    use crate::ReportedId;
    use crate::ReportedParent;
    use crate::ReportedSerial;
    use crate::ReporterCoverage;
    use crate::ReporterId;
    use crate::ReporterRegistration;
    use crate::RiggingAppExt;
    use crate::RiggingLimits;
    use crate::RiggingPlugin;
    use crate::RiggingRevision;
    use crate::SchemeName;
    use crate::registration::Reporters;

    /// Counts the bytes one thread requests so a test can prove the settled reconcile path asks
    /// the allocator for nothing.
    ///
    /// The counter is thread-local and const-initialized, so the allocator itself never allocates
    /// and a test measuring its own thread cannot be disturbed by another test's work.
    struct CountingAllocator;

    thread_local! {
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
        static COUNTING: Cell<bool> = const { Cell::new(false) };
    }

    // SAFETY: every method forwards to the system allocator with the same layout it received, so
    // the allocation contract is the system allocator's. The counter only reads and writes a
    // `Cell<usize>` in thread-local storage and allocates nothing itself.
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record_allocation();
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) };
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record_allocation();
            unsafe { System.realloc(pointer, layout, new_size) }
        }
    }

    fn record_allocation() {
        let _ = COUNTING.try_with(|counting| {
            if counting.get() {
                let _ = ALLOCATIONS.try_with(|allocations| {
                    allocations.set(allocations.get() + 1);
                });
            }
        });
    }

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    /// Run one closure with this thread's allocation counter enabled and report the count.
    fn allocations_during(measured: impl FnOnce()) -> usize {
        ALLOCATIONS.with(|allocations| allocations.set(0));
        COUNTING.with(|counting| counting.set(true));
        measured();
        COUNTING.with(|counting| counting.set(false));
        ALLOCATIONS.with(Cell::get)
    }

    /// A reporter that returns the same authored records on every scan.
    struct FixedReporter(fn() -> Vec<DeviceRecord>);

    impl DeviceReporter for FixedReporter {
        fn discover(&mut self) -> DiscoveryWork {
            let build = self.0;
            DiscoveryWork::Immediate(MainThreadDiscoveryJob::new(move |_| {
                DeviceScan::Complete(build())
            }))
        }
    }

    const TEST_SCHEME: &str = "test-scheme";

    fn scheme() -> SchemeName { SchemeName::new(TEST_SCHEME).expect("test scheme is well formed") }

    fn key(value: &str) -> DeviceKey { keyed_in(TEST_SCHEME, value) }

    fn keyed_in(scheme_name: &str, value: &str) -> DeviceKey {
        DeviceKey {
            kind: DeviceKind::Display,
            id:   DeviceIdSource::Reported {
                scheme: SchemeName::new(scheme_name).expect("test scheme is well formed"),
                value:  ReportedId::new(value).expect("test reported id is well formed"),
            },
        }
    }

    fn record(reported_as: ReportedAs) -> DeviceRecord {
        DeviceRecord {
            reported_as,
            parent: ReportedParent::Root,
            presence: Presence::Present,
            claim: Claim::NotApplicable,
            capabilities: Capabilities::new(),
            serial: ReportedSerial::NotExposedByUnit,
            os_id: OsDeviceId::PlatformReportedNothing,
            attachment: AttachmentPath::PlatformHasNoConcept,
            descriptor: DeviceDescriptor::PlatformReportedNothing,
        }
    }

    fn app_with_scheme() -> App {
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);
        app.register_device_scheme(scheme());
        app
    }

    fn every_frame() -> DiscoveryCadence {
        DiscoveryCadence::Periodic {
            interval: Duration::ZERO,
        }
    }

    fn add_reporter(app: &mut App, build: fn() -> Vec<DeviceRecord>) -> ReporterId {
        add_reporter_with_cadence(app, build, every_frame())
    }

    fn add_reporter_with_cadence(
        app: &mut App,
        build: fn() -> Vec<DeviceRecord>,
        cadence: DiscoveryCadence,
    ) -> ReporterId {
        app.add_device_reporter(
            FixedReporter(build),
            ReporterRegistration::required(cadence, ReporterCoverage::MatchingEvidenceOnly),
        )
    }

    /// Run one reconcile pass over the app's resources without going through the schedule, so a
    /// test controls the frame clock and the reporter registry contents exactly.
    fn reconcile_once(app: &mut App, clock: FrameClockReading) -> usize {
        let mut allocations = 0;
        app.world_mut()
            .resource_scope::<Reporters, _>(|world, mut reporters| {
                world.resource_scope::<Devices, _>(|world, mut devices| {
                    world.resource_scope::<RiggingRevision, _>(|world, mut rigging_revision| {
                        world.resource_scope::<RiggingLimits, _>(|world, rigging_limits| {
                            world.resource_scope::<RegisteredSchemes, _>(
                                |_, registered_schemes| {
                                    allocations = allocations_during(|| {
                                        reconcile_devices(
                                            &mut reporters,
                                            &mut devices,
                                            &mut rigging_revision,
                                            &rigging_limits,
                                            &registered_schemes,
                                            clock,
                                        );
                                    });
                                },
                            );
                        });
                    });
                });
            });
        allocations
    }

    fn resolved(devices: &Devices, device_key: &DeviceKey) -> Option<crate::DeviceId> {
        match devices.resolve(device_key) {
            DeviceResolution::Resolved(device_id) => Some(device_id),
            DeviceResolution::NotResolved => None,
        }
    }

    fn presence_of(devices: &Devices, device_key: &DeviceKey) -> Option<Presence> {
        let device_id = resolved(devices, device_key)?;
        match devices.state(device_id) {
            DeviceStateLookup::Retained(state) => Some(state.presence),
            DeviceStateLookup::Retired => None,
        }
    }

    fn claim_of(devices: &Devices, device_key: &DeviceKey) -> Option<Claim> {
        let device_id = resolved(devices, device_key)?;
        match devices.state(device_id) {
            DeviceStateLookup::Retained(state) => Some(state.claim.clone()),
            DeviceStateLookup::Retired => None,
        }
    }

    fn contributors(devices: &Devices, device_key: &DeviceKey) -> Vec<ReporterId> {
        let Some(device_id) = resolved(devices, device_key) else {
            return Vec::new();
        };
        match devices.state(device_id) {
            DeviceStateLookup::Retained(state) => state.contributors.clone(),
            DeviceStateLookup::Retired => Vec::new(),
        }
    }

    /// Run the frames one accepted whole set needs: discovery admits the scan on the first frame
    /// and reconciliation sees the accepted set on the next one.
    fn run_until_reconciled(app: &mut App) {
        app.update();
        app.update();
    }

    #[test]
    fn two_reporters_naming_one_display_produce_one_device_with_two_contributors() {
        let mut app = app_with_scheme();
        let first = add_reporter(&mut app, || vec![record(ReportedAs::Keyed(key("panel-a")))]);
        let second = add_reporter(&mut app, || vec![record(ReportedAs::Keyed(key("panel-a")))]);

        run_until_reconciled(&mut app);

        let devices = app.world().resource::<Devices>();
        assert_eq!(devices.count(), 1);
        assert_eq!(contributors(devices, &key("panel-a")), vec![first, second]);
    }

    #[test]
    fn one_reporter_repeating_a_key_within_one_scan_reports_it_as_duplicated() {
        let mut app = app_with_scheme();
        add_reporter(&mut app, || {
            vec![
                record(ReportedAs::Keyed(key("panel-a"))),
                record(ReportedAs::Keyed(key("panel-a"))),
            ]
        });

        run_until_reconciled(&mut app);

        let devices = app.world().resource::<Devices>();
        assert!(devices.duplicate_keys().contains(&key("panel-a")));
        // The pass draws no conclusion from the duplication: the key still resolves to one device,
        // and the identity verdict stage is what turns the report into an unverified verdict.
        assert_eq!(devices.count(), 1);
    }

    #[test]
    fn a_child_of_an_unreachable_parent_is_unreachable_and_not_absent() {
        let mut app = app_with_scheme();
        add_reporter(&mut app, || {
            let mut parent = record(ReportedAs::Keyed(key("capture-card")));
            parent.presence = Presence::Unreachable {
                since: Duration::from_secs(3),
            };
            let mut child = record(ReportedAs::Keyed(key("camera")));
            child.parent = ReportedParent::ChildOf(key("capture-card"));
            vec![parent, child]
        });

        run_until_reconciled(&mut app);

        let devices = app.world().resource::<Devices>();
        assert!(matches!(
            presence_of(devices, &key("camera")),
            Some(Presence::Unreachable { .. })
        ));
    }

    #[test]
    fn a_set_listing_children_before_parents_still_ingests_roots_first() {
        let mut app = app_with_scheme();
        add_reporter(&mut app, || {
            let mut grandchild = record(ReportedAs::Keyed(key("camera")));
            grandchild.parent = ReportedParent::ChildOf(key("capture-card"));
            let mut child = record(ReportedAs::Keyed(key("capture-card")));
            child.parent = ReportedParent::ChildOf(key("dock"));
            let mut root = record(ReportedAs::Keyed(key("dock")));
            root.presence = Presence::Unreachable {
                since: Duration::from_secs(1),
            };
            vec![grandchild, child, root]
        });

        run_until_reconciled(&mut app);

        // The root's uncertainty reaches the leaf, which is only possible if the fold settled the
        // root before the two devices that were reported ahead of it.
        let devices = app.world().resource::<Devices>();
        assert!(matches!(
            presence_of(devices, &key("camera")),
            Some(Presence::Unreachable { .. })
        ));
        assert!(matches!(
            presence_of(devices, &key("capture-card")),
            Some(Presence::Unreachable { .. })
        ));
    }

    #[test]
    fn a_child_whose_parent_is_missing_from_the_whole_set_is_unreachable() {
        let mut app = app_with_scheme();
        add_reporter(&mut app, || {
            let mut child = record(ReportedAs::Keyed(key("camera")));
            child.parent = ReportedParent::ChildOf(key("capture-card"));
            vec![child]
        });

        run_until_reconciled(&mut app);

        let devices = app.world().resource::<Devices>();
        assert!(matches!(
            presence_of(devices, &key("camera")),
            Some(Presence::Unreachable { .. })
        ));
    }

    #[test]
    fn resolution_names_the_unresolved_case_without_an_optional_handle() {
        let mut app = app_with_scheme();
        add_reporter(&mut app, || vec![record(ReportedAs::Keyed(key("panel-a")))]);

        run_until_reconciled(&mut app);

        let devices = app.world().resource::<Devices>();
        assert!(matches!(
            devices.resolve(&key("panel-a")),
            DeviceResolution::Resolved(_)
        ));
        assert_eq!(
            devices.resolve(&key("never-reported")),
            DeviceResolution::NotResolved
        );
    }

    #[test]
    fn a_settled_frame_leaves_the_revision_unmoved_and_allocates_nothing() {
        let mut app = app_with_scheme();
        add_reporter_with_cadence(
            &mut app,
            || vec![record(ReportedAs::Keyed(key("panel-a")))],
            DiscoveryCadence::EventDriven {
                backstop: Duration::from_hours(1),
            },
        );
        let wedged = add_reporter_with_cadence(
            &mut app,
            || vec![record(ReportedAs::Keyed(key("panel-b")))],
            DiscoveryCadence::Periodic {
                interval: Duration::from_secs(5),
            },
        );

        run_until_reconciled(&mut app);
        let settled_revision = *app.world().resource::<RiggingRevision>();

        // Nothing completed and no lease can expire within an hour-long backstop, so this pass has
        // no work: it must neither move the revision nor ask the allocator for anything.
        let allocations = reconcile_once(&mut app, FrameClockReading::Measurable(Instant::now()));

        assert_eq!(allocations, 0);
        assert_eq!(*app.world().resource::<RiggingRevision>(), settled_revision);

        let report_grace = app.world().resource::<RiggingLimits>().report_grace;
        app.world_mut()
            .resource_mut::<Reporters>()
            .backdate_completion(wedged, report_grace + Duration::from_mins(10));
        reconcile_once(&mut app, FrameClockReading::Measurable(Instant::now()));

        // The wedged reporter stays wedged for as long as the application runs. Its device was
        // judged once, so every following frame has to settle without collecting anything.
        let allocations = reconcile_once(&mut app, FrameClockReading::Measurable(Instant::now()));

        assert_eq!(allocations, 0);
        assert_eq!(*app.world().resource::<RiggingRevision>(), settled_revision);
        assert!(matches!(
            presence_of(app.world().resource::<Devices>(), &key("panel-b")),
            Some(Presence::Unreachable { .. })
        ));
    }

    #[test]
    fn a_reporter_silent_past_its_cadence_loses_its_devices_and_no_others() {
        let mut app = app_with_scheme();
        let silent = add_reporter_with_cadence(
            &mut app,
            || vec![record(ReportedAs::Keyed(key("silent-panel")))],
            DiscoveryCadence::Periodic {
                interval: Duration::from_secs(5),
            },
        );
        add_reporter_with_cadence(
            &mut app,
            || vec![record(ReportedAs::Keyed(key("live-panel")))],
            DiscoveryCadence::Periodic {
                interval: Duration::from_secs(5),
            },
        );

        run_until_reconciled(&mut app);

        let report_grace = app.world().resource::<RiggingLimits>().report_grace;
        app.world_mut()
            .resource_mut::<Reporters>()
            .backdate_completion(silent, report_grace + Duration::from_mins(10));
        reconcile_once(&mut app, FrameClockReading::Measurable(Instant::now()));

        let devices = app.world().resource::<Devices>();
        assert!(matches!(
            presence_of(devices, &key("silent-panel")),
            Some(Presence::Unreachable { .. })
        ));
        assert_eq!(
            presence_of(devices, &key("live-panel")),
            Some(Presence::Present)
        );
    }

    #[test]
    fn a_silent_reporter_does_not_lower_a_device_a_fresh_reporter_still_reports() {
        let mut app = app_with_scheme();
        let winit_like = add_reporter_with_cadence(
            &mut app,
            || vec![record(ReportedAs::Keyed(key("panel-a")))],
            DiscoveryCadence::Periodic {
                interval: Duration::from_secs(5),
            },
        );
        let wedged = add_reporter_with_cadence(
            &mut app,
            || vec![record(ReportedAs::Keyed(key("panel-a")))],
            DiscoveryCadence::Periodic {
                interval: Duration::from_secs(5),
            },
        );

        run_until_reconciled(&mut app);

        let report_grace = app.world().resource::<RiggingLimits>().report_grace;
        let silence = report_grace + Duration::from_mins(10);
        app.world_mut()
            .resource_mut::<Reporters>()
            .backdate_completion(wedged, silence);
        reconcile_once(&mut app, FrameClockReading::Measurable(Instant::now()));

        // One reporter wedging withdraws its evidence; it does not report the device gone, and the
        // reporter that still enumerates the device every few seconds keeps it present.
        assert_eq!(
            presence_of(app.world().resource::<Devices>(), &key("panel-a")),
            Some(Presence::Present)
        );

        app.world_mut()
            .resource_mut::<Reporters>()
            .backdate_completion(winit_like, silence);
        reconcile_once(&mut app, FrameClockReading::Measurable(Instant::now()));

        assert!(matches!(
            presence_of(app.world().resource::<Devices>(), &key("panel-a")),
            Some(Presence::Unreachable { .. })
        ));
    }

    #[test]
    fn a_reporter_that_never_completed_a_scan_is_not_stale() {
        let mut app = app_with_scheme();
        add_reporter_with_cadence(
            &mut app,
            Vec::new,
            DiscoveryCadence::Periodic {
                interval: Duration::from_secs(5),
            },
        );

        // No frame has run, so the reporter holds no completed set. A lease that judged silence
        // from registration would call every reporter stale before its first scan.
        let allocations = reconcile_once(&mut app, FrameClockReading::Measurable(Instant::now()));

        assert_eq!(allocations, 0);
        assert_eq!(app.world().resource::<RiggingRevision>().get(), 0);
    }

    #[test]
    fn a_reporter_with_no_declared_cadence_is_never_stale() {
        let mut app = app_with_scheme();
        let on_demand = add_reporter_with_cadence(
            &mut app,
            || vec![record(ReportedAs::Keyed(key("panel-a")))],
            DiscoveryCadence::OnDemand,
        );

        run_until_reconciled(&mut app);
        app.world_mut()
            .resource_mut::<Reporters>()
            .backdate_completion(on_demand, Duration::from_hours(24));
        reconcile_once(&mut app, FrameClockReading::Measurable(Instant::now()));

        assert_eq!(
            presence_of(app.world().resource::<Devices>(), &key("panel-a")),
            Some(Presence::Present)
        );
    }

    #[test]
    fn evidence_only_records_join_a_keyed_record_through_the_reported_os_handle() {
        let mut app = app_with_scheme();
        let keyed = add_reporter(&mut app, || {
            let mut keyed_record = record(ReportedAs::Keyed(key("panel-a")));
            keyed_record.os_id =
                OsDeviceId::Reported(ReportedId::new("display-7").expect("well formed"));
            vec![keyed_record]
        });
        let evidence = add_reporter(&mut app, || {
            let mut evidence_record = record(ReportedAs::MatchEvidenceOnly);
            evidence_record.os_id =
                OsDeviceId::Reported(ReportedId::new("display-7").expect("well formed"));
            vec![evidence_record]
        });

        run_until_reconciled(&mut app);

        let devices = app.world().resource::<Devices>();
        assert_eq!(devices.count(), 1);
        assert_eq!(
            contributors(devices, &key("panel-a")),
            vec![keyed, evidence]
        );
    }

    #[test]
    fn a_joined_evidence_only_record_merges_as_a_co_report_of_the_device_it_joins() {
        let mut app = app_with_scheme();
        add_reporter(&mut app, || {
            let mut keyed_record = record(ReportedAs::Keyed(key("panel-a")));
            keyed_record.os_id =
                OsDeviceId::Reported(ReportedId::new("display-7").expect("well formed"));
            vec![keyed_record]
        });
        add_reporter(&mut app, || {
            let mut evidence_record = record(ReportedAs::MatchEvidenceOnly);
            evidence_record.os_id =
                OsDeviceId::Reported(ReportedId::new("display-7").expect("well formed"));
            evidence_record.presence = Presence::Unreachable {
                since: Duration::from_secs(4),
            };
            evidence_record.claim = Claim::Held;
            vec![evidence_record]
        });

        run_until_reconciled(&mut app);

        // The joined record is a report about the same device, so the merge takes its presence and
        // its more restrictive claim rather than only its reporter id.
        let devices = app.world().resource::<Devices>();
        assert!(matches!(
            presence_of(devices, &key("panel-a")),
            Some(Presence::Unreachable { .. })
        ));
        assert_eq!(claim_of(devices, &key("panel-a")), Some(Claim::Held));
    }

    #[test]
    fn a_platform_handle_two_reporters_give_different_keys_joins_nothing() {
        let mut app = app_with_scheme();
        add_reporter(&mut app, || {
            let mut keyed_record = record(ReportedAs::Keyed(key("panel-a")));
            keyed_record.os_id =
                OsDeviceId::Reported(ReportedId::new("display-7").expect("well formed"));
            vec![keyed_record]
        });
        add_reporter(&mut app, || {
            let mut keyed_record = record(ReportedAs::Keyed(key("panel-b")));
            keyed_record.os_id =
                OsDeviceId::Reported(ReportedId::new("display-7").expect("well formed"));
            vec![keyed_record]
        });
        let evidence = add_reporter(&mut app, || {
            let mut evidence_record = record(ReportedAs::MatchEvidenceOnly);
            evidence_record.os_id =
                OsDeviceId::Reported(ReportedId::new("display-7").expect("well formed"));
            vec![evidence_record]
        });

        // Discovery admits a bounded number of jobs per frame, so three reporters need more frames
        // than two before every whole set has been accepted.
        run_until_reconciled(&mut app);
        run_until_reconciled(&mut app);

        // The handle names two keys, so it names neither: attaching the evidence to whichever key
        // was ingested last would be the plausible fallback exact-match identity forbids.
        let devices = app.world().resource::<Devices>();
        assert_eq!(devices.count(), 2);
        for device_key in [key("panel-a"), key("panel-b")] {
            assert!(!contributors(devices, &device_key).contains(&evidence));
        }
    }

    #[test]
    fn an_evidence_only_record_that_matches_nothing_produces_no_device_and_no_key() {
        let mut app = app_with_scheme();
        add_reporter(&mut app, || {
            let mut evidence_record = record(ReportedAs::MatchEvidenceOnly);
            evidence_record.os_id =
                OsDeviceId::Reported(ReportedId::new("display-9").expect("well formed"));
            vec![evidence_record]
        });

        run_until_reconciled(&mut app);

        assert_eq!(app.world().resource::<Devices>().count(), 0);
    }

    #[test]
    fn two_evidence_only_records_without_platform_handles_do_not_join_each_other() {
        let mut app = app_with_scheme();
        add_reporter(&mut app, || vec![record(ReportedAs::MatchEvidenceOnly)]);
        add_reporter(&mut app, || vec![record(ReportedAs::MatchEvidenceOnly)]);

        run_until_reconciled(&mut app);

        // Both records carry `PlatformReportedNothing`, which compares equal to itself. Joining on
        // it would mint a device out of two reports that share no evidence at all.
        assert_eq!(app.world().resource::<Devices>().count(), 0);
    }

    #[test]
    fn a_key_naming_an_unregistered_scheme_is_rejected_at_the_ingest_boundary() {
        let mut app = app_with_scheme();
        add_reporter(&mut app, || {
            vec![
                record(ReportedAs::Keyed(keyed_in("not-registered", "panel-a"))),
                record(ReportedAs::Keyed(key("panel-b"))),
            ]
        });

        run_until_reconciled(&mut app);

        let devices = app.world().resource::<Devices>();
        assert_eq!(devices.count(), 1);
        assert!(
            devices
                .unregistered_schemes()
                .contains(&SchemeName::new("not-registered").expect("test scheme is well formed"))
        );
        assert_eq!(
            devices.resolve(&keyed_in("not-registered", "panel-a")),
            DeviceResolution::NotResolved
        );
    }

    #[test]
    fn merging_reads_each_reporter_set_once_rather_than_joining_them_pairwise() {
        // Three reporters that each name the same two devices. A pairwise join would compare every
        // reporter against every other; the single-pass merge visits six records and stops.
        let mut app = app_with_scheme();
        let first = add_reporter(&mut app, || {
            vec![
                record(ReportedAs::Keyed(key("panel-a"))),
                record(ReportedAs::Keyed(key("panel-b"))),
            ]
        });
        let second = add_reporter(&mut app, || {
            vec![
                record(ReportedAs::Keyed(key("panel-a"))),
                record(ReportedAs::Keyed(key("panel-b"))),
            ]
        });
        let third = add_reporter(&mut app, || {
            vec![
                record(ReportedAs::Keyed(key("panel-a"))),
                record(ReportedAs::Keyed(key("panel-b"))),
            ]
        });

        // Discovery admits a bounded number of jobs per frame, so three reporters need more frames
        // than two before every whole set has been accepted.
        run_until_reconciled(&mut app);
        run_until_reconciled(&mut app);

        let devices = app.world().resource::<Devices>();
        assert_eq!(devices.count(), 2);
        for device_key in [key("panel-a"), key("panel-b")] {
            assert_eq!(
                contributors(devices, &device_key),
                vec![first, second, third]
            );
            assert!(!devices.duplicate_keys().contains(&device_key));
        }
    }
}
