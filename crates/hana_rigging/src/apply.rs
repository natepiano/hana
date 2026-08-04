//! Attempt lifecycle: starting, re-validating, polling, bounding, retrying, and escalating one
//! role's endpoint apply.
//!
//! Every system here runs in `crate::RiggingSystems::Apply`, after reconciliation has published the
//! frame's device set, so an attempt is always validated against the current pass rather than the
//! one that authorized it.

use bevy::prelude::World;
use bevy::time::Real;
use bevy::time::Time;

use crate::ApplyPermit;
use crate::Attempt;
use crate::AttemptDeadlineStatus;
use crate::AttemptFinished;
use crate::AttemptId;
use crate::AttemptOutcome;
use crate::AttemptProgress;
use crate::Attempts;
use crate::BindingEntities;
use crate::BindingEntityLookup;
use crate::Bindings;
use crate::Claim;
use crate::DeviceEndpoint;
use crate::DeviceId;
use crate::DeviceResolution;
use crate::DeviceRevisionLookup;
use crate::DeviceStateLookup;
use crate::Devices;
use crate::HardwareInventory;
use crate::LastKnownGoodConfiguration;
use crate::OnAbort;
use crate::Presence;
use crate::RetiredRoleAttemptEnded;
use crate::RiggingLimits;
use crate::RoleAttemptLookup;
use crate::RoleKey;
use crate::RoleView;
use crate::WaitingRole;
use crate::WaitingWork;
use crate::binding::BindingTransition;
use crate::binding::BindingTransitionBatch;
use crate::binding::EndpointAvailability;
use crate::reconcile::FrameClockReading;
use crate::registration::Drivers;

/// Whether one in-flight attempt still holds the authorization it started under.
///
/// A named answer rather than an `Option<AttemptInvalidation>`: absence would have to be read as
/// "every re-check passed, so the driver may be polled", which is the decision this phase exists to
/// make, and a caller meeting `None` at the call site cannot tell it from "nothing was checked".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttemptValidity {
    /// Every re-check passed, so the driver may be polled again this frame.
    Holds,
    /// One re-check failed, and this is the reason the attempt is abandoned.
    Invalidated(AttemptInvalidation),
}

/// Why an in-flight attempt may not continue.
///
/// Named rather than a flag because the reasons finish the attempt differently: an abandoned
/// attempt is `AttemptOutcome::Aborted` and never auto-retries, while a role that no longer exists
/// has no binding left to move. Only `Self::RevisionAdvanced` consults `crate::OnAbort` — a lost
/// claim makes reversion impossible and a deferred veto makes it unsafe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttemptInvalidation {
    /// The device the attempt was authorized against is no longer the one the endpoint resolves to,
    /// so continuing would land an authorized configuration on a different unit.
    DeviceChanged,
    /// This device's own reconciled state moved under the attempt in a way no other check names —
    /// its endpoint set, what its reporters disagree about, where it hangs. Narrow by construction:
    /// every reason with a consequence of its own is a variant of its own, and this one is what is
    /// left. It is also the only reason that consults `crate::OnAbort`.
    RevisionAdvanced,
    /// The device's `crate::Claim` stopped permitting this process to use it, so whatever the
    /// attempt was doing has already stopped reaching the hardware.
    ClaimLost,
    /// The device's `crate::IdentityVerdict` no longer identifies the unit the attempt was
    /// authorized against, so continuing would drive hardware the kernel can no longer name.
    IdentityNoLongerConfirmed,
    /// The device stopped being `crate::Presence::Present`. Checked ahead of
    /// `Self::RevisionAdvanced` so a freshness lease expiring reads as a departure: the lease
    /// rewrites presence with no scan behind it, and calling that an ordinary state move would send
    /// whoever reads the ending looking for a change no reporter made.
    DeviceNotPresent,
    /// The device is still present, but the application switched its authored inventory entry to
    /// `crate::ConfiguredDeviceMode::Offline`, so no driver operation may touch it. Separate from
    /// `Self::DeviceNotPresent` because nothing about the hardware changed: reporting a withdrawal
    /// as a departure would send whoever reads the ending looking for a cable.
    InventoryWithdrewTheDevice,
    /// The attempt ran past `deadline + crate::RiggingLimits::apply_overrun`, so the kernel stopped
    /// asking a device that was never going to converge.
    OverrunExhausted,
    /// The application retired or replaced the role while its attempt was in flight.
    RoleGone,
}

/// End every attempt whose authorization no longer holds, before any driver poll this frame.
///
/// Ordered before `poll_attempts` so an attempt authorized against one unit can never land on
/// another: the re-check happens while the driver is still untouched. Every abort closes a retry
/// gate through `crate::Bindings::record_attempt_ending`, which is what makes the abort terminal:
/// the three apply systems are chained inside one set, so a role left ungated would be restarted
/// later in the same frame against the conditions that just invalidated it.
pub(crate) fn abort_invalidated_attempts(world: &mut World) {
    let invalidated = invalidated_attempts(world);
    if invalidated.is_empty() {
        return;
    }

    let now = FrameClockReading::from(world.resource::<Time<Real>>());
    let mut endings = Vec::with_capacity(invalidated.len());
    world.resource_scope::<Attempts, _>(|world, mut attempts| {
        world.resource_scope::<Bindings, _>(|world, mut bindings| {
            for (attempt, role, attempt_invalidation) in invalidated {
                if let Ok(RoleView::Applying(mut applying_role)) = bindings.role_view(&role) {
                    applying_role.abort();
                }
                let device_revision =
                    role_device_revision(world.resource::<Devices>(), &bindings, &role);
                bindings.record_attempt_ending(
                    &role,
                    AttemptOutcome::Aborted,
                    device_revision,
                    now,
                );
                apply_abort_policy(&mut bindings, &role, attempt_invalidation);
                attempts.end(attempt);
                endings.push((role, attempt));
            }
        });
    });

    // Triggered after both scopes close, so an observer may declare `Res<Attempts>` or
    // `Res<Bindings>`. A system parameter naming a resource the world does not currently hold is
    // skipped rather than reported, so an observer written against a taken resource would silently
    // never run.
    for (role, attempt) in endings {
        announce_attempt_ending(world, &role, attempt, AttemptOutcome::Aborted);
    }
}

/// Carry out the role's `crate::OnAbort` choice for the attempt the kernel just abandoned.
///
/// The kernel performs no I/O, so `crate::OnAbort::Revert` cannot itself drive the endpoint back.
/// What it does is record the debt: the role is owed a restoration of the value the last safe
/// capture read back, so the next dispatch mints a last-known-good restore instead of the
/// authored request, and the role's own driver performs it. With nothing established there is
/// nothing to revert to, and the role keeps whatever the abandoned apply left — which is
/// `crate::OnAbort::LeaveAsIs` by another route, and the only answer available.
fn apply_abort_policy(
    bindings: &mut Bindings,
    role: &RoleKey,
    attempt_invalidation: AttemptInvalidation,
) {
    if attempt_invalidation != AttemptInvalidation::RevisionAdvanced {
        return;
    }
    let reverts = bindings.binding(role).is_ok_and(|binding| {
        binding.on_abort == OnAbort::Revert
            && !matches!(
                binding.last_known_good,
                LastKnownGoodConfiguration::NotEstablished
            )
    });
    if reverts {
        bindings.set_waiting_work(role, WaitingWork::RestorationOwed);
    }
}

