//! Entity events the kernel emits on the edge where a reconciled fact changed.
//!
//! Every event here fires on a change edge and never on a settled frame, so a consumer can treat
//! one arrival as one transition instead of debouncing a per-frame restatement.

use bevy::ecs::event::EntityEvent;
use bevy::ecs::event::Event;
use bevy::prelude::Entity;
use bevy::prelude::Reflect;

use crate::AttemptId;
use crate::AttemptOutcome;
use crate::RoleKey;

/// The capabilities two reporters disagree about for this device changed.
///
/// A co-reported unit whose reporters contradict each other about one capability stays drivable
/// for every capability they agree about, so the disagreement has to reach a diagnostic somehow:
/// nothing else in the kernel reports which capability went contested. Emitted on the change edge
/// only. An empty `capabilities` means the disagreement cleared and the device is fully drivable
/// again.
#[derive(Debug, EntityEvent, Reflect)]
pub struct CapabilitiesDisputed {
    /// Device entity whose contributors changed what they disagree about.
    #[event_target]
    pub device:       Entity,
    /// Reflected type paths of the disputed capability components, resolved from
    /// `crate::ReconciledDeviceState::disputed` through the type registry.
    ///
    /// Type paths rather than the `std::any::TypeId` values the kernel stores, because `TypeId`
    /// does not implement `Reflect` and so could not cross an event a Bevy Remote Protocol client
    /// reads — and the path is what a human reading a warning needs anyway.
    pub capabilities: Vec<String>,
}

/// One attempt reached a terminal outcome while its role still had a binding entity.
///
/// Targeted at the binding entity rather than the device entity because the binding outlives the
/// unit: an attempt that ends because the device departed still has somewhere to land. The role is
/// carried alongside the target so a consumer that observes the event does not have to read the
/// entity's components back to learn which role ended.
#[derive(Debug, EntityEvent, Reflect)]
pub struct AttemptFinished {
    /// Binding entity for the role this attempt ran for.
    #[event_target]
    pub binding: Entity,
    /// Application role the attempt ran for.
    pub role:    RoleKey,
    /// Registry-issued identifier of the attempt that ended.
    pub attempt: AttemptId,
    /// Terminal result the attempt ended with.
    pub outcome: AttemptOutcome,
}

/// An attempt ended after its role was retired or replaced, so no binding entity remained.
///
/// Global rather than entity-targeted: retirement despawns the binding entity in the same frame the
/// kernel aborts the attempt, and an event addressed to a despawned entity reaches no observer at
/// all. The ending still has to be reportable, so it carries the `RoleKey` the entity would have
/// identified.
#[derive(Debug, Event, Reflect)]
pub struct RetiredRoleAttemptEnded {
    /// Application role whose binding was retired or replaced out from under the attempt.
    pub role:    RoleKey,
    /// Registry-issued identifier of the attempt that ended.
    pub attempt: AttemptId,
    /// Terminal result the attempt ended with, which is `crate::AttemptOutcome::Aborted` whenever
    /// the kernel rather than the driver ended it.
    pub outcome: AttemptOutcome,
}
