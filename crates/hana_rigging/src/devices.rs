use std::any::TypeId;
use std::collections::HashMap;
use std::collections::HashSet;

use bevy::ecs::entity::Entity;
use bevy::ecs::reflect::ReflectComponent;
use bevy::ecs::reflect::ReflectResource;
use bevy::prelude::Component;
use bevy::prelude::Reflect;
use bevy::prelude::Resource;
use thiserror::Error;

use crate::ApplyPermit;
use crate::AttachmentPath;
use crate::Attempt;
use crate::AttemptId;
use crate::Claim;
use crate::ConfiguredDeviceConnection;
use crate::ConfiguredDeviceMode;
use crate::DeviceId;
use crate::DeviceKey;
use crate::IdentityDecisionOwed;
use crate::IdentityVerdict;
use crate::Presence;
use crate::ReportedParent;
use crate::ReporterId;
use crate::SchemeName;

/// First identifier `Attempts` issues, chosen so no issued value equals `AttemptId::default()`.
const FIRST_ISSUED_ATTEMPT: u64 = 1;

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
    ///
    /// A key a reporter still names while it stops reporting the unit `Presence::Present` is the
    /// other way a device leaves. It is recorded through `Presence::is_same_variant`, the one
    /// comparison the entity mirror also uses, and it retires nothing: the unplugged monitor keeps
    /// its handle, its state, and its entity while the roles bound to it learn it is gone.
    pub(crate) fn replace_reconciled(
        &mut self,
        reconciled: Vec<ReconciledDeviceState>,
        duplicate_keys: HashSet<DeviceKey>,
        unregistered_schemes: HashSet<SchemeName>,
    ) -> ReconciledDeviceChanges {
        let mut ids = HashMap::with_capacity(reconciled.len());
        let mut state = HashMap::with_capacity(reconciled.len());
        let mut changes = ReconciledDeviceChanges::default();

        for reconciled_device_state in reconciled {
            let device_id = self
                .ids
                .get(&reconciled_device_state.key)
                .copied()
                .unwrap_or_else(|| self.issue());
            let dispute_changed = self
                .state
                .get(&device_id)
                .map_or(!reconciled_device_state.disputed.is_empty(), |held| {
                    held.disputed != reconciled_device_state.disputed
                });
            if dispute_changed {
                changes.disputes_changed.push(device_id);
            }
            if self.state.get(&device_id).is_some_and(|held| {
                held.presence == Presence::Present
                    && !held
                        .presence
                        .is_same_variant(reconciled_device_state.presence)
            }) {
                changes.departed.push(DepartedDevice {
                    key:       reconciled_device_state.key.clone(),
                    departure: DeviceDeparture::RetainedButNotPresent,
                });
            }
            ids.insert(reconciled_device_state.key.clone(), device_id);
            state.insert(device_id, reconciled_device_state);
        }

        for (key, device_id) in &self.ids {
            if !state.contains_key(device_id) {
                changes.departed.push(DepartedDevice {
                    key:       key.clone(),
                    departure: DeviceDeparture::KeyLeftTheSet,
                });
            }
        }
        self.entity.retain(|device_id, entity| {
            let retained = state.contains_key(device_id);
            if !retained {
                changes.orphaned_entities.push(*entity);
            }

            retained
        });
        self.ids = ids;
        self.state = state;
        self.duplicate_keys = duplicate_keys;
        self.unregistered_schemes = unregistered_schemes;

        changes
    }

    /// Remember which entity mirrors one handle, so the next pass updates that entity instead of
    /// spawning a second one for the same device.
    pub(crate) fn project_entity(&mut self, device_id: DeviceId, entity: Entity) {
        self.entity.insert(device_id, entity);
    }

    /// Read every retained state, so a consumer that did not author the keys can list what the
    /// kernel currently holds.
    ///
    /// Iteration order follows the state map and is therefore unspecified: a caller that needs the
    /// order reporters supplied should key off `ReconciledDeviceState::key` instead.
    pub fn states(&self) -> impl Iterator<Item = &ReconciledDeviceState> { self.state.values() }

    /// Find the entity mirroring one handle, so a caller holding a durable key can reach the
    /// components the projection inserted without scanning every device entity.
    #[must_use]
    pub fn entity(&self, device_id: DeviceId) -> DeviceEntityLookup {
        self.entity
            .get(&device_id)
            .map_or(DeviceEntityLookup::NotProjected, |entity| {
                DeviceEntityLookup::Projected(*entity)
            })
    }

    /// Decide whether one device may be put in service.
    ///
    /// This and `Self::authorize_service_for` are the only in-service decision points in the
    /// kernel. Every check is against the merged view, so a co-reported device folds
    /// most-restrictive-wins: one reporter seeing an idle camera does not authorize capture when
    /// another watched a second application open it.
    ///
    /// `disputed` is deliberately ignored here. A unit whose reporters contradict each other about
    /// one capability is still correct about every other one, so refusing the whole device would
    /// take a Stream Deck dark over a disagreement about its LED brightness range. Use
    /// `Self::authorize_service_for` when the operation depends on a specific capability.
    ///
    /// # Errors
    ///
    /// Returns the `ApplyAuthorizationError` naming the first check that refused: an unknown
    /// handle, an identity that was never proven, a unit that is not reachable, a claim another
    /// process holds, or an authored entry the application marked offline.
    pub fn authorize_service(
        &self,
        device_id: DeviceId,
    ) -> Result<ApplyPermit, ApplyAuthorizationError> {
        self.in_service_state(device_id)?;

        Ok(ApplyPermit::in_service())
    }

    /// Decide whether one device may be put in service for the capability `C` specifically.
    ///
    /// Runs every device-wide check and additionally refuses when the contributors disagree about
    /// `C`. A device stays drivable for the capabilities its reporters agree about.
    ///
    /// # Errors
    ///
    /// Returns every `ApplyAuthorizationError` `Self::authorize_service` returns, plus
    /// `ApplyAuthorizationError::CapabilityDisputed` when `C` is in the disputed set.
    pub fn authorize_service_for<C: 'static>(
        &self,
        device_id: DeviceId,
    ) -> Result<ApplyPermit, ApplyAuthorizationError> {
        let reconciled_device_state = self.in_service_state(device_id)?;
        if reconciled_device_state
            .disputed
            .contains(&TypeId::of::<C>())
        {
            return Err(ApplyAuthorizationError::CapabilityDisputed {
                key:       reconciled_device_state.key.clone(),
                type_path: std::any::type_name::<C>().to_owned(),
            });
        }

        Ok(ApplyPermit::in_service())
    }

    /// Decide whether one device may receive a saved configuration back.
    ///
    /// The weaker gate: it additionally accepts `IdentityVerdict::RestoreOnly`, so a window can go
    /// back to the monitor a synthesized key names. It still refuses an offline authored entry,
    /// because returning a saved layout to hardware the application withdrew is still an automatic
    /// action on that hardware.
    ///
    /// # Errors
    ///
    /// Returns the `ApplyAuthorizationError` naming the first check that refused, on the same
    /// terms as `Self::authorize_service` except that a restore-only identity passes.
    pub fn authorize_restore(
        &self,
        device_id: DeviceId,
    ) -> Result<ApplyPermit, ApplyAuthorizationError> {
        let reconciled_device_state = self.authorized_state(device_id)?;
        if !reconciled_device_state.verdict.identified() {
            return Err(ApplyAuthorizationError::IdentityNotProven {
                key: reconciled_device_state.key.clone(),
            });
        }

        Ok(ApplyPermit::restore_only())
    }

    /// Run every device-wide in-service check and hand back the state both in-service predicates
    /// read, so neither of them resolves the same handle twice.
    fn in_service_state(
        &self,
        device_id: DeviceId,
    ) -> Result<&ReconciledDeviceState, ApplyAuthorizationError> {
        let reconciled_device_state = self.authorized_state(device_id)?;
        match reconciled_device_state.verdict {
            IdentityVerdict::Proven | IdentityVerdict::Authored => Ok(reconciled_device_state),
            _ => Err(ApplyAuthorizationError::IdentityNotProven {
                key: reconciled_device_state.key.clone(),
            }),
        }
    }

    /// Run the checks every predicate shares: the handle resolves, the authored mode permits driver
    /// work, the unit is reachable, and this process may use it.
    fn authorized_state(
        &self,
        device_id: DeviceId,
    ) -> Result<&ReconciledDeviceState, ApplyAuthorizationError> {
        let DeviceStateLookup::Retained(reconciled_device_state) = self.state(device_id) else {
            return Err(ApplyAuthorizationError::DeviceRetired { device_id });
        };
        if reconciled_device_state.mode == ConfiguredDeviceMode::Offline {
            return Err(ApplyAuthorizationError::Offline {
                key: reconciled_device_state.key.clone(),
            });
        }
        if reconciled_device_state.presence != Presence::Present {
            return Err(ApplyAuthorizationError::NotPresent {
                key: reconciled_device_state.key.clone(),
            });
        }
        match reconciled_device_state.claim {
            Claim::Held | Claim::Free | Claim::NotApplicable => {},
            Claim::Contended { .. } | Claim::Blocked { .. } => {
                return Err(ApplyAuthorizationError::ClaimUnavailable {
                    key: reconciled_device_state.key.clone(),
                });
            },
        }

        Ok(reconciled_device_state)
    }

    const fn issue(&mut self) -> DeviceId {
        let device_id = DeviceId::new(self.next);
        self.next += 1;

        device_id
    }
}