/// List the attempts this frame must end, while only shared borrows are held.
///
/// Selecting first keeps a settled frame silent: `World::resource_scope` marks a resource changed
/// on reinsertion whether or not the closure wrote anything, so a frame with no invalidated attempt
/// must never enter one.
fn invalidated_attempts(world: &World) -> Vec<(AttemptId, RoleKey, AttemptInvalidation)> {
    let attempts = world.resource::<Attempts>();
    let devices = world.resource::<Devices>();
    let bindings = world.resource::<Bindings>();
    let hardware_inventory = world.resource::<HardwareInventory>();
    let rigging_limits = world.resource::<RiggingLimits>();
    let now = FrameClockReading::from(world.resource::<Time<Real>>());
    let mut invalidated: Vec<(AttemptId, RoleKey, AttemptInvalidation)> = Vec::new();

    // Retirement and replacement are asked role first, through the registry's role-keyed lookup:
    // both destroy the `RoleState::Applying` that named the attempt, and a replacement leaves a
    // fresh binding standing under the same role, so nothing the attempt itself carries would say
    // the role it ran for is gone.
    for role in retired_or_replaced_roles(world) {
        if let RoleAttemptLookup::InFlight(attempt) = attempts.in_flight_for(&role, bindings)
            && !already_selected(&invalidated, attempt)
        {
            invalidated.push((attempt, role, AttemptInvalidation::RoleGone));
        }
    }

    for attempt in attempts.in_flight_attempts() {
        if already_selected(&invalidated, attempt.id) {
            continue;
        }
        let AttemptValidity::Invalidated(attempt_invalidation) = attempt_validity(
            attempt,
            bindings,
            devices,
            hardware_inventory,
            attempts.deadline_status(attempt.id, now, rigging_limits.apply_overrun),
        ) else {
            continue;
        };
        invalidated.push((attempt.id, attempt.role.clone(), attempt_invalidation));
    }

    invalidated
}

/// Whether this frame's selection already names `attempt`.
///
/// Two binding transitions can name one role in a frame — a replacement followed by a retirement —
/// and the role-keyed lookup answers with the same in-flight attempt each time; the second pass
/// then walks that attempt again. `AttemptFinished` fires once per attempt, so a repeat selection
/// is dropped rather than ended twice.
fn already_selected(
    invalidated: &[(AttemptId, RoleKey, AttemptInvalidation)],
    attempt: AttemptId,
) -> bool {
    invalidated
        .iter()
        .any(|(selected, ..)| *selected == attempt)
}

/// Decide whether one in-flight attempt still holds the authorization it started under.
fn attempt_validity(
    attempt: &Attempt,
    bindings: &Bindings,
    devices: &Devices,
    hardware_inventory: &HardwareInventory,
    deadline_status: AttemptDeadlineStatus,
) -> AttemptValidity {
    if bindings.binding(&attempt.role).is_err() {
        return AttemptValidity::Invalidated(AttemptInvalidation::RoleGone);
    }
    if devices.resolve(&attempt.endpoint.device)
        != DeviceResolution::Resolved(attempt.expected_device_id)
    {
        return AttemptValidity::Invalidated(AttemptInvalidation::DeviceChanged);
    }
    // Claim, verdict, and presence are read through `Devices`, never through the device entity's
    // mirrored components: the entity can be despawned while an attempt is still finishing, which
    // is exactly when these checks matter most. Presence variants are compared, never values,
    // because `Presence::Unreachable` carries an elapsed time that grows on every scan.
    match devices.state(attempt.expected_device_id) {
        DeviceStateLookup::Retired => {
            return AttemptValidity::Invalidated(AttemptInvalidation::DeviceNotPresent);
        },
        DeviceStateLookup::Retained(reconciled_device_state) => {
            match reconciled_device_state.claim {
                Claim::Held | Claim::Free | Claim::NotApplicable => {},
                Claim::Contended { .. } | Claim::Blocked { .. } => {
                    return AttemptValidity::Invalidated(AttemptInvalidation::ClaimLost);
                },
            }
            if !reconciled_device_state.verdict.identified() {
                return AttemptValidity::Invalidated(
                    AttemptInvalidation::IdentityNoLongerConfirmed,
                );
            }
            if !reconciled_device_state
                .presence
                .is_same_variant(Presence::Present)
            {
                return AttemptValidity::Invalidated(AttemptInvalidation::DeviceNotPresent);
            }
        },
    }
    if hardware_inventory
        .ensure_operational(&attempt.endpoint.device)
        .is_err()
    {
        return AttemptValidity::Invalidated(AttemptInvalidation::InventoryWithdrewTheDevice);
    }
    // Last of the state checks, because its meaning is "moved in a way none of the others names".
    // A device the kernel no longer retains has no revision at all, which the checks above have
    // already ended the attempt for.
    if devices.revision(attempt.expected_device_id)
        != DeviceRevisionLookup::Retained(attempt.device_revision)
    {
        return AttemptValidity::Invalidated(AttemptInvalidation::RevisionAdvanced);
    }
    match deadline_status {
        AttemptDeadlineStatus::OverrunExhausted { .. } => {
            AttemptValidity::Invalidated(AttemptInvalidation::OverrunExhausted)
        },
        AttemptDeadlineStatus::NoSuchAttempt
        | AttemptDeadlineStatus::WithinDeadline
        | AttemptDeadlineStatus::OverdueWithinOverrun { .. } => AttemptValidity::Holds,
    }
}

/// Report one terminal attempt outcome to whichever observer can still receive it.
///
/// A role that still projects a binding entity gets the entity-targeted `crate::AttemptFinished`.
/// A role whose entity was despawned this frame gets the global `crate::RetiredRoleAttemptEnded`
/// instead: an event addressed to a despawned entity reaches nobody, and a retirement is exactly
/// the ending a diagnostic most needs to see.
fn announce_attempt_ending(
    world: &mut World,
    role: &RoleKey,
    attempt: AttemptId,
    outcome: AttemptOutcome,
) {
    match world.resource::<BindingEntities>().entity(role) {
        BindingEntityLookup::Registered(binding) => {
            world.trigger(AttemptFinished {
                binding,
                role: role.clone(),
                attempt,
                outcome,
            });
        },
        BindingEntityLookup::Unregistered => {
            world.trigger(RetiredRoleAttemptEnded {
                role: role.clone(),
                attempt,
                outcome,
            });
        },
    }
}

/// Read this frame's accepted lifecycle changes for roles whose binding no longer holds an attempt.
///
/// `crate::BindingTransitionBatch` is still readable throughout `crate::RiggingSystems::Apply`; a
/// separate system ordered after the set empties it.
fn retired_or_replaced_roles(world: &World) -> Vec<RoleKey> {
    world
        .resource::<BindingTransitionBatch>()
        .transitions()
        .iter()
        .filter_map(|binding_transition| match binding_transition {
            BindingTransition::Replaced { role, .. } | BindingTransition::Retired { role, .. } => {
                Some(role.clone())
            },
            BindingTransition::Registered { .. } => None,
        })
        .collect()
}

