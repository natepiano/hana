//! Validated binding output: per-semantic-action sets and their entries.
//!
//! Types:
//! - [`OrbitCamOrbitActionBindings`] / [`OrbitCamPanActionBindings`] /
//!   [`OrbitCamZoomSmoothActionBindings`] / [`OrbitCamZoomCoarseActionBindings`] — per-action
//!   public newtypes stored on [`super::OrbitCamBindings`].
//! - [`ActionBindingSet`] / [`HeldActionBindingSet`] — generic backing storage parameterized by
//!   [`super::super::CameraSemanticAction`] / [`super::super::HeldCameraAction`].
//! - [`ActionBindingEntry`] / [`HeldActionBindingEntry`] — individual binding entries holding
//!   flattened [`super::descriptor::InputBindingDescriptor`] data plus source and routing metadata.
//! - [`BindingRoutePolicy`] / [`BindingEngagement`] — routing and engagement enums attached to each
//!   entry.

use std::marker::PhantomData;

use bevy::prelude::*;

use super::descriptor::InputBindingDescriptor;
use super::descriptor::InputBindingEntry;
use super::error::OrbitCamBindingsError;
use super::held_binding::BindingGates;
use super::held_binding::OrbitCamHeldBinding;
use crate::input::CameraInteractionSources;
use crate::input::CameraSemanticAction;
use crate::input::ControlSpeed;
use crate::input::HeldCameraAction;
use crate::input::OrbitCamOrbitAction;
use crate::input::OrbitCamPanAction;
use crate::input::OrbitCamZoomCoarseAction;
use crate::input::OrbitCamZoomSmoothAction;
use crate::input::actions::OrbitCamOrbitEngagedAction;
use crate::input::actions::OrbitCamPanEngagedAction;
use crate::input::actions::OrbitCamZoomEngagedAction;

/// Orbit action binding set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrbitCamOrbitActionBindings(
    pub(super) HeldActionBindingSet<OrbitCamOrbitAction, OrbitCamOrbitEngagedAction>,
);

/// Pan action binding set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrbitCamPanActionBindings(
    pub(super) HeldActionBindingSet<OrbitCamPanAction, OrbitCamPanEngagedAction>,
);

/// Smooth zoom action binding set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrbitCamZoomSmoothActionBindings(
    pub(super) HeldActionBindingSet<OrbitCamZoomSmoothAction, OrbitCamZoomEngagedAction>,
);

/// Coarse zoom action binding set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrbitCamZoomCoarseActionBindings(pub(super) ActionBindingSet<OrbitCamZoomCoarseAction>);

impl OrbitCamOrbitActionBindings {
    /// Returns the number of orbit bindings.
    #[must_use]
    pub const fn len(&self) -> usize { self.0.len() }

    /// Returns `true` when there are no orbit bindings.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.0.is_empty() }

    /// Returns orbit binding entries.
    #[must_use]
    pub fn entries(&self) -> &[HeldActionBindingEntry<OrbitCamOrbitAction>] { self.0.entries() }

    /// Returns orbit binding entries that participate in runtime input.
    pub fn enabled_entries(
        &self,
    ) -> impl Iterator<Item = &HeldActionBindingEntry<OrbitCamOrbitAction>> {
        self.0.enabled_entries()
    }
}

impl OrbitCamPanActionBindings {
    /// Returns the number of pan bindings.
    #[must_use]
    pub const fn len(&self) -> usize { self.0.len() }

    /// Returns `true` when there are no pan bindings.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.0.is_empty() }

    /// Returns pan binding entries.
    #[must_use]
    pub fn entries(&self) -> &[HeldActionBindingEntry<OrbitCamPanAction>] { self.0.entries() }

    /// Returns pan binding entries that participate in runtime input.
    pub fn enabled_entries(
        &self,
    ) -> impl Iterator<Item = &HeldActionBindingEntry<OrbitCamPanAction>> {
        self.0.enabled_entries()
    }
}

impl OrbitCamZoomSmoothActionBindings {
    /// Returns the number of smooth-zoom bindings.
    #[must_use]
    pub const fn len(&self) -> usize { self.0.len() }

    /// Returns `true` when there are no smooth-zoom bindings.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.0.is_empty() }

    /// Returns smooth-zoom binding entries.
    #[must_use]
    pub fn entries(&self) -> &[HeldActionBindingEntry<OrbitCamZoomSmoothAction>] {
        self.0.entries()
    }

    /// Returns smooth-zoom binding entries that participate in runtime input.
    pub fn enabled_entries(
        &self,
    ) -> impl Iterator<Item = &HeldActionBindingEntry<OrbitCamZoomSmoothAction>> {
        self.0.enabled_entries()
    }
}

impl OrbitCamZoomCoarseActionBindings {
    /// Returns the number of coarse-zoom bindings.
    #[must_use]
    pub const fn len(&self) -> usize { self.0.len() }

    /// Returns `true` when there are no coarse-zoom bindings.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.0.is_empty() }

    /// Returns coarse-zoom binding entries.
    #[must_use]
    pub fn entries(&self) -> &[ActionBindingEntry<OrbitCamZoomCoarseAction>] { self.0.entries() }

    /// Returns coarse-zoom binding entries that participate in runtime input.
    pub fn enabled_entries(
        &self,
    ) -> impl Iterator<Item = &ActionBindingEntry<OrbitCamZoomCoarseAction>> {
        self.0.enabled_entries()
    }
}