/// What the kernel currently believes about one device.
///
/// Durable and entity-free, so retirement by key runs with no entity alive. Capability *values*
/// stay with the reporter registry that retains them — `Box<dyn Reflect>` is neither clonable nor
/// reflectable, and a second copy could drift from the reporter's. What lands here is the
/// normalized conclusions the kernel itself drew, plus the one piece of reporter evidence a later
/// pass has to remember: `Self::attachment`, without which a returning unit could never be judged
/// against the slot the departed one occupied.
#[derive(Clone, Debug, Reflect)]
pub struct ReconciledDeviceState {
    /// The durable name, so a handle can be turned back into one without a reverse scan of the
    /// key-to-handle map.
    pub key:           DeviceKey,
    /// Whether this live unit corresponds to its durable key, and therefore what it may be asked
    /// to do.
    ///
    /// Computed here rather than reported: a verdict a reporter supplied would be a claim about
    /// identity that the merge is the only thing able to check.
    pub verdict:       IdentityVerdict,
    /// The `Displaced` or `WrongUnit` verdict a human still has to resolve, held separately from
    /// `Self::verdict` so a pass that reports a scan observation instead does not destroy it.
    ///
    /// A key duplicated within one scan is the case that separates the two: the duplicate must be
    /// reportable while the scan shows it and must clear when the scan stops, so it cannot be
    /// stored in the same slot as a verdict that outlives every scan.
    pub decision_owed: IdentityDecisionOwed,
    /// The authored operation mode for this key, `ConfiguredDeviceMode::Managed` when the
    /// application authored no inventory entry for it.
    ///
    /// Stamped during reconciliation so the authorization predicates answer from one value instead
    /// of asking every caller to carry the inventory to the decision point and combine the two
    /// rules themselves.
    pub mode:          ConfiguredDeviceMode,
    /// Where the contributors observed this unit attached.
    ///
    /// Retained across passes because the displaced-unit rule compares a departed key's slot with
    /// the slot a newly arrived unit occupies, and by the time the arrival is judged the departed
    /// reporter record is gone.
    pub attachment:    AttachmentPath,
    /// What this device hangs off. Drives the conjunctive presence fold and the retirement of
    /// descendants by key.
    pub parent:        ReportedParent,
    /// Reachability after folding every contributor's report against this device's parent chain.
    ///
    /// Compared by variant, never by value: `crate::Presence::Unreachable` carries the reporter's
    /// elapsed time, which grows on every scan, so comparing values would report a change forever
    /// and defeat the once-per-change rule the entity projection depends on.
    pub presence:      Presence,
    /// Exclusive ownership, retained separately from `presence` because a camera can be present
    /// while another process owns its capture stream.
    pub claim:         Claim,
    /// Every reporter contributing to this device, in the order their sets were ingested.
    ///
    /// The freshness lease reads this to find whose devices to mark unreachable when one reporter
    /// goes stale. A `Vec` rather than a small-vector type: reflection support for those is
    /// feature-gated in Bevy, and a device rarely has more than two contributors.
    pub contributors:  Vec<ReporterId>,
    /// Capability component types the contributors declared for this device.
    pub declared:      HashSet<TypeId>,
    /// Capability component types whose values the contributors disagree about.
    ///
    /// Facts only; the values stay with the reporters that own them, because the erased capability
    /// payload is neither clonable nor reflectable and copying it would create a second
    /// authoritative record that can drift from the reporter's.
    pub disputed:      HashSet<TypeId>,
}

