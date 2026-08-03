use std::any::TypeId;
use std::collections::HashMap;
use std::collections::HashSet;

use bevy::ecs::entity::Entity;
use bevy::ecs::reflect::ReflectComponent;
use bevy::ecs::reflect::ReflectResource;
use bevy::prelude::Component;
use bevy::prelude::Reflect;
use bevy::prelude::Resource;

use crate::Claim;
use crate::DeviceId;
use crate::DeviceKey;
use crate::Presence;
use crate::ReportedParent;
use crate::ReporterId;
use crate::SchemeName;

/// Marker for the entity that mirrors one reconciled device.
///
/// Queries and the Bevy Remote Protocol reach device state through entities, so the kernel keeps
/// one entity per reconciled device alongside the `Devices` registry. The marker exists so a query
/// can select devices without naming every component the projection inserts.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Component, Reflect)]
#[reflect(Component, PartialEq)]
pub struct Device;

/// Marker inserted only while a device is present **and** its claim permits this process to use
/// it.
///
/// The two facts are separate components because they answer separate questions, and a consumer
/// that has to combine them at every call site eventually combines them wrongly: a camera that is
/// present but open in another application is not usable. This marker is the combined guarantee,
/// so a system can query it directly instead of restating the rule.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Component, Reflect)]
#[reflect(Component, PartialEq)]
pub struct PresentWithUsableClaim;

/// What the kernel currently believes about every device, keyed by the handle it issued.
///
/// Durable state cannot live only on entities: retirement by key runs during startup and
/// immediately after a departure, when no entity is alive. The entity projection mirrors this
/// registry so queries and the Bevy Remote Protocol can read the same facts.
///
/// State is keyed by `DeviceId` rather than by `DeviceKey` because the durable key is two strings
/// and hashing both on every policy query from application code is the cost that matters. One map
/// resolves a durable key to a handle; everything else is keyed by the copyable handle.
#[derive(Debug, Default, Resource, Reflect)]
#[reflect(Resource)]
pub struct Devices {
    ids:                  HashMap<DeviceKey, DeviceId>,
    state:                HashMap<DeviceId, ReconciledDeviceState>,
    entity:               HashMap<DeviceId, Entity>,
    /// Issues `DeviceId`. Monotonic, never reused within a process, so a retired handle dangles
    /// instead of denoting a later device.
    next:                 u64,
    duplicate_keys:       HashSet<DeviceKey>,
    unregistered_schemes: HashSet<SchemeName>,
}

impl Devices {
    /// Turn one durable key into the handle this process issued for it.
    ///
    /// Lookup is exact or nothing. There is deliberately no nearest-match, no first-of-kind, and
    /// no fallback to a primary device: every live defect in this area came from a fallback
    /// returning something plausible instead of nothing.
    #[must_use]
    pub fn resolve(&self, key: &DeviceKey) -> DeviceResolution {
        self.ids
            .get(key)
            .map_or(DeviceResolution::NotResolved, |device_id| {
                DeviceResolution::Resolved(*device_id)
            })
    }

    /// Read what the latest reconcile pass concluded about one handle.
    #[must_use]
    pub fn state(&self, device_id: DeviceId) -> DeviceStateLookup<'_> {
        self.state
            .get(&device_id)
            .map_or(DeviceStateLookup::Retired, DeviceStateLookup::Retained)
    }

    /// How many devices the latest reconcile pass retained.
    #[must_use]
    pub fn count(&self) -> usize { self.state.len() }

    /// Keys that arrived more than once from a single reporter in the latest reconcile pass.
    ///
    /// This is one set per pass, not one fact per device, and it is replaced on every pass that
    /// ingests reports. Reconciliation draws no conclusion from it: the identity verdict stage
    /// turns each key into an unverified verdict, which is what stops a weak scheme — two
    /// identical webcams under one device name, neither reporting a serial — from presenting as
    /// proven.
    #[must_use]
    pub const fn duplicate_keys(&self) -> &HashSet<DeviceKey> { &self.duplicate_keys }

    /// Identity spaces the latest reconcile pass rejected at its ingest boundary.
    ///
    /// A reported key whose scheme no provider registered during app construction never becomes
    /// device state, because an unregistered name is a typo rather than an identity space. The
    /// rejected names are retained here so the mistake is visible in a report instead of silently
    /// producing a device that no consumer can address.
    #[must_use]
    pub const fn unregistered_schemes(&self) -> &HashSet<SchemeName> { &self.unregistered_schemes }

    /// Replace the reconciled set with the current pass's conclusions.
    ///
    /// `reconciled` arrives roots first so presence is already folded down each parent chain.
    /// Keys absent from it are retired by key: their handle, their state, and their entity mapping
    /// are dropped, and a device that returns later receives a newly issued handle.
    pub(crate) fn replace_reconciled(
        &mut self,
        reconciled: Vec<ReconciledDeviceState>,
        duplicate_keys: HashSet<DeviceKey>,
        unregistered_schemes: HashSet<SchemeName>,
    ) {
        let mut ids = HashMap::with_capacity(reconciled.len());
        let mut state = HashMap::with_capacity(reconciled.len());

        for reconciled_device_state in reconciled {
            let device_id = self
                .ids
                .get(&reconciled_device_state.key)
                .copied()
                .unwrap_or_else(|| self.issue());
            ids.insert(reconciled_device_state.key.clone(), device_id);
            state.insert(device_id, reconciled_device_state);
        }

        self.entity
            .retain(|device_id, _| state.contains_key(device_id));
        self.ids = ids;
        self.state = state;
        self.duplicate_keys = duplicate_keys;
        self.unregistered_schemes = unregistered_schemes;
    }

    /// Read every retained state, so the freshness lease can ask whether it has work to do before
    /// reconciliation pays for a merge pass.
    pub(crate) fn states(&self) -> impl Iterator<Item = &ReconciledDeviceState> {
        self.state.values()
    }

    const fn issue(&mut self) -> DeviceId {
        let device_id = DeviceId::new(self.next);
        self.next += 1;

        device_id
    }
}

