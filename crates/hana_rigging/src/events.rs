//! Entity events the kernel emits on the edge where a reconciled fact changed.
//!
//! Every event here fires on a change edge and never on a settled frame, so a consumer can treat
//! one arrival as one transition instead of debouncing a per-frame restatement.

use bevy::ecs::event::EntityEvent;
use bevy::prelude::Entity;
use bevy::prelude::Reflect;

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