/// What the latest reconcile pass changed about the retained device set, held until the entity
/// projection has applied it.
///
/// A named record rather than a tuple of vectors, because a reader has to learn from the type that
/// these devices left, these entities have nothing behind them any more, and these devices' report
/// disagreements moved. `Devices::replace_reconciled` rebuilds its maps in place, so nothing else
/// can recover the previous generation once it returns.
///
/// It is a resource because the projection runs as its own system after `reconcile` — device
/// entities cannot be spawned from inside the merge, which holds borrows of every reporter's
/// retained set. The projection clears it once applied, so a settled frame that never reaches the
/// merge finds nothing left to re-apply.
#[derive(Debug, Default, Resource)]
pub(crate) struct ReconciledDeviceChanges {
    /// Devices that stopped being usable this pass, whether their key left the reported set or a
    /// reporter still names them while no longer reporting them present.
    pub(crate) departed:          Vec<DepartedDevice>,
    /// Entities that were mirroring a departed device and now need despawning.
    pub(crate) orphaned_entities: Vec<Entity>,
    /// Handles whose contributors changed what they disagree about, including a device whose first
    /// pass already found a disagreement.
    pub(crate) disputes_changed:  Vec<DeviceId>,
    /// Authored inventory keys whose connection conclusion this pass changed.
    ///
    /// Only the changed keys travel: `HardwareInventory` is written through mutable resource
    /// access, which marks the resource changed whether or not any value differs, so a pass that
    /// concluded nothing new must leave this empty rather than rewrite what is already there.
    pub(crate) connections:       Vec<ConfiguredDeviceConnectionChange>,
}