/// What the kernel currently believes about one device.
///
/// Durable and entity-free, so retirement by key runs with no entity alive. It holds no reporter
/// evidence: device records, whole sets, and capability values stay with the reporter registry
/// that retains them, and none of those derives `Reflect` in any case. Only normalized facts land
/// here — which capability types were declared, and which the contributors disagree about.
#[derive(Clone, Debug, Reflect)]
pub struct ReconciledDeviceState {
    /// The durable name, so a handle can be turned back into one without a reverse scan of the
    /// key-to-handle map.
    pub key:          DeviceKey,
    /// What this device hangs off. Drives the conjunctive presence fold and the retirement of
    /// descendants by key.
    pub parent:       ReportedParent,
    /// Reachability after folding every contributor's report against this device's parent chain.
    ///
    /// Compared by variant, never by value: `crate::Presence::Unreachable` carries the reporter's
    /// elapsed time, which grows on every scan, so comparing values would report a change forever
    /// and defeat the once-per-change rule the entity projection depends on.
    pub presence:     Presence,
    /// Exclusive ownership, retained separately from `presence` because a camera can be present
    /// while another process owns its capture stream.
    pub claim:        Claim,
    /// Every reporter contributing to this device, in the order their sets were ingested.
    ///
    /// The freshness lease reads this to find whose devices to mark unreachable when one reporter
    /// goes stale. A `Vec` rather than a small-vector type: reflection support for those is
    /// feature-gated in Bevy, and a device rarely has more than two contributors.
    pub contributors: Vec<ReporterId>,
    /// Capability component types the contributors declared for this device.
    pub declared:     HashSet<TypeId>,
    /// Capability component types whose values the contributors disagree about.
    ///
    /// Facts only; the values stay with the reporters that own them, because the erased capability
    /// payload is neither clonable nor reflectable and copying it would create a second
    /// authoritative record that can drift from the reporter's.
    pub disputed:     HashSet<TypeId>,
}

/// Result of resolving one durable key into the current process-local handle.
///
/// A named result rather than an optional handle: the two outcomes lead to different work, and a
/// caller that reads "no handle" as "nothing to do" would silently skip a device that is merely
/// waiting for its reporter's first complete scan.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Reflect)]
pub enum DeviceResolution {
    /// No reporter has contributed a record under this key during this process, so the key names
    /// nothing that can be queried, claimed, or driven right now.
    NotResolved,
    /// The key names a device the kernel currently retains, and this handle addresses it.
    Resolved(DeviceId),
}

/// Result of reading the kernel's belief about one handle.
#[derive(Clone, Copy, Debug)]
pub enum DeviceStateLookup<'a> {
    /// The handle addresses no retained device: it was issued for a device that has since departed
    /// or its key was never reported in this process.
    Retired,
    /// The kernel retains this device and its latest reconciled state.
    Retained(&'a ReconciledDeviceState),
}

/// One global revision, folded from every reporter's own revision.
///
/// It advances once per reconcile pass in which any reporter returned a complete scan, whether or
/// not the contents changed. Counting completed scans rather than content changes is what makes a
/// rapid absent-then-present cycle observable across consecutive passes: the set looks identical at
/// both ends, so a content hash would report no change and a panel watching the revision would
/// stay blank.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default, Resource, Reflect)]
#[reflect(opaque)]
#[reflect(Resource)]
pub struct RiggingRevision(u64);

impl RiggingRevision {
    /// Report the number of reconcile passes that ingested at least one completed scan.
    #[must_use]
    pub const fn get(self) -> u64 { self.0 }

    pub(crate) const fn advance(&mut self) { self.0 += 1; }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::any::TypeId;
    use std::collections::HashSet;
    use std::error::Error;