/// Binding set for one semantic action.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActionBindingSet<A: CameraSemanticAction> {
    pub(super) entries: Vec<ActionBindingEntry<A>>,
    pub(super) action:  PhantomData<A>,
}

impl<A: CameraSemanticAction> ActionBindingSet<A> {
    /// Returns the number of bindings in the set.
    #[must_use]
    pub const fn len(&self) -> usize { self.entries.len() }

    /// Returns `true` when the set has no bindings.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.entries.is_empty() }

    /// Returns the binding entries.
    #[must_use]
    pub fn entries(&self) -> &[ActionBindingEntry<A>] { &self.entries }

    pub(super) fn enabled_entries(&self) -> impl Iterator<Item = &ActionBindingEntry<A>> {
        self.entries.iter().filter(|entry| entry.is_enabled())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct HeldActionBindingSet<A: HeldCameraAction, E: CameraSemanticAction> {
    pub(super) entries: Vec<HeldActionBindingEntry<A>>,
    pub(super) action:  PhantomData<(A, E)>,
}

impl<A: HeldCameraAction, E: CameraSemanticAction> HeldActionBindingSet<A, E> {
    pub(super) const fn len(&self) -> usize { self.entries.len() }

    pub(super) const fn is_empty(&self) -> bool { self.entries.is_empty() }

    pub(super) fn entries(&self) -> &[HeldActionBindingEntry<A>] { &self.entries }

    pub(super) fn enabled_entries(&self) -> impl Iterator<Item = &HeldActionBindingEntry<A>> {
        self.entries.iter().filter(|entry| entry.is_enabled())
    }
}

/// Binding entry for an impulse camera action.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionBindingEntry<A: CameraSemanticAction> {
    pub(super) binding:    InputBindingDescriptor,
    pub(super) sources:    CameraInteractionSources,
    pub(super) route:      BindingRoutePolicy,
    pub(super) engagement: BindingEngagement,
    pub(super) action:     PhantomData<A>,
}

impl<A: CameraSemanticAction> ActionBindingEntry<A> {
    /// Returns the flattened binding descriptor.
    #[must_use]
    pub const fn binding_descriptor(&self) -> &InputBindingDescriptor { &self.binding }

    /// Returns source metadata for this binding.
    #[must_use]
    pub const fn sources(&self) -> CameraInteractionSources { self.sources }

    /// Returns route policy for this binding.
    #[must_use]
    pub const fn route(&self) -> BindingRoutePolicy { self.route }

    /// Returns engagement kind for this binding.
    #[must_use]
    pub const fn engagement(&self) -> BindingEngagement { self.engagement }

    fn is_enabled(&self) -> bool { self.binding.has_enabled_entries() }
}

/// Paired movement and engagement entry for held camera actions.
#[derive(Clone, Debug, PartialEq)]
pub struct HeldActionBindingEntry<A: HeldCameraAction> {
    pub(super) motion:     InputBindingDescriptor,
    pub(super) engagement: InputBindingDescriptor,
    pub(super) gates:      BindingGates,
    pub(super) sources:    CameraInteractionSources,
    pub(super) route:      BindingRoutePolicy,
    pub(super) speed:      ControlSpeed,
    pub(super) action:     PhantomData<A>,
}

impl<A: HeldCameraAction> HeldActionBindingEntry<A> {
    /// Creates a held binding from BEI-style input bindings.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when the source metadata is empty.
    pub fn new(binding: OrbitCamHeldBinding) -> Result<Self, OrbitCamBindingsError> {
        if binding.sources.is_empty() {
            return Err(OrbitCamBindingsError::MissingSources);
        }
        Ok(Self {
            motion:     binding.motion.descriptor(),
            engagement: binding.engagement.descriptor(),
            gates:      binding.gates,
            sources:    binding.sources,
            route:      binding.route,
            speed:      binding.speed,
            action:     PhantomData,
        })
    }

    /// Returns the flattened motion binding descriptor.
    #[must_use]
    pub const fn motion_descriptor(&self) -> &InputBindingDescriptor { &self.motion }

    /// Returns the flattened engagement binding descriptor.
    #[must_use]
    pub const fn engagement_descriptor(&self) -> &InputBindingDescriptor { &self.engagement }

    /// Returns gates applied to both motion and engagement descriptors.
    #[must_use]
    pub const fn gates(&self) -> &BindingGates { &self.gates }

    /// Returns the speed variant this binding represents.
    #[must_use]
    pub const fn speed(&self) -> ControlSpeed { self.speed }

    /// Returns source metadata for this binding.
    #[must_use]
    pub const fn sources(&self) -> CameraInteractionSources { self.sources }

    /// Returns route policy for this binding.
    #[must_use]
    pub const fn route(&self) -> BindingRoutePolicy { self.route }

    fn is_enabled(&self) -> bool {
        self.motion.has_enabled_entries() && self.engagement.has_enabled_entries()
    }

    /// Returns motion binding entries that participate in runtime input.
    pub fn enabled_motion_entries(&self) -> impl Iterator<Item = &InputBindingEntry> {
        self.motion.enabled_entries()
    }
}

/// Route policy attached to a binding entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum BindingRoutePolicy {
    /// Route by cursor or touch position.
    #[default]
    CursorPosition,
    /// Route without position when a latch or explicit owner exists.
    NoPosition,
}

/// Whether a binding is an impulse or held input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum BindingEngagement {
    /// One-frame or impulse binding.
    Impulse,
    /// Held binding with a separate engagement binding.
    Held,
}
