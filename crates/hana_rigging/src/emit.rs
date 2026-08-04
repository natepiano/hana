//! Turning reconciled state changes into the events in `crate::events`.
//!
//! Everything here runs at the end of `crate::RiggingSystems::Reconcile`, after
//! `crate::reconcile::project_device_entities` has written this pass's conclusions onto entities.
//! The mirrors are written only when a value differs, so Bevy's own change detection is the
//! once-per-change gate: a settled frame leaves every mirror untouched and this stage emits
//! nothing.
//!
//! `crate::RoleState` is the one axis emitted elsewhere. `crate::apply` writes it a full system set
//! later and can move one role `Applying → Waiting → Applying` inside a single frame, which a
//! mirror-derived event would arrive too late to see and would collapse into one arrival.

use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::change_detection::Ref;
use bevy::ecs::observer::On;
use bevy::ecs::system::Commands;
use bevy::ecs::system::Query;
use bevy::ecs::system::Res;
use bevy::ecs::system::ResMut;
use bevy::prelude::Added;
use bevy::prelude::Entity;
use bevy::prelude::Resource;

use crate::Bindings;
use crate::Claim;
use crate::ClaimChanged;
use crate::ConfiguredDeviceConnectionChanged;
use crate::DeviceArrived;
use crate::DeviceDeparted;
use crate::DeviceKey;
use crate::DeviceResolution;
use crate::Devices;
use crate::DiscoveryFinished;
use crate::DiscoveryProgressChanged;
use crate::IdentityChanged;
use crate::IdentityVerdict;
use crate::Presence;
use crate::PresenceChanged;
use crate::ReapplyConfiguration;
use crate::RecoveryPolicy;
use crate::RecoveryPolicyChanged;
use crate::RetireRole;
use crate::RoleAvailable;
use crate::RoleAwaiting;
use crate::RoleKey;
use crate::SchemeName;
use crate::StartupDiscoveryChanged;
use crate::UnregisteredSchemeReported;
use crate::WaitingWork;
use crate::devices::DepartureAnnouncements;
use crate::discovery::DiscoveryTransition;
use crate::discovery::DiscoveryTransitionJournal;

/// Which schemes and role-availability edges have already reached a consumer.
///
/// Kept because neither fact is mirrored onto an entity, so neither has Bevy change detection
/// behind it. `crate::Devices::unregistered_schemes` is a retained set that keeps naming a rejected
/// scheme for the life of the process, and a role's availability is derived from a resource lookup
/// rather than from a component, so both would restate themselves on every frame without a record
/// of what was already said.
#[derive(Debug, Default, Resource)]
pub(crate) struct AnnouncedEdges {
    /// Schemes a `crate::UnregisteredSchemeReported` has already been emitted for.
    schemes:   Vec<SchemeName>,
    /// Roles currently reported as waiting for hardware, so the closing `crate::RoleAvailable`
    /// edge fires once and the opening `crate::RoleAwaiting` edge does not repeat while the
    /// wait lasts.
    awaiting:  Vec<RoleKey>,
    /// Roles already reported as resolved, so a role that never went absent is not re-announced
    /// every frame its device stays put.
    available: Vec<RoleKey>,
}