/// One device that left service this pass, and which of the two ways it left by.
///
/// The cause travels with the key because the two lead to different work: both make every
/// `crate::RecoveryPolicy::ReapplyOnReturn` role owe its restoration, but only a key that left the
/// reported set retires a handle, despawns an entity, and drops a `crate::ResolvedToDevice` link.
#[derive(Debug)]
pub(crate) struct DepartedDevice {
    pub(crate) key:       DeviceKey,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "restoration is owed for both causes; the retirement half reads \
                      `orphaned_entities`, which only `KeyLeftTheSet` fills"
        )
    )]
    pub(crate) departure: DeviceDeparture,
}

/// How one device stopped being usable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DeviceDeparture {
    /// No reporter named the key this pass, so its handle, state, and entity are retired.
    KeyLeftTheSet,
    /// A reporter still names the key but no longer reports the unit present, so everything keyed
    /// to it stays while the hardware itself is gone.
    RetainedButNotPresent,
}

/// One authored inventory key and the connection conclusion this pass reached for it.
#[derive(Debug)]
pub(crate) struct ConfiguredDeviceConnectionChange {
    pub(crate) key:        DeviceKey,
    pub(crate) connection: ConfiguredDeviceConnection,
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

/// Result of asking which entity mirrors one handle.
///
/// A named result rather than an optional entity, because the absent case means reconciliation has
/// not projected this device yet — the handle is live, its components are simply not queryable this
/// frame. A caller that read "no entity" as "no device" would drop a unit the kernel retains.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeviceEntityLookup {
    /// The handle names a device, but no entity currently mirrors it.
    NotProjected,
    /// This entity carries the device's mirrored identity, presence, and capability components.
    Projected(Entity),
}