/// Poll every attempt that survived re-validation and finish the ones a driver reports terminal.
pub(crate) fn poll_attempts(world: &mut World) {
    let polled: Vec<AttemptId> = world
        .resource::<Attempts>()
        .in_flight_attempts()
        .map(|attempt| attempt.id)
        .collect();
    if polled.is_empty() {
        return;
    }

    let now = FrameClockReading::from(world.resource::<Time<Real>>());
    let mut endings = Vec::new();
    world.resource_scope::<Attempts, _>(|world, mut attempts| {
        world.resource_scope::<Bindings, _>(|world, mut bindings| {
            world.resource_scope::<Drivers, _>(|world, mut drivers| {
                world.resource_scope::<HardwareInventory, _>(|world, hardware_inventory| {
                    for attempt in polled {
                        let crate::AttemptLookup::InFlight(retained) = attempts.in_flight(attempt)
                        else {
                            continue;
                        };
                        let role = retained.role.clone();
                        let endpoint = retained.endpoint.clone();
                        let attempt_progress = {
                            let Ok(RoleView::Applying(applying_role)) = bindings.role_view(&role)
                            else {
                                continue;
                            };
                            let Ok(poll_request) = applying_role.poll_request(&hardware_inventory)
                            else {
                                continue;
                            };
                            // The request carries the role and endpoint the binding currently
                            // holds; the attempt carries the ones it
                            // was authorized against. A driver is
                            // only asked to continue when the two still name the same work.
                            if poll_request.role != &role || poll_request.endpoint != &endpoint {
                                continue;
                            }
                            match drivers.poll(world, poll_request) {
                                Ok(attempt_progress) => attempt_progress,
                                Err(_) => AttemptProgress::Finished(AttemptOutcome::Aborted),
                            }
                        };
                        let AttemptProgress::Finished(attempt_outcome) = attempt_progress else {
                            continue;
                        };
                        if let Ok(RoleView::Applying(mut applying_role)) = bindings.role_view(&role)
                        {
                            applying_role.finish(attempt_outcome.clone());
                        }
                        let device_revision =
                            role_device_revision(world.resource::<Devices>(), &bindings, &role);
                        bindings.record_attempt_ending(
                            &role,
                            attempt_outcome.clone(),
                            device_revision,
                            now,
                        );
                        attempts.end(attempt);
                        endings.push((role, attempt, attempt_outcome));
                    }
                });
            });
        });
    });

    // Triggered after every scope closes, for the same reason the abort path defers its endings: an
    // observer declaring a resource the world does not currently hold is skipped, not reported.
    for (role, attempt, attempt_outcome) in endings {
        announce_attempt_ending(world, &role, attempt, attempt_outcome);
    }
}

/// Start one authorized apply for every waiting role whose retry pacing has opened.
///
/// Erased driver dispatch runs first and only a successful dispatch commits the binding to
/// `crate::RoleState::Applying`, so a missing driver or a configuration-contract failure leaves the
/// role `crate::RoleState::Waiting` with no in-flight attempt.
pub(crate) fn start_authorized_applies(world: &mut World) {
    rearm_reacquired_roles(world);

    let startable = startable_roles(world);
    if startable.is_empty() {
        return;
    }

    world.resource_scope::<Attempts, _>(|world, mut attempts| {
        world.resource_scope::<Bindings, _>(|world, mut bindings| {
            world.resource_scope::<Drivers, _>(|world, mut drivers| {
                world.resource_scope::<Devices, _>(|world, devices| {
                    world.resource_scope::<HardwareInventory, _>(|world, hardware_inventory| {
                        for role in startable {
                            start_one_apply(
                                world,
                                &mut attempts,
                                &mut bindings,
                                &mut drivers,
                                &devices,
                                &hardware_inventory,
                                &role,
                            );
                        }
                    });
                });
            });
        });
    });
}

/// Return every role the kernel stopped after repeated failures whose device has since returned.
///
/// The second way out of `crate::RoleState::StoppedAfterRepeatedFailures`, beside the explicit
/// `crate::Bindings::restart_after_repeated_failures`: a role gets one more attempt once the unit
/// it kept failing against has actually left and come back, and a success on that attempt clears
/// the run. Watching for the departure first is what keeps a device that never leaves from being
/// retried — that role stays stopped until the application restarts it.
fn rearm_reacquired_roles(world: &mut World) {
    let stopped = stopped_role_endpoints(world);
    if stopped.is_empty() {
        return;
    }

    world.resource_scope::<Bindings, _>(|_world, mut bindings| {
        for (role, endpoint_availability) in stopped {
            bindings.observe_stopped_role_endpoint(&role, endpoint_availability);
        }
    });
}

/// Read how every stopped role's endpoint resolves this frame, while only shared borrows are held.
fn stopped_role_endpoints(world: &World) -> Vec<(RoleKey, EndpointAvailability)> {
    let bindings = world.resource::<Bindings>();
    let devices = world.resource::<Devices>();

    bindings
        .registered_roles()
        .filter_map(|role| {
            let binding = bindings.binding(role).ok()?;
            if binding.state != crate::RoleState::StoppedAfterRepeatedFailures {
                return None;
            }

            Some((
                role.clone(),
                endpoint_availability(devices, &binding.endpoint.device),
            ))
        })
        .collect()
}

/// Report whether one durable endpoint currently names a device the kernel could drive.
fn endpoint_availability(devices: &Devices, device_key: &crate::DeviceKey) -> EndpointAvailability {
    let DeviceResolution::Resolved(device_id) = devices.resolve(device_key) else {
        return EndpointAvailability::Gone;
    };
    match devices.state(device_id) {
        DeviceStateLookup::Retired => EndpointAvailability::Gone,
        DeviceStateLookup::Retained(reconciled_device_state) => {
            if reconciled_device_state
                .presence
                .is_same_variant(Presence::Present)
            {
                EndpointAvailability::Available
            } else {
                EndpointAvailability::Gone
            }
        },
    }
}

/// Read how many times one role's device has changed, in the form the retry gate stamps and reads.
///
/// A role whose endpoint resolves to nothing answers `DeviceRevisionLookup::Retired`, which is what
/// lets a gate stamped while the device was gone reopen when the device returns: reacquisition
/// issues a fresh handle whose counter restarts, so the two readings differ even though the numbers
/// would not.
fn role_device_revision(
    devices: &Devices,
    bindings: &Bindings,
    role: &RoleKey,
) -> DeviceRevisionLookup {
    let Ok(binding) = bindings.binding(role) else {
        return DeviceRevisionLookup::Retired;
    };
    match devices.resolve(&binding.endpoint.device) {
        DeviceResolution::NotResolved => DeviceRevisionLookup::Retired,
        DeviceResolution::Resolved(device_id) => devices.revision(device_id),
    }
}

/// List the roles whose waiting state and retry pacing both permit a dispatch this frame.
fn startable_roles(world: &World) -> Vec<RoleKey> {
    let bindings = world.resource::<Bindings>();
    let devices = world.resource::<Devices>();
    let now = FrameClockReading::from(world.resource::<Time<Real>>());

    bindings
        .registered_roles()
        .filter(|role| {
            bindings
                .retry_pacing(role)
                .permits_dispatch(role_device_revision(devices, bindings, role), now)
                && bindings.waiting_work(role) != crate::WaitingWork::ApplicationRequestOwed
                && bindings.binding(role).is_ok_and(|binding| {
                    binding.state == crate::RoleState::Waiting
                        && matches!(
                            devices.resolve(&binding.endpoint.device),
                            DeviceResolution::Resolved(_)
                        )
                })
        })
        .cloned()
        .collect()
}