    use bevy::app::App;
    use bevy::ecs::reflect::AppTypeRegistry;
    use bevy::ecs::reflect::ReflectComponent;
    use bevy::ecs::reflect::ReflectResource;
    use bevy::reflect::FromReflect;
    use bevy::reflect::tuple_struct::DynamicTupleStruct;

    use super::Device;
    use super::DeviceResolution;
    use super::DeviceStateLookup;
    use super::Devices;
    use super::PresentWithUsableClaim;
    use super::ReconciledDeviceState;
    use super::RiggingRevision;
    use crate::Claim;
    use crate::DeviceIdSource;
    use crate::DeviceKey;
    use crate::DeviceKind;
    use crate::Presence;
    use crate::ReportedId;
    use crate::ReportedParent;
    use crate::SchemeName;

    fn reported_key(value: &str) -> Result<DeviceKey, Box<dyn Error>> {
        Ok(DeviceKey {
            kind: DeviceKind::Display,
            id:   DeviceIdSource::Reported {
                scheme: SchemeName::new("edid-serial")?,
                value:  ReportedId::new(value)?,
            },
        })
    }

    fn reconciled(key: DeviceKey) -> ReconciledDeviceState {
        ReconciledDeviceState {
            key,
            parent: ReportedParent::Root,
            presence: Presence::Present,
            claim: Claim::NotApplicable,
            contributors: Vec::new(),
            declared: HashSet::new(),
            disputed: HashSet::new(),
        }
    }

    #[test]
    fn resolution_distinguishes_an_unknown_key_from_a_retained_handle() -> Result<(), Box<dyn Error>>
    {
        let key = reported_key("DELL-U2723QE-9J4K2H3")?;
        let absent_key = reported_key("DELL-U2723QE-OTHER")?;
        let mut devices = Devices::default();

        assert_eq!(devices.resolve(&key), DeviceResolution::NotResolved);

        devices.replace_reconciled(
            vec![reconciled(key.clone())],
            HashSet::new(),
            HashSet::new(),
        );

        let DeviceResolution::Resolved(device_id) = devices.resolve(&key) else {
            panic!("an ingested key must resolve to the handle the registry issued");
        };
        assert!(matches!(
            devices.state(device_id),
            DeviceStateLookup::Retained(state) if state.key == key
        ));
        assert_eq!(devices.resolve(&absent_key), DeviceResolution::NotResolved);

        Ok(())
    }

    #[test]
    fn a_returning_key_never_reuses_the_retired_handle() -> Result<(), Box<dyn Error>> {
        let key = reported_key("DELL-U2723QE-9J4K2H3")?;
        let mut devices = Devices::default();
        devices.replace_reconciled(
            vec![reconciled(key.clone())],
            HashSet::new(),
            HashSet::new(),
        );
        let DeviceResolution::Resolved(first) = devices.resolve(&key) else {
            panic!("an ingested key must resolve");
        };

        devices.replace_reconciled(Vec::new(), HashSet::new(), HashSet::new());
        assert!(matches!(devices.state(first), DeviceStateLookup::Retired));

        devices.replace_reconciled(
            vec![reconciled(key.clone())],
            HashSet::new(),
            HashSet::new(),
        );
        let DeviceResolution::Resolved(second) = devices.resolve(&key) else {
            panic!("a returning key must resolve again");
        };

        assert_ne!(first, second);

        Ok(())
    }

    #[test]
    fn an_unchanged_key_keeps_its_handle_across_passes() -> Result<(), Box<dyn Error>> {
        let key = reported_key("DELL-U2723QE-9J4K2H3")?;
        let mut devices = Devices::default();
        devices.replace_reconciled(
            vec![reconciled(key.clone())],
            HashSet::new(),
            HashSet::new(),
        );
        let first = devices.resolve(&key);

        devices.replace_reconciled(
            vec![reconciled(key.clone())],
            HashSet::new(),
            HashSet::new(),
        );

        assert_eq!(devices.resolve(&key), first);

        Ok(())
    }

    #[test]
    fn reflection_cannot_construct_a_rigging_revision() {
        let mut dynamic_rigging_revision = DynamicTupleStruct::default();
        dynamic_rigging_revision.insert(0_u64);

        assert!(RiggingRevision::from_reflect(&dynamic_rigging_revision).is_none());
    }

    #[test]
    fn device_registry_types_register_reflection_metadata() {
        let app = App::new();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();

        for type_id in [
            TypeId::of::<Device>(),
            TypeId::of::<PresentWithUsableClaim>(),
        ] {
            assert!(
                type_registry
                    .get_type_data::<ReflectComponent>(type_id)
                    .is_some()
            );
        }
        for type_id in [TypeId::of::<Devices>(), TypeId::of::<RiggingRevision>()] {
            assert!(
                type_registry
                    .get_type_data::<ReflectResource>(type_id)
                    .is_some()
            );
        }

        drop(type_registry);
    }
}