/// Why the kernel refused to authorize an endpoint operation on one device.
///
/// Named for the refusal rather than for the caller, because both predicates return it and the
/// reader needs to know which check failed, not which function asked. Every variant carries the
/// durable key rather than the handle wherever one exists, so a refusal stays readable in a log
/// after the process that issued the handle exits.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ApplyAuthorizationError {
    /// The handle addresses no retained device, so there is nothing to authorize.
    #[error("device handle `{device_id:?}` addresses no retained device")]
    DeviceRetired {
        /// The handle that resolved to nothing.
        device_id: DeviceId,
    },
    /// The application authored this device offline, so no driver operation may touch it — not
    /// even a restore. Passive discovery still reports its presence.
    #[error("device `{key:?}` is configured offline")]
    Offline {
        /// Durable key of the authored entry whose mode refused the operation.
        key: DeviceKey,
    },
    /// The unit is not reachable right now, so an operation would address hardware that is absent
    /// or whose reachability the kernel cannot establish.
    #[error("device `{key:?}` is not present")]
    NotPresent {
        /// Durable key of the unreachable device.
        key: DeviceKey,
    },
    /// Another process owns the unit, or the platform blocked access to it, so an apply would fail
    /// at the driver.
    #[error("device `{key:?}` claim does not permit use by this process")]
    ClaimUnavailable {
        /// Durable key of the device whose claim refused the operation.
        key: DeviceKey,
    },
    /// The identity verdict does not authorize this operation: in-service use requires `Proven` or
    /// `Authored`, and a restore additionally accepts `RestoreOnly`.
    #[error("device `{key:?}` identity does not authorize this operation")]
    IdentityNotProven {
        /// Durable key of the device whose verdict refused the operation.
        key: DeviceKey,
    },
    /// The contributing reporters disagree about the capability the operation depends on, so the
    /// kernel cannot say which of their values would reach the hardware.
    #[error("reporters disagree about capability `{type_path}` on device `{key:?}`")]
    CapabilityDisputed {
        /// Durable key of the co-reported device whose contributors disagree.
        key:       DeviceKey,
        /// Rust type name of the contested capability component.
        type_path: String,
    },
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

/// In-flight endpoint attempts, keyed by the identifier this registry issued.
///
/// The registry is the only issuer of `AttemptId`, and its counter is monotonic and never reused,
/// so a driver poll that arrives after its attempt finished resolves to nothing rather than to a
/// later attempt for another role.
///
/// `next` starts at 1, not 0: `AttemptId` derives `Default`, so `AttemptId::default()` is
/// `AttemptId(0)`. An issued identifier equal to a defaulted or reflection-round-tripped one
/// would make "a late poll for a finished attempt resolves to nothing" unenforceable, because the
/// zero value appears wherever a field was left unset.
#[derive(Debug, Resource, Reflect)]
#[reflect(Resource)]
pub struct Attempts {
    in_flight: HashMap<AttemptId, Attempt>,
    next:      u64,
}

impl Default for Attempts {
    fn default() -> Self {
        Self {
            in_flight: HashMap::new(),
            next:      FIRST_ISSUED_ATTEMPT,
        }
    }
}

impl Attempts {
    /// Read the record the kernel retained for one issued identifier.
    #[must_use]
    pub fn in_flight(&self, attempt: AttemptId) -> AttemptLookup<'_> {
        self.in_flight
            .get(&attempt)
            .map_or(AttemptLookup::Finished, AttemptLookup::InFlight)
    }

    /// Advance the counter and hand back the identifier the next attempt record must carry.
    ///
    /// # Errors
    ///
    /// Returns `AttemptIssueError::SequenceExhausted` when the counter cannot advance without
    /// wrapping back to `AttemptId::default()` and reusing identifiers a late driver poll could
    /// still resolve.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "attempt identifiers are minted before a driver apply starts, and applies are \
                      not wired up yet"
        )
    )]
    pub(crate) fn issue(&mut self) -> Result<AttemptId, AttemptIssueError> {
        let next = self
            .next
            .checked_add(1)
            .ok_or(AttemptIssueError::SequenceExhausted)?;
        let attempt = AttemptId::new(self.next);
        self.next = next;

        Ok(attempt)
    }

    /// How many attempts are still in flight.
    #[must_use]
    pub fn len(&self) -> usize { self.in_flight.len() }

    /// Report whether no attempt is in flight.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.in_flight.is_empty() }

    /// Retain one attempt record until its driver reports a terminal outcome.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "started attempts are retained here for per-poll re-checks, and applies are \
                      not wired up yet"
        )
    )]
    pub(crate) fn retain(&mut self, attempt: Attempt) {
        self.in_flight.insert(attempt.id, attempt);
    }
}