/// How far one waiting role got toward handing an apply to its driver.
///
/// Owned rather than borrowed, so the lifecycle view that mints the request is dropped before the
/// caller records what happened: the refusal paths have to touch `crate::Bindings` again, and the
/// request borrows it.
enum ApplyDispatch {
    /// The driver accepted the start under this permit, for this endpoint.
    Started {
        /// Endpoint the minted request named, retained on the attempt so a later poll can be
        /// compared against it rather than against whatever the driver believes it opened.
        endpoint: DeviceEndpoint,
        /// Authorization the request was minted under, stored once and handed to the driver, so no
        /// second copy can disagree.
        permit:   ApplyPermit,
    },
    /// The device is not authorized for this kind of apply right now. Not paced: the condition is
    /// device state that reconciliation re-answers every frame, and no driver was asked anything.
    NotAuthorized,
    /// Nothing reached a driver: the lifecycle view refused to mint a request, or erased dispatch
    /// failed on an unregistered driver or a configuration-contract mismatch.
    Refused,
}

/// Authorize, mint, and dispatch one apply for a single waiting role.
///
/// The identifier is minted as late as the request contract allows — after the binding, the
/// resolution, and the clock — and handed back on every path that does not commit, so a role whose
/// driver is unregistered cannot spend the registry's non-reusable sequence one identifier per
/// frame. A refusal that reached a driver also closes a retry gate, because the role stays
/// `crate::RoleState::Waiting` and would otherwise be re-dispatched on the next frame, forever.
fn start_one_apply(
    world: &mut World,
    attempts: &mut Attempts,
    bindings: &mut Bindings,
    drivers: &mut Drivers,
    devices: &Devices,
    hardware_inventory: &HardwareInventory,
    role: &RoleKey,
) {
    let Ok(binding) = bindings.binding(role) else {
        return;
    };
    let DeviceResolution::Resolved(device_id) = devices.resolve(&binding.endpoint.device) else {
        return;
    };
    // The two maps are written by one reconcile pass, so a handle that resolves always has a
    // revision; the attempt is stamped with it rather than with a stand-in, because a stand-in
    // would be indistinguishable from a device that never changed.
    let DeviceRevisionLookup::Retained(device_revision) = devices.revision(device_id) else {
        return;
    };
    let now = FrameClockReading::from(world.resource::<Time<Real>>());
    let FrameClockReading::Measurable(measured) = now else {
        return;
    };
    let deadline = measured + world.resource::<RiggingLimits>().apply_deadline;
    let Ok(attempt) = attempts.issue() else {
        return;
    };

    match dispatch_one_apply(
        world,
        bindings,
        drivers,
        devices,
        hardware_inventory,
        role,
        device_id,
        attempt,
    ) {
        ApplyDispatch::Started { endpoint, permit } => {
            attempts.begin(Attempt {
                id: attempt,
                role: role.clone(),
                endpoint,
                permit,
                expected_device_id: device_id,
                device_revision,
                deadline,
            });
        },
        ApplyDispatch::NotAuthorized => attempts.release(attempt),
        ApplyDispatch::Refused => {
            attempts.release(attempt);
            bindings.record_dispatch_refused(
                role,
                DeviceRevisionLookup::Retained(device_revision),
                now,
            );
        },
    }
}

/// Ask one waiting role's lifecycle view for a request and hand it to erased driver dispatch.
///
/// The stored `crate::WaitingWork` picks the configuration and the resulting view then demands the
/// matching permit; permission never selects the configuration.
fn dispatch_one_apply(
    world: &mut World,
    bindings: &mut Bindings,
    drivers: &mut Drivers,
    devices: &Devices,
    hardware_inventory: &HardwareInventory,
    role: &RoleKey,
    device_id: DeviceId,
    attempt: AttemptId,
) -> ApplyDispatch {
    let Ok(RoleView::Waiting(waiting_role)) = bindings.role_view(role) else {
        return ApplyDispatch::NotAuthorized;
    };
    let (start_apply_request, permit) = match waiting_role {
        // The role's recovery policy refused an automatic reapply after its device departed. No
        // driver is asked anything, so this is not paced and counts as no failure: the state clears
        // when application code acts.
        WaitingRole::ForApplication => return ApplyDispatch::NotAuthorized,
        WaitingRole::ForHardware(requesting_role) => {
            // A `RestoreOnly` device has nothing to restore until a safe readback establishes one,
            // so its first apply runs from authored intent under the weaker permit. `RestoreOnly`
            // means the kernel only ever applies what the application asked for, never a
            // configuration of its own choosing.
            let Ok(permit) = devices
                .authorize_service(device_id)
                .or_else(|_| devices.authorize_restore(device_id))
            else {
                return ApplyDispatch::NotAuthorized;
            };
            match requesting_role.start_requested_apply(attempt, permit, hardware_inventory) {
                Ok(start_apply_request) => (start_apply_request, permit),
                Err(_) => return ApplyDispatch::Refused,
            }
        },
        WaitingRole::ForRestoration(restoring_role) => {
            let Ok(permit) = devices.authorize_restore(device_id) else {
                return ApplyDispatch::NotAuthorized;
            };
            match restoring_role.start_last_known_good_restore(attempt, permit, hardware_inventory)
            {
                Ok(start_apply_request) => (start_apply_request, permit),
                Err(_) => return ApplyDispatch::Refused,
            }
        },
    };
    let endpoint = start_apply_request.binding.endpoint.clone();
    if drivers.start_apply(world, start_apply_request).is_err() {
        return ApplyDispatch::Refused;
    }

    ApplyDispatch::Started { endpoint, permit }
}