/// Emit one event per device-entity state axis that moved this pass.
///
/// `Ref::is_changed` is true on the tick a component is inserted as well as on a later write, so a
/// device's first reported reachability is an edge like any other and does not need a separate
/// first-value path.
pub(crate) fn announce_device_changes(
    mut commands: Commands,
    arrived: Query<(Entity, &DeviceKey), Added<DeviceKey>>,
    presences: Query<(Entity, Ref<'_, Presence>)>,
    claims: Query<(Entity, Ref<'_, Claim>)>,
    verdicts: Query<(Entity, Ref<'_, IdentityVerdict>)>,
) {
    for (device, key) in &arrived {
        commands.trigger(DeviceArrived {
            device,
            key: key.clone(),
        });
    }
    for (device, presence) in &presences {
        if presence.is_changed() {
            commands.trigger(PresenceChanged {
                device,
                presence: *presence,
            });
        }
    }
    for (device, claim) in &claims {
        if claim.is_changed() {
            commands.trigger(ClaimChanged {
                device,
                claim: (*claim).clone(),
            });
        }
    }
    for (device, verdict) in &verdicts {
        if verdict.is_changed() {
            commands.trigger(IdentityChanged {
                device,
                verdict: (*verdict).clone(),
            });
        }
    }
}

/// Emit one event per binding-entity state axis that moved this pass.
///
/// Only `crate::RecoveryPolicy` is mirror-derived here. `crate::RoleState` is announced from
/// `crate::RiggingSystems::Apply` for the reason this module's own documentation gives.
pub(crate) fn announce_binding_changes(
    mut commands: Commands,
    recoveries: Query<(Entity, &RoleKey, Ref<'_, RecoveryPolicy>)>,
) {
    for (binding, role, recovery) in &recoveries {
        if recovery.is_changed() {
            commands.trigger(RecoveryPolicyChanged {
                binding,
                role: role.clone(),
                recovery: *recovery,
            });
        }
    }
}

/// Emit the departure, connection, and rejected-scheme facts that have no mirrored component.
///
/// A departure that retired its key despawned the device entity before this runs, which is exactly
/// why `crate::DeviceDeparted` is global and carries the durable key: there is nothing left to
/// address or to read the key back from.
pub(crate) fn announce_reconciled_facts(
    mut commands: Commands,
    mut departure_announcements: ResMut<DepartureAnnouncements>,
    devices: Res<Devices>,
    mut announced_edges: ResMut<AnnouncedEdges>,
) {
    for departed_device in departure_announcements.departed.drain(..) {
        commands.trigger(DeviceDeparted {
            key:       departed_device.key,
            departure: departed_device.departure,
        });
    }
    for connection_change in departure_announcements.connections.drain(..) {
        commands.trigger(ConfiguredDeviceConnectionChanged {
            key:        connection_change.key,
            connection: connection_change.connection,
        });
    }
    for scheme in devices.unregistered_schemes() {
        if !announced_edges.schemes.contains(scheme) {
            announced_edges.schemes.push(scheme.clone());
            commands.trigger(UnregisteredSchemeReported {
                scheme: scheme.clone(),
            });
        }
    }
}

/// Emit the interval in which a registered role has no live device behind its endpoint.
///
/// Both events are global because during that interval there may be no device entity at all, and a
/// role registered this frame may not have had its binding entity spawned yet.
pub(crate) fn announce_role_availability(
    mut commands: Commands,
    bindings: Res<Bindings>,
    devices: Res<Devices>,
    mut announced_edges: ResMut<AnnouncedEdges>,
) {
    let mut awaiting = Vec::new();
    let mut available = Vec::new();
    for role in bindings.registered_roles() {
        let Ok(binding) = bindings.binding(role) else {
            continue;
        };
        match devices.resolve(&binding.endpoint.device) {
            DeviceResolution::Resolved(_) => available.push(role.clone()),
            DeviceResolution::NotResolved => awaiting.push(role.clone()),
        }
    }

    for role in &awaiting {
        if !announced_edges.awaiting.contains(role) {
            commands.trigger(RoleAwaiting { role: role.clone() });
        }
    }
    for role in &available {
        if !announced_edges.available.contains(role) {
            commands.trigger(RoleAvailable { role: role.clone() });
        }
    }
    announced_edges.awaiting = awaiting;
    announced_edges.available = available;
}

/// Retire a role because application code asked for it, rather than because a device left.
///
/// Global rather than entity-targeted so a role registered and retired inside one frame — which
/// never had a binding entity — can still be retired. A role that was never bound is not an error
/// here: the request and the outcome agree.
pub(crate) fn on_retire_role(retire_role: On<RetireRole>, mut bindings: ResMut<Bindings>) {
    drop(bindings.retire(&retire_role.role));
}

/// Answer an application's request to re-apply a role's saved configuration.
///
/// Honoured only for `crate::RecoveryPolicy::ReapplyOnRequest`. `crate::RecoveryPolicy::Retain`
/// promises the kernel remembers and reports but never touches the device, so honouring a request
/// would break that promise through the front door; `crate::RecoveryPolicy::Forget` dropped the
/// saved value at the departure and has nothing left to re-apply. Both leave the role's owed
/// application request exactly where it was.
pub(crate) fn on_reapply_configuration(
    reapply_configuration: On<ReapplyConfiguration>,
    roles: Query<&RoleKey>,
    mut bindings: ResMut<Bindings>,
) {
    let Ok(role) = roles.get(reapply_configuration.binding).cloned() else {
        return;
    };
    let Ok(binding) = bindings.binding(&role) else {
        return;
    };
    if binding.recovery != RecoveryPolicy::ReapplyOnRequest
        || bindings.waiting_work(&role) != WaitingWork::ApplicationRequestOwed
    {
        return;
    }
    bindings.request_reapply(&role);
}

/// Emit every discovery transition the scheduler recorded this frame, and empty the journal.
///
/// Ordered after the four systems above so a consumer that watches both sees this frame's device
/// conclusions before the discovery bookkeeping that produced them. Draining here is what keeps the
/// journal bounded — it is a record of one frame's transitions, never a history.
///
/// `crate::DiscoveryProgressChanged` carries the reporter's own report and its batch counts from
/// one recorded transition rather than from two reads of the retained status, so the per-reporter
/// and the aggregate view cannot disagree about the same batch.
pub(crate) fn announce_discovery_transitions(
    mut commands: Commands,
    mut discovery_transition_journal: ResMut<DiscoveryTransitionJournal>,
) {
    for discovery_transition in discovery_transition_journal.drain() {
        match discovery_transition {
            DiscoveryTransition::Progressed {
                batch,
                reporter,
                progress,
                completed,
                total,
                running,
                queued,
            } => {
                commands.trigger(DiscoveryProgressChanged {
                    batch,
                    reporter,
                    progress,
                    completed,
                    total,
                    running,
                    queued,
                });
            },
            DiscoveryTransition::Finished {
                batch,
                reporter,
                outcome,
            } => {
                commands.trigger(DiscoveryFinished {
                    batch,
                    reporter,
                    outcome,
                });
            },
            DiscoveryTransition::StartupChanged { startup } => {
                commands.trigger(StartupDiscoveryChanged { state: startup });
            },
        }
    }
}