/// Result of looking one issued identifier up in the attempt registry.
///
/// A named result rather than an optional record: "no record" is the terminal answer a late poll
/// must receive, and a caller that read it as "not ready yet" would keep polling a driver whose
/// attempt already ended.
#[derive(Clone, Copy, Debug)]
pub enum AttemptLookup<'a> {
    /// No record remains for this identifier, so the attempt it named has already ended.
    Finished,
    /// The kernel retains this attempt and the authorization it was started under.
    InFlight(&'a Attempt),
}

/// Failure from asking the attempt registry for another identifier.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub(crate) enum AttemptIssueError {
    /// The registry cannot issue again without reusing an identifier a driver may still poll.
    #[error("attempt identifier sequence is exhausted")]
    SequenceExhausted,
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
    use bevy::prelude::Component;
    use bevy::prelude::Reflect;
    use bevy::reflect::FromReflect;
    use bevy::reflect::tuple_struct::DynamicTupleStruct;

    use super::ApplyAuthorizationError;
    use super::AttemptLookup;
    use super::Attempts;
    use super::Device;
    use super::DeviceDeparture;
    use super::DeviceResolution;
    use super::DeviceStateLookup;
    use super::Devices;
    use super::PresentWithUsableClaim;
    use super::ReconciledDeviceState;
    use super::RiggingRevision;
    use crate::AttachmentPath;
    use crate::Attempt;
    use crate::AttemptId;
    use crate::Claim;
    use crate::ClaimHolder;
    use crate::ConfiguredDeviceMode;
    use crate::DeviceId;
    use crate::DeviceIdSource;
    use crate::DeviceKey;
    use crate::DeviceKind;
    use crate::IdentityDecisionOwed;
    use crate::IdentityVerdict;
    use crate::PermissionGate;
    use crate::Presence;
    use crate::ReportedId;
    use crate::ReportedParent;
    use crate::SchemeName;
    use crate::UnverifiedReason;

    /// Capability type the contributing reporters disagree about in the authorization tests.
    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct DisputedCapability;

    /// Capability type on the same device that every contributing reporter agrees about.
    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct AgreedCapability;

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
            verdict: IdentityVerdict::Proven,
            decision_owed: IdentityDecisionOwed::Nothing,
            mode: ConfiguredDeviceMode::Managed,
            attachment: AttachmentPath::PlatformHasNoConcept,
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

    #[test]
    fn both_ways_a_device_leaves_are_reported_and_stay_distinguishable()
    -> Result<(), Box<dyn Error>> {
        let unplugged = reported_key("UNPLUGGED")?;
        let removed = reported_key("REMOVED")?;
        let mut devices = Devices::default();
        devices.replace_reconciled(
            vec![reconciled(unplugged.clone()), reconciled(removed.clone())],
            HashSet::new(),
            HashSet::new(),
        );
        let unplugged_handle = handle(&devices, &unplugged);
        let mut absent = reconciled(unplugged.clone());
        absent.presence = Presence::Absent;

        let changes = devices.replace_reconciled(vec![absent], HashSet::new(), HashSet::new());

        let departures: Vec<(DeviceKey, DeviceDeparture)> = changes
            .departed
            .iter()
            .map(|departed_device| (departed_device.key.clone(), departed_device.departure))
            .collect();
        assert!(departures.contains(&(unplugged.clone(), DeviceDeparture::RetainedButNotPresent)));
        assert!(departures.contains(&(removed, DeviceDeparture::KeyLeftTheSet)));
        // The unit that stopped being present keeps everything keyed to it, so its roles stay bound
        // to the same handle while the hardware is away.
        assert_eq!(
            devices.resolve(&unplugged),
            DeviceResolution::Resolved(unplugged_handle)
        );
        assert!(changes.orphaned_entities.is_empty());

        Ok(())
    }

    // --- the authorization predicates ---

    /// Resolve one retained key to its handle, so an authorization test names the device it is
    /// asking about rather than the order the registry happened to issue handles in.
    fn handle(devices: &Devices, key: &DeviceKey) -> DeviceId {
        match devices.resolve(key) {
            DeviceResolution::Resolved(device_id) => device_id,
            DeviceResolution::NotResolved => panic!("key `{key:?}` must resolve after ingest"),
        }
    }

    #[test]
    fn a_proven_present_unclaimed_device_authorizes_both_service_and_restore()
    -> Result<(), Box<dyn Error>> {
        let key = reported_key("DELL-U2723QE-9J4K2H3")?;
        let mut devices = Devices::default();
        devices.replace_reconciled(
            vec![reconciled(key.clone())],
            HashSet::new(),
            HashSet::new(),
        );
        let device_id = handle(&devices, &key);

        assert!(
            devices
                .authorize_service(device_id)?
                .allows_in_service_use()
        );
        assert!(
            !devices
                .authorize_restore(device_id)?
                .allows_in_service_use()
        );

        Ok(())
    }

    #[test]
    fn every_refusal_names_the_check_that_actually_failed() -> Result<(), Box<dyn Error>> {
        let offline = reported_key("OFFLINE")?;
        let absent = reported_key("ABSENT")?;
        let contended = reported_key("CONTENDED")?;
        let blocked = reported_key("BLOCKED")?;
        let unverified = reported_key("UNVERIFIED")?;
        let mut offline_state = reconciled(offline.clone());
        offline_state.mode = ConfiguredDeviceMode::Offline;
        let mut absent_state = reconciled(absent.clone());
        absent_state.presence = Presence::Absent;
        let mut contended_state = reconciled(contended.clone());
        contended_state.claim = Claim::Contended {
            holder: ClaimHolder::Unidentified,
        };
        let mut blocked_state = reconciled(blocked.clone());
        blocked_state.claim = Claim::Blocked {
            gate: PermissionGate::CameraAccess,
        };
        let mut unverified_state = reconciled(unverified.clone());
        unverified_state.verdict = IdentityVerdict::Unverified(UnverifiedReason::NotUniqueInScan);
        let mut devices = Devices::default();
        devices.replace_reconciled(
            vec![
                offline_state,
                absent_state,
                contended_state,
                blocked_state,
                unverified_state,
            ],
            HashSet::new(),
            HashSet::new(),
        );

        let retired = DeviceId::new(u64::MAX);
        assert_eq!(
            devices.authorize_service(retired).err(),
            Some(ApplyAuthorizationError::DeviceRetired { device_id: retired })
        );
        for (key, expected) in [
            (
                &offline,
                ApplyAuthorizationError::Offline {
                    key: offline.clone(),
                },
            ),
            (
                &absent,
                ApplyAuthorizationError::NotPresent {
                    key: absent.clone(),
                },
            ),
            (
                &contended,
                ApplyAuthorizationError::ClaimUnavailable {
                    key: contended.clone(),
                },
            ),
            (
                &blocked,
                ApplyAuthorizationError::ClaimUnavailable {
                    key: blocked.clone(),
                },
            ),
            (
                &unverified,
                ApplyAuthorizationError::IdentityNotProven {
                    key: unverified.clone(),
                },
            ),
        ] {
            let device_id = handle(&devices, key);
            assert_eq!(
                devices.authorize_service(device_id).err(),
                Some(expected.clone())
            );
            assert_eq!(devices.authorize_restore(device_id).err(), Some(expected));
        }

        Ok(())
    }

    #[test]
    fn a_restore_only_verdict_returns_a_saved_configuration_and_drives_nothing()
    -> Result<(), Box<dyn Error>> {
        let key = reported_key("SERIAL-LESS-PANEL")?;
        let mut restore_only = reconciled(key.clone());
        restore_only.verdict = IdentityVerdict::RestoreOnly;
        let mut devices = Devices::default();
        devices.replace_reconciled(vec![restore_only], HashSet::new(), HashSet::new());
        let device_id = handle(&devices, &key);

        assert_eq!(
            devices.authorize_service(device_id).err(),
            Some(ApplyAuthorizationError::IdentityNotProven { key })
        );
        assert!(
            !devices
                .authorize_restore(device_id)?
                .allows_in_service_use()
        );

        Ok(())
    }

    #[test]
    fn an_offline_authored_entry_refuses_a_restore_as_well_as_service() -> Result<(), Box<dyn Error>>
    {
        let key = reported_key("WITHDRAWN")?;
        let mut offline = reconciled(key.clone());
        offline.mode = ConfiguredDeviceMode::Offline;
        let mut devices = Devices::default();
        devices.replace_reconciled(vec![offline], HashSet::new(), HashSet::new());
        let device_id = handle(&devices, &key);

        assert_eq!(
            devices.authorize_restore(device_id).err(),
            Some(ApplyAuthorizationError::Offline { key })
        );

        Ok(())
    }

    #[test]
    fn a_disputed_capability_blocks_only_operations_that_depend_on_it() -> Result<(), Box<dyn Error>>
    {
        let key = reported_key("STREAMDECK-XL-A00")?;
        let mut disputed_state = reconciled(key.clone());
        disputed_state.disputed = HashSet::from([TypeId::of::<DisputedCapability>()]);
        let mut devices = Devices::default();
        devices.replace_reconciled(vec![disputed_state], HashSet::new(), HashSet::new());
        let device_id = handle(&devices, &key);

        assert!(devices.authorize_service(device_id).is_ok());
        assert!(devices.authorize_restore(device_id).is_ok());
        assert!(
            devices
                .authorize_service_for::<AgreedCapability>(device_id)
                .is_ok()
        );
        assert!(matches!(
            devices.authorize_service_for::<DisputedCapability>(device_id).err(),
            Some(ApplyAuthorizationError::CapabilityDisputed { key: disputed, .. })
                if disputed == key
        ));

        Ok(())
    }

    // --- the attempt registry ---

    #[test]
    fn successive_issued_attempt_identifiers_differ_and_none_equals_the_default() {
        let mut attempts = Attempts::default();

        let first = attempts
            .issue()
            .expect("a fresh registry can issue an identifier");
        let second = attempts
            .issue()
            .expect("a fresh registry can issue a second identifier");

        assert_ne!(first, second);
        assert_ne!(first, AttemptId::default());
        assert_ne!(second, AttemptId::default());
    }

    #[test]
    fn a_retained_attempt_is_in_flight_while_an_unissued_identifier_is_finished() {
        let mut attempts = Attempts::default();
        let attempt = attempts
            .issue()
            .expect("a fresh registry can issue an identifier");
        attempts.retain(Attempt {
            id:                 attempt,
            role:               crate::RoleKey::new("window/main")
                .expect("`window/main` is a valid role handle"),
            endpoint:           crate::DeviceEndpoint {
                device: DeviceKey {
                    kind: crate::DeviceKind::Display,
                    id:   crate::DeviceIdSource::Authored {
                        value: crate::AuthoredId::new("panel")
                            .expect("`panel` is a valid authored identifier"),
                    },
                },
                id:     crate::EndpointId::Whole,
            },
            permit:             crate::ApplyPermit::in_service(),
            expected_device_id: DeviceId::new(0),
            rigging_revision:   RiggingRevision::default(),
            deadline:           bevy::platform::time::Instant::now(),
        });

        assert!(matches!(
            attempts.in_flight(attempt),
            AttemptLookup::InFlight(_)
        ));
        assert!(matches!(
            attempts.in_flight(AttemptId::default()),
            AttemptLookup::Finished
        ));
    }

    #[test]
    fn the_attempt_registry_registers_reflection_metadata() {
        let app = App::new();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let type_id = TypeId::of::<Attempts>();

        assert!(type_registry.contains(type_id));
        assert!(
            type_registry
                .get_type_data::<ReflectResource>(type_id)
                .is_some()
        );

        drop(type_registry);
    }
}