/// Empty the frame's accepted lifecycle changes once every reader has run.
///
/// Ordered after `crate::RiggingSystems::Apply` rather than living in the event stage: the real
/// constraint is that nobody clears until every reader has run, and this phase's aborts read the
/// batch a full set later than the events that report the same transitions.
pub(crate) fn clear_binding_transitions(world: &mut World) {
    let batch = world.resource::<BindingTransitionBatch>();
    if batch.transitions().is_empty() {
        return;
    }
    world.resource_mut::<BindingTransitionBatch>().clear();
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::collections::HashSet;
    use std::error::Error;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;

    use bevy::MinimalPlugins;
    use bevy::app::App;
    use bevy::ecs::reflect::ReflectComponent;
    use bevy::prelude::Component;
    use bevy::prelude::On;
    use bevy::prelude::Reflect;
    use bevy::prelude::ResMut;
    use bevy::prelude::Resource;
    use bevy::prelude::World;

    use crate::ApplyPermit;
    use crate::AttachmentPath;
    use crate::AttemptFinished;
    use crate::AttemptId;
    use crate::AttemptOutcome;
    use crate::AttemptProgress;
    use crate::AvailableConfiguration;
    use crate::Binding;
    use crate::Bindings;
    use crate::Capabilities;
    use crate::CaptureOutcome;
    use crate::Claim;
    use crate::ClaimHolder;
    use crate::ConfiguredDevice;
    use crate::ConfiguredDeviceMode;
    use crate::DeviceDescriptor;
    use crate::DeviceEndpoint;
    use crate::DeviceIdSource;
    use crate::DeviceKey;
    use crate::DeviceKind;
    use crate::DeviceRecord;
    use crate::DeviceReporter;
    use crate::DeviceScan;
    use crate::Devices;
    use crate::DiscoveryCadence;
    use crate::DiscoveryControl;
    use crate::DiscoveryWork;
    use crate::DriverId;
    use crate::EndpointDriver;
    use crate::EndpointId;
    use crate::HardwareInventory;
    use crate::IdentityVerdict;
    use crate::LastKnownGoodConfiguration;
    use crate::MainThreadDiscoveryJob;
    use crate::OnAbort;
    use crate::OnSessionLoss;
    use crate::OsDeviceId;
    use crate::Presence;
    use crate::ReconciledDeviceState;
    use crate::RecoveryPolicy;
    use crate::ReportedAs;
    use crate::ReportedId;
    use crate::ReportedParent;
    use crate::ReportedSerial;
    use crate::ReporterCoverage;
    use crate::ReporterId;
    use crate::ReporterRegistration;
    use crate::RequestedConfiguration;
    use crate::RetiredRoleAttemptEnded;
    use crate::RetryOn;
    use crate::RiggingAppExt;
    use crate::RiggingLimits;
    use crate::RiggingPlugin;
    use crate::RiggingRevision;
    use crate::RoleAttemptLookup;
    use crate::RoleKey;
    use crate::RoleState;
    use crate::SchemeName;
    use crate::UnverifiedReason;
    use crate::WaitingWork;

    const SECOND_ROLE: &str = "secondary-window";
    const SECOND_UNIT: &str = "UNIT-0002";
    const TEST_ROLE: &str = "primary-window";
    const TEST_SCHEME: &str = "test-scheme";
    const TEST_UNIT: &str = "UNIT-0001";
    /// Frames one test may spend waiting for a reported set to reach the binding.
    const FRAME_CEILING: u32 = 32;
    /// Frames one requested re-scan needs to be collected, reconciled, and acted on.
    const RESCAN_FRAMES: u32 = 4;

    /// Configuration the test role asks its driver for.
    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct TestConfiguration(u32);

    /// What every test driver did, and what its next poll answers.
    ///
    /// Held in the world rather than in the driver value, because the driver itself lives inside
    /// the kernel's registry and a test never sees it again after registration.
    #[derive(Resource)]
    struct DriverLog {
        started:  Vec<AttemptId>,
        polled:   Vec<AttemptId>,
        progress: AttemptProgress,
    }

    impl Default for DriverLog {
        fn default() -> Self {
            Self {
                started:  Vec::new(),
                polled:   Vec::new(),
                progress: AttemptProgress::Pending,
            }
        }
    }

    struct TestDriver;

    impl EndpointDriver for TestDriver {
        type Configuration = TestConfiguration;

        fn capture(
            &mut self,
            _: &mut World,
            _: &DeviceEndpoint,
        ) -> CaptureOutcome<Self::Configuration> {
            CaptureOutcome::NotReadable
        }

        fn start_apply(
            &mut self,
            world: &mut World,
            _: &DeviceEndpoint,
            _: &Self::Configuration,
            attempt: AttemptId,
            _: ApplyPermit,
        ) {
            world.resource_mut::<DriverLog>().started.push(attempt);
        }

        fn poll(&mut self, world: &mut World, attempt: AttemptId) -> AttemptProgress {
            let mut driver_log = world.resource_mut::<DriverLog>();
            driver_log.polled.push(attempt);
            driver_log.progress.clone()
        }
    }

    /// Reports whatever set the shared list currently holds, on demand only.
    ///
    /// On demand rather than periodic so a scan lands at a frame the test names: a reporter that
    /// submits a set every frame would leave when its evidence reaches a role up to the frame rate,
    /// and a test asserting what one scan did could not say which scan it saw.
    struct ListReporter(Arc<Mutex<Vec<DeviceKey>>>);

    impl DeviceReporter for ListReporter {
        fn discover(&mut self) -> DiscoveryWork {
            let reported_keys = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            DiscoveryWork::Immediate(MainThreadDiscoveryJob::new(move |_: &mut World| {
                DeviceScan::Complete(reported_keys.iter().cloned().map(present).collect())
            }))
        }
    }

    /// Every attempt ending an observer saw, split by whether a binding entity still existed.
    #[derive(Default, Resource)]
    struct ObservedEndings {
        on_binding_entity: Vec<(RoleKey, AttemptOutcome)>,
        after_retirement:  Vec<(RoleKey, AttemptOutcome)>,
    }

    fn observe_attempt_finished(
        attempt_finished: On<AttemptFinished>,
        mut observed_endings: ResMut<ObservedEndings>,
    ) {
        observed_endings.on_binding_entity.push((
            attempt_finished.role.clone(),
            attempt_finished.outcome.clone(),
        ));
    }

    fn observe_retired_role_attempt_ended(
        retired_role_attempt_ended: On<RetiredRoleAttemptEnded>,
        mut observed_endings: ResMut<ObservedEndings>,
    ) {
        observed_endings.after_retirement.push((
            retired_role_attempt_ended.role.clone(),
            retired_role_attempt_ended.outcome.clone(),
        ));
    }

    /// One app whose role is bound to a reported unit, driven until that unit resolves.
    struct ApplyHarness {
        app:           App,
        role:          RoleKey,
        reporter:      ReporterId,
        reported_keys: Arc<Mutex<Vec<DeviceKey>>>,
    }

    impl ApplyHarness {
        fn new(driver: DriverId) -> Result<Self, Box<dyn Error>> {
            Self::with_cadence(driver, DiscoveryCadence::OnDemand)
        }

        /// Build a harness whose reporter declares `cadence`.
        ///
        /// The cadence is what decides whether the reporter's evidence can ever go stale: an
        /// on-demand reporter promised nothing and so is never late, while a reporter that declared
        /// an interval has a freshness lease the test can run past.
        fn with_cadence(
            driver: DriverId,
            cadence: DiscoveryCadence,
        ) -> Result<Self, Box<dyn Error>> {
            let reported_keys = Arc::new(Mutex::new(vec![unit_key(TEST_UNIT)?]));
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(RiggingPlugin)
                .register_device_scheme(SchemeName::new(TEST_SCHEME)?)
                .init_resource::<DriverLog>()
                .init_resource::<ObservedEndings>()
                .add_observer(observe_attempt_finished)
                .add_observer(observe_retired_role_attempt_ended);
            let registered = app.add_endpoint_driver(TestDriver);
            assert_eq!(registered, DriverId(0));
            let reporter = app.add_device_reporter(
                ListReporter(Arc::clone(&reported_keys)),
                ReporterRegistration::required(cadence, ReporterCoverage::MatchingEvidenceOnly),
            );
            let role = RoleKey::new(TEST_ROLE)?;
            app.world_mut()
                .resource_mut::<Bindings>()
                .register(test_binding(role.clone(), unit_key(TEST_UNIT)?, driver))?;

            Ok(Self {
                app,
                role,
                reporter,
                reported_keys,
            })
        }

        /// Build a harness whose role already has one attempt in flight.
        fn started(driver: DriverId) -> Result<Self, Box<dyn Error>> {
            let mut harness = Self::new(driver)?;
            harness.settle();

            Ok(harness)
        }

        /// Build a started harness whose role reverts on abort and has a value to go back to.
        ///
        /// Both halves are needed to tell the abort reasons apart: `OnAbort::Revert` alone
        /// records nothing while `LastKnownGoodConfiguration::NotEstablished` says there is nothing
        /// to restore, so a test that omitted either would pass for the wrong reason.
        fn started_reverting(driver: DriverId) -> Result<Self, Box<dyn Error>> {
            let mut harness = Self::new(driver)?;
            let mut reverting = test_binding(harness.role.clone(), unit_key(TEST_UNIT)?, driver);
            reverting.on_abort = OnAbort::Revert;
            reverting.last_known_good = LastKnownGoodConfiguration::known(TestConfiguration(9));
            let _replaced = harness
                .app
                .world_mut()
                .resource_mut::<Bindings>()
                .replace(reverting)?;
            harness.settle();

            Ok(harness)
        }

        /// Bind a second role to a second unit, so a scan can change one device and leave the
        /// other one reported exactly as it was.
        fn bind_second_role(&mut self, driver: DriverId) -> Result<RoleKey, Box<dyn Error>> {
            let second_role = RoleKey::new(SECOND_ROLE)?;
            self.app
                .world_mut()
                .resource_mut::<Bindings>()
                .register(test_binding(
                    second_role.clone(),
                    unit_key(SECOND_UNIT)?,
                    driver,
                ))?;

            Ok(second_role)
        }

        /// The reconciled state the kernel currently holds for the harness's unit.
        fn reconciled_device_state(&self) -> ReconciledDeviceState {
            self.app
                .world()
                .resource::<Devices>()
                .states()
                .next()
                .expect("the harness settles once its reported unit has been reconciled")
                .clone()
        }

        /// Write one reconciled state straight into `Devices`, as a reconcile pass would.
        ///
        /// Written directly rather than reported, because the kernel computes the identity verdict
        /// and the merged claim itself: a test that had to talk a reconcile pass into producing a
        /// particular one would be exercising the verdict rules instead of the abort rules.
        fn reconcile_device(&mut self, reconciled_device_state: ReconciledDeviceState) {
            self.app
                .world_mut()
                .resource_mut::<Devices>()
                .replace_reconciled(
                    vec![reconciled_device_state],
                    HashSet::new(),
                    HashSet::new(),
                );
        }

        /// Drive frames until the driver has started `count` attempts, or the ceiling arrives.
        fn settle_until_started(&mut self, count: usize) {
            for _ in 0..FRAME_CEILING {
                if self.driver_log().started.len() >= count {
                    return;
                }
                self.app.update();
            }
        }

        fn waiting_work(&self) -> WaitingWork {
            self.app
                .world()
                .resource::<Bindings>()
                .waiting_work(&self.role)
        }

        /// Drive frames until the reported unit reaches the role's binding, or the ceiling arrives.
        fn settle(&mut self) {
            for _ in 0..FRAME_CEILING {
                self.app.update();
                if self.app.world().resource::<DriverLog>().started.is_empty() {
                    continue;
                }
                return;
            }
        }

        /// Drive frames until an attempt ending has been observed, or the ceiling arrives.
        fn settle_until_ending(&mut self) {
            for _ in 0..FRAME_CEILING {
                if !self
                    .app
                    .world()
                    .resource::<ObservedEndings>()
                    .on_binding_entity
                    .is_empty()
                {
                    return;
                }
                self.app.update();
            }
        }

        fn attempt(&self) -> RoleAttemptLookup {
            self.app
                .world()
                .resource::<crate::Attempts>()
                .in_flight_for(&self.role, self.app.world().resource::<Bindings>())
        }

        fn role_state(&self) -> RoleState {
            self.app
                .world()
                .resource::<Bindings>()
                .binding(&self.role)
                .map_or(RoleState::Retired, |binding| binding.state)
        }

        fn driver_log(&self) -> &DriverLog { self.app.world().resource::<DriverLog>() }

        fn set_progress(&mut self, attempt_progress: AttemptProgress) {
            self.app.world_mut().resource_mut::<DriverLog>().progress = attempt_progress;
        }

        /// Report a whole set and ask for one run, which is what lands a scan at a known frame.
        fn report(&mut self, keys: Vec<DeviceKey>) {
            *self
                .reported_keys
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = keys;
            self.app
                .world_mut()
                .resource_mut::<DiscoveryControl>()
                .request(self.reporter)
                .expect("the harness registered this reporter");
        }
    }

    fn unit_key(value: &str) -> Result<DeviceKey, Box<dyn Error>> {
        Ok(DeviceKey {
            kind: DeviceKind::Display,
            id:   DeviceIdSource::Reported {
                scheme: SchemeName::new(TEST_SCHEME)?,
                value:  ReportedId::new(value)?,
            },
        })
    }

    fn present(device_key: DeviceKey) -> DeviceRecord {
        DeviceRecord {
            reported_as:  ReportedAs::Keyed(device_key),
            parent:       ReportedParent::Root,
            presence:     Presence::Present,
            claim:        Claim::NotApplicable,
            capabilities: Capabilities::new(),
            serial:       ReportedSerial::NotExposedByUnit,
            os_id:        OsDeviceId::PlatformReportedNothing,
            attachment:   AttachmentPath::PlatformHasNoConcept,
            descriptor:   DeviceDescriptor::PlatformReportedNothing,
        }
    }

    fn test_binding(role: RoleKey, device: DeviceKey, driver: DriverId) -> Binding {
        Binding {
            role,
            endpoint: DeviceEndpoint {
                device,
                id: EndpointId::Whole,
            },
            driver,
            recovery: RecoveryPolicy::Forget,
            retry: RetryOn::NewRevision,
            on_abort: OnAbort::default(),
            on_loss: OnSessionLoss::default(),
            state: RoleState::default(),
            requested: RequestedConfiguration::new(TestConfiguration(3)),
            last_known_good: LastKnownGoodConfiguration::default(),
        }
    }

    #[test]
    fn a_waiting_role_whose_device_resolves_starts_one_attempt_and_polls_it_the_next_frame()
    -> Result<(), Box<dyn Error>> {
        let mut harness = ApplyHarness::started(DriverId(0))?;

        assert_eq!(harness.driver_log().started.len(), 1);
        assert!(harness.driver_log().polled.is_empty());
        assert!(matches!(harness.role_state(), RoleState::Applying(_)));
        assert!(matches!(harness.attempt(), RoleAttemptLookup::InFlight(_)));

        harness.app.update();

        assert_eq!(harness.driver_log().polled.len(), 1);
        assert_eq!(harness.driver_log().started.len(), 1);
        assert!(matches!(harness.role_state(), RoleState::Applying(_)));

        Ok(())
    }

    #[test]
    fn a_successful_attempt_leaves_the_role_ready_without_establishing_a_readback()
    -> Result<(), Box<dyn Error>> {
        let mut harness = ApplyHarness::started(DriverId(0))?;
        harness.set_progress(AttemptProgress::Finished(AttemptOutcome::Succeeded));

        harness.app.update();

        assert_eq!(harness.role_state(), RoleState::Ready);
        assert_eq!(harness.attempt(), RoleAttemptLookup::Idle);
        assert_eq!(
            harness
                .app
                .world()
                .resource::<ObservedEndings>()
                .on_binding_entity,
            vec![(RoleKey::new(TEST_ROLE)?, AttemptOutcome::Succeeded)]
        );
        // A successful apply proves what the kernel asked for, never what the endpoint holds.
        assert!(matches!(
            harness
                .app
                .world()
                .resource::<Bindings>()
                .configuration_for(&harness.role)?,
            AvailableConfiguration::Requested(_)
        ));

        Ok(())
    }

    #[test]
    fn a_role_naming_an_unregistered_driver_stays_waiting_with_no_attempt()
    -> Result<(), Box<dyn Error>> {
        let mut harness = ApplyHarness::started(DriverId(7))?;

        assert!(harness.driver_log().started.is_empty());
        assert_eq!(harness.role_state(), RoleState::Waiting);
        assert_eq!(harness.attempt(), RoleAttemptLookup::Idle);

        harness.app.update();
        harness.app.update();

        // A refused dispatch neither runs an attempt nor spends one: the registry's sequence is
        // non-reusable, so a role whose driver is missing must not consume it once per frame. The
        // first identifier is still on offer after every frame the harness drove.
        assert!(harness.driver_log().started.is_empty());
        assert_eq!(
            harness
                .app
                .world_mut()
                .resource_mut::<crate::Attempts>()
                .issue()
                .expect("a registry that issued nothing can still issue"),
            AttemptId::new(1)
        );

        Ok(())
    }

    #[test]
    fn a_change_to_the_devices_own_state_abandons_the_attempt_and_still_owes_a_restoration()
    -> Result<(), Box<dyn Error>> {
        let mut harness = ApplyHarness::started_reverting(DriverId(0))?;
        let mut moved = harness.reconciled_device_state();
        moved.attachment = AttachmentPath::Reported(ReportedId::new("bay-2")?);
        // Written between frames rather than reported, so the newer revision is visible on the
        // attempt's very first poll frame and the check is not racing a scan.
        harness.reconcile_device(moved);

        harness.app.update();

        assert!(harness.driver_log().polled.is_empty());
        assert_eq!(
            harness
                .app
                .world()
                .resource::<ObservedEndings>()
                .on_binding_entity,
            vec![(RoleKey::new(TEST_ROLE)?, AttemptOutcome::Aborted)]
        );
        assert_eq!(harness.waiting_work(), WaitingWork::RestorationOwed);

        Ok(())
    }

    #[test]
    fn a_rescan_reporting_the_same_state_leaves_the_in_flight_attempt_alone()
    -> Result<(), Box<dyn Error>> {
        let mut harness = ApplyHarness::started(DriverId(0))?;
        let before = harness.app.world().resource::<RiggingRevision>().get();

        harness.report(vec![unit_key(TEST_UNIT)?]);
        for _ in 0..RESCAN_FRAMES {
            harness.app.update();
        }

        // The scan did land: the global counter moved, and it is precisely that counter which no
        // longer reaches the apply path.
        assert!(harness.app.world().resource::<RiggingRevision>().get() > before);
        assert!(
            harness
                .app
                .world()
                .resource::<ObservedEndings>()
                .on_binding_entity
                .is_empty()
        );
        assert!(matches!(harness.attempt(), RoleAttemptLookup::InFlight(_)));

        Ok(())
    }

    #[test]
    fn a_scan_that_changes_one_device_leaves_another_devices_attempt_in_flight()
    -> Result<(), Box<dyn Error>> {
        let mut harness = ApplyHarness::new(DriverId(0))?;
        let second_role = harness.bind_second_role(DriverId(0))?;
        harness.report(vec![unit_key(TEST_UNIT)?, unit_key(SECOND_UNIT)?]);
        harness.settle_until_started(2);
        assert_eq!(harness.driver_log().started.len(), 2);

        // Only the second role's unit leaves the reported set. The first role's device is reported
        // exactly as before, so nothing about it moved.
        harness.report(vec![unit_key(TEST_UNIT)?]);
        harness.settle_until_ending();

        assert_eq!(
            harness
                .app
                .world()
                .resource::<ObservedEndings>()
                .on_binding_entity,
            vec![(second_role, AttemptOutcome::Aborted)]
        );
        assert!(matches!(harness.attempt(), RoleAttemptLookup::InFlight(_)));

        Ok(())
    }

    #[test]
    fn a_lost_claim_abandons_the_attempt_without_owing_a_restoration() -> Result<(), Box<dyn Error>>
    {
        let mut harness = ApplyHarness::started_reverting(DriverId(0))?;
        let mut contended = harness.reconciled_device_state();
        contended.claim = Claim::Contended {
            holder: ClaimHolder::Unidentified,
        };
        harness.reconcile_device(contended);

        harness.app.update();

        assert!(harness.driver_log().polled.is_empty());
        assert_eq!(
            harness
                .app
                .world()
                .resource::<ObservedEndings>()
                .on_binding_entity,
            vec![(RoleKey::new(TEST_ROLE)?, AttemptOutcome::Aborted)]
        );
        // A role that reverts on abort and has a value to go back to still owes nothing: another
        // process holds the endpoint, so writing to it is exactly what must not happen.
        assert_eq!(harness.waiting_work(), WaitingWork::Nothing);

        Ok(())
    }

    #[test]
    fn a_withdrawn_identity_abandons_the_attempt_without_owing_a_restoration()
    -> Result<(), Box<dyn Error>> {
        let mut harness = ApplyHarness::started_reverting(DriverId(0))?;
        let mut unverified = harness.reconciled_device_state();
        unverified.verdict = IdentityVerdict::Unverified(UnverifiedReason::NotUniqueInScan);
        harness.reconcile_device(unverified);

        harness.app.update();

        assert!(harness.driver_log().polled.is_empty());
        assert_eq!(
            harness
                .app
                .world()
                .resource::<ObservedEndings>()
                .on_binding_entity,
            vec![(RoleKey::new(TEST_ROLE)?, AttemptOutcome::Aborted)]
        );
        // The unit at the far end is no longer known to be the authored one, so a restoration would
        // write the role's configuration to whatever is actually there.
        assert_eq!(harness.waiting_work(), WaitingWork::Nothing);

        Ok(())
    }

    #[test]
    fn a_departed_device_abandons_the_attempt_and_the_driver_is_never_polled()
    -> Result<(), Box<dyn Error>> {
        let mut harness = ApplyHarness::started(DriverId(0))?;
        harness.report(Vec::new());

        harness.settle_until_ending();

        assert_eq!(
            harness
                .app
                .world()
                .resource::<ObservedEndings>()
                .on_binding_entity,
            vec![(RoleKey::new(TEST_ROLE)?, AttemptOutcome::Aborted)]
        );
        // The endpoint resolves to nothing now, so no replacement attempt is authorized.
        assert_eq!(harness.driver_log().started.len(), 1);
        assert_eq!(harness.attempt(), RoleAttemptLookup::Idle);

        Ok(())
    }

    #[test]
    fn an_offline_inventory_entry_abandons_the_attempt_and_starts_no_other()
    -> Result<(), Box<dyn Error>> {
        let mut harness = ApplyHarness::started(DriverId(0))?;
        harness
            .app
            .world_mut()
            .resource_mut::<HardwareInventory>()
            .configure(ConfiguredDevice {
                key:  unit_key(TEST_UNIT)?,
                mode: ConfiguredDeviceMode::Offline,
            });

        harness.app.update();
        harness.app.update();

        assert!(harness.driver_log().polled.is_empty());
        assert_eq!(harness.driver_log().started.len(), 1);
        assert_eq!(harness.attempt(), RoleAttemptLookup::Idle);
        assert_eq!(
            harness
                .app
                .world()
                .resource::<ObservedEndings>()
                .on_binding_entity,
            vec![(RoleKey::new(TEST_ROLE)?, AttemptOutcome::Aborted)]
        );

        Ok(())
    }

    #[test]
    fn an_attempt_past_its_bounded_overrun_is_abandoned() -> Result<(), Box<dyn Error>> {
        let mut harness = ApplyHarness::new(DriverId(0))?;
        // Set before the attempt is authorized: the deadline is stamped onto the attempt when it
        // starts, so a limit changed afterwards would not shorten one already in flight.
        {
            let mut rigging_limits = harness.app.world_mut().resource_mut::<RiggingLimits>();
            rigging_limits.apply_deadline = Duration::ZERO;
            rigging_limits.apply_overrun = Duration::ZERO;
        }
        harness.settle();

        harness.app.update();

        assert!(harness.driver_log().polled.is_empty());
        assert_eq!(
            harness
                .app
                .world()
                .resource::<ObservedEndings>()
                .on_binding_entity,
            vec![(RoleKey::new(TEST_ROLE)?, AttemptOutcome::Aborted)]
        );
        // The abort is terminal for the frame that made it. The three apply systems are chained
        // inside one set, so an ungated role would be restarted by the dispatch two systems later,
        // against the conditions that just abandoned it — and under `RetryOn::NewRevision` it stays
        // stopped until a scan lands, which the on-demand reporter never does unasked.
        assert_eq!(harness.driver_log().started.len(), 1);
        assert_eq!(harness.role_state(), RoleState::Waiting);
        assert_eq!(harness.attempt(), RoleAttemptLookup::Idle);

        harness.app.update();
        harness.app.update();

        assert_eq!(harness.driver_log().started.len(), 1);
        assert_eq!(
            harness
                .app
                .world()
                .resource::<ObservedEndings>()
                .on_binding_entity
                .len(),
            1
        );

        Ok(())
    }

    #[test]
    fn a_device_that_only_goes_stale_abandons_its_attempt_without_a_scan_landing()
    -> Result<(), Box<dyn Error>> {
        // A declared cadence is what gives the reporter a freshness lease at all: an on-demand
        // reporter promised nothing, so its silence proves nothing and its devices never age out.
        // The backstop is long enough that the reporter is never due again during the test, so the
        // only thing that changes is how old its evidence is.
        let mut harness = ApplyHarness::with_cadence(
            DriverId(0),
            DiscoveryCadence::EventDriven {
                backstop: Duration::from_hours(1),
            },
        )?;
        harness.settle();
        assert_eq!(harness.driver_log().started.len(), 1);
        let authorized_revision = *harness.app.world().resource::<RiggingRevision>();

        // The device is still reported and still names the same unit. Only the evidence ages.
        harness
            .app
            .world_mut()
            .resource_mut::<crate::registration::Reporters>()
            .backdate_completion(harness.reporter, Duration::from_hours(2));

        harness.settle_until_ending();

        assert_eq!(
            harness
                .app
                .world()
                .resource::<ObservedEndings>()
                .on_binding_entity,
            vec![(RoleKey::new(TEST_ROLE)?, AttemptOutcome::Aborted)]
        );
        assert!(harness.driver_log().polled.is_empty());
        // The whole point of the freshness read: no reporter submitted anything, so the ending
        // came from the lease rewriting presence rather than from any scan the kernel ingested.
        assert_eq!(
            *harness.app.world().resource::<RiggingRevision>(),
            authorized_revision
        );
        // The key never left the set, so the endpoint still resolves — it is the presence the
        // lease rewrote, and the presence alone that ended the attempt.
        assert!(matches!(
            harness
                .app
                .world()
                .resource::<crate::Devices>()
                .resolve(&unit_key(TEST_UNIT)?),
            crate::DeviceResolution::Resolved(_)
        ));

        Ok(())
    }

    #[test]
    fn a_revision_abort_records_a_revert_only_when_the_role_asked_and_has_a_value_to_go_back_to()
    -> Result<(), Box<dyn Error>> {
        let role = RoleKey::new(TEST_ROLE)?;
        let mut bindings = Bindings::default();
        let mut reverting = test_binding(role.clone(), unit_key(TEST_UNIT)?, DriverId(0));
        reverting.on_abort = OnAbort::Revert;
        bindings.register(reverting)?;

        // Nothing a safe readback established, so there is no captured configuration to go back to
        // and the role keeps whatever the abandoned apply left.
        super::apply_abort_policy(
            &mut bindings,
            &role,
            super::AttemptInvalidation::RevisionAdvanced,
        );
        assert_eq!(bindings.waiting_work(&role), crate::WaitingWork::Nothing);

        let mut established = test_binding(role.clone(), unit_key(TEST_UNIT)?, DriverId(0));
        established.on_abort = OnAbort::Revert;
        established.last_known_good = LastKnownGoodConfiguration::known(TestConfiguration(9));
        bindings.replace(established)?;

        // A claim lost and a service veto never revert; only a revision change consults the policy.
        super::apply_abort_policy(
            &mut bindings,
            &role,
            super::AttemptInvalidation::DeviceNotPresent,
        );
        assert_eq!(bindings.waiting_work(&role), crate::WaitingWork::Nothing);

        super::apply_abort_policy(
            &mut bindings,
            &role,
            super::AttemptInvalidation::RevisionAdvanced,
        );

        // The kernel drives no hardware, so reverting is recorded as the restoration the next
        // dispatch mints and the role's own driver performs.
        assert_eq!(
            bindings.waiting_work(&role),
            crate::WaitingWork::RestorationOwed
        );

        let leave_as_is_role = RoleKey::new("secondary-window")?;
        let mut leave_as_is = test_binding(
            leave_as_is_role.clone(),
            unit_key("UNIT-0002")?,
            DriverId(0),
        );
        leave_as_is.last_known_good = LastKnownGoodConfiguration::known(TestConfiguration(9));
        bindings.register(leave_as_is)?;

        super::apply_abort_policy(
            &mut bindings,
            &leave_as_is_role,
            super::AttemptInvalidation::RevisionAdvanced,
        );

        assert_eq!(
            bindings.waiting_work(&leave_as_is_role),
            crate::WaitingWork::Nothing
        );

        Ok(())
    }

    #[test]
    fn a_retired_role_reports_its_abandoned_attempt_globally() -> Result<(), Box<dyn Error>> {
        let mut harness = ApplyHarness::started(DriverId(0))?;
        harness
            .app
            .world_mut()
            .resource_mut::<Bindings>()
            .retire(&harness.role)?;

        harness.app.update();

        assert!(harness.driver_log().polled.is_empty());
        let observed_endings = harness.app.world().resource::<ObservedEndings>();
        assert!(observed_endings.on_binding_entity.is_empty());
        assert_eq!(
            observed_endings.after_retirement,
            vec![(RoleKey::new(TEST_ROLE)?, AttemptOutcome::Aborted)]
        );

        Ok(())
    }

    #[test]
    fn two_transitions_naming_one_role_end_its_attempt_once() -> Result<(), Box<dyn Error>> {
        let mut harness = ApplyHarness::started(DriverId(0))?;
        let role = harness.role.clone();
        {
            let mut bindings = harness.app.world_mut().resource_mut::<Bindings>();
            let _replaced = bindings.replace(test_binding(
                role.clone(),
                unit_key(TEST_UNIT)?,
                DriverId(0),
            ))?;
            let _retired = bindings.retire(&role)?;
        }

        harness.app.update();

        let observed_endings = harness.app.world().resource::<ObservedEndings>();
        assert_eq!(
            observed_endings.on_binding_entity.len() + observed_endings.after_retirement.len(),
            1
        );

        Ok(())
    }
}
