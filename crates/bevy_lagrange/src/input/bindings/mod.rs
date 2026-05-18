//! Orbit-camera binding model: presets, builder, validated bindings, and the supporting
//! descriptor and entry types consumed by `crate::input::adapter` and
//! `crate::input::control_summary`.
//!
//! Submodules:
//! - [`preset`] — built-in [`OrbitCamPreset`] keymaps.
//! - [`builder`] — [`OrbitCamBindingsBuilder`], [`OrbitCamBindingsDescriptor`], dispatch enums, and
//!   the user-facing concrete binding kinds (mouse drag, trackpad, mouse wheel, pinch, button drag,
//!   touch, gamepad policy, zoom direction).
//! - [`held_binding`] — [`OrbitCamHeldBinding`] / [`OrbitCamInputBinding`] primitives.
//! - [`action_set`] — per-action binding-set newtypes and entry types written by the validator and
//!   read by the adapter.
//! - [`descriptor`] — internal descriptor and entry types plus the runtime binding-active
//!   predicates.
//! - [`error`] — [`OrbitCamBindingsError`].
//! - [`validate`] — descriptor → [`OrbitCamBindings`] lowering.
//!
//! This file holds the validated runtime [`OrbitCamBindings`] component, the
//! [`PinchGestureZoom`] field-type enum, and the cross-cutting integration tests.

mod action_set;
mod builder;
mod descriptor;
mod error;
mod held_binding;
mod preset;
mod validate;

pub use action_set::ActionBindingEntry;
pub use action_set::ActionBindingSet;
pub use action_set::BindingEngagement;
pub use action_set::BindingRoutePolicy;
pub use action_set::HeldActionBindingEntry;
pub use action_set::OrbitCamOrbitActionBindings;
pub use action_set::OrbitCamPanActionBindings;
pub use action_set::OrbitCamZoomCoarseActionBindings;
pub use action_set::OrbitCamZoomSmoothActionBindings;
use bevy::prelude::*;
pub use builder::CameraInputGamepadSelectionPolicy;
pub use builder::OrbitCamBindingsBuilder;
pub use builder::OrbitCamBindingsDescriptor;
pub use builder::OrbitCamButtonDragZoom;
pub use builder::OrbitCamButtonDragZoomAxis;
pub use builder::OrbitCamMouseDrag;
pub use builder::OrbitCamMouseWheelZoom;
pub use builder::OrbitCamOrbitBinding;
pub use builder::OrbitCamPanBinding;
pub use builder::OrbitCamPinchZoom;
pub use builder::OrbitCamTouchBinding;
pub use builder::OrbitCamTrackpadScroll;
pub use builder::OrbitCamZoomBinding;
pub use builder::WheelZoomPolarity;
pub use builder::ZoomDirection;
#[cfg(test)]
pub(crate) use builder::invalid_bindings_descriptor_for_tests;
pub use descriptor::ActionBindingDescriptor;
pub use descriptor::InputBindingDescriptor;
pub use descriptor::InputBindingEntry;
pub use descriptor::InputBindingTransform;
pub(crate) use descriptor::mod_keys_pressed;
pub use error::OrbitCamBindingsError;
pub use held_binding::OrbitCamHeldBinding;
pub use held_binding::OrbitCamInputBinding;
pub use preset::OrbitCamPreset;
pub use validate::validate_bindings;

/// Validated runtime binding specification for an `OrbitCam`.
#[derive(Component, Clone, Debug, PartialEq, Reflect)]
#[reflect(Component)]
#[reflect(opaque)]
pub struct OrbitCamBindings {
    pub(super) orbit:            OrbitCamOrbitActionBindings,
    pub(super) pan:              OrbitCamPanActionBindings,
    pub(super) zoom_smooth:      OrbitCamZoomSmoothActionBindings,
    pub(super) zoom_coarse:      OrbitCamZoomCoarseActionBindings,
    pub(super) trackpad_orbit:   Vec<OrbitCamTrackpadScroll>,
    pub(super) trackpad_pan:     Vec<OrbitCamTrackpadScroll>,
    pub(super) trackpad_zoom:    Vec<OrbitCamTrackpadScroll>,
    pub(super) mouse_wheel_zoom: Option<OrbitCamMouseWheelZoom>,
    pub(super) pinch_zoom:       PinchGestureZoom,
    pub(super) touch:            Option<OrbitCamTouchBinding>,
    pub(super) gamepad:          CameraInputGamepadSelectionPolicy,
    pub(super) zoom_direction:   ZoomDirection,
    pub(super) button_drag_zoom: Option<OrbitCamButtonDragZoom>,
}

/// Whether the pinch gesture is wired up as a zoom input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum PinchGestureZoom {
    /// Pinch gestures contribute to zoom.
    Enabled,
    /// Pinch gestures are ignored.
    #[default]
    Disabled,
}

impl OrbitCamBindings {
    /// Creates an `OrbitCamBindings` builder.
    #[must_use]
    pub fn builder() -> OrbitCamBindingsBuilder { OrbitCamBindingsBuilder::default() }

    /// Returns orbit action bindings.
    #[must_use]
    pub const fn orbit(&self) -> &OrbitCamOrbitActionBindings { &self.orbit }

    /// Returns pan action bindings.
    #[must_use]
    pub const fn pan(&self) -> &OrbitCamPanActionBindings { &self.pan }

    /// Returns smooth zoom action bindings.
    #[must_use]
    pub const fn zoom_smooth(&self) -> &OrbitCamZoomSmoothActionBindings { &self.zoom_smooth }

    /// Returns coarse zoom action bindings.
    #[must_use]
    pub const fn zoom_coarse(&self) -> &OrbitCamZoomCoarseActionBindings { &self.zoom_coarse }

    /// Returns trackpad orbit bindings.
    #[must_use]
    pub fn trackpad_orbit(&self) -> &[OrbitCamTrackpadScroll] { &self.trackpad_orbit }

    /// Returns trackpad pan bindings.
    #[must_use]
    pub fn trackpad_pan(&self) -> &[OrbitCamTrackpadScroll] { &self.trackpad_pan }

    /// Returns trackpad zoom bindings.
    #[must_use]
    pub fn trackpad_zoom(&self) -> &[OrbitCamTrackpadScroll] { &self.trackpad_zoom }

    /// Returns mouse wheel zoom binding.
    #[must_use]
    pub const fn mouse_wheel_zoom(&self) -> Option<OrbitCamMouseWheelZoom> { self.mouse_wheel_zoom }

    /// Returns whether pinch zoom is enabled.
    #[must_use]
    pub const fn pinch_zoom(&self) -> PinchGestureZoom { self.pinch_zoom }

    /// Returns touch policy.
    #[must_use]
    pub const fn touch(&self) -> Option<OrbitCamTouchBinding> { self.touch }

    /// Returns gamepad selection policy.
    #[must_use]
    pub const fn gamepad(&self) -> CameraInputGamepadSelectionPolicy { self.gamepad }

    /// Returns zoom direction policy.
    #[must_use]
    pub const fn zoom_direction(&self) -> ZoomDirection { self.zoom_direction }

    /// Returns button-drag zoom policy.
    #[must_use]
    pub const fn button_drag_zoom(&self) -> Option<OrbitCamButtonDragZoom> { self.button_drag_zoom }
}

#[cfg(test)]
mod tests {
    use bevy_enhanced_input::prelude::Binding;
    use bevy_enhanced_input::prelude::ModKeys;

    use super::action_set::BindingEngagement;
    use super::action_set::BindingRoutePolicy;
    use super::descriptor::ActionBindingDescriptor;
    use super::descriptor::HeldBindingDescriptor;
    use super::*;
    use crate::input::CameraInteractionSources;
    use crate::input::constants::ORBIT_ACTION_NAME;
    use crate::input::constants::PAN_ACTION_NAME;
    use crate::input::constants::ZOOM_COARSE_ACTION_NAME;

    fn descriptor_with_no_bindings() -> OrbitCamBindingsDescriptor {
        OrbitCamBindingsDescriptor::default()
    }

    #[test]
    fn presets_validate_through_shared_path() -> Result<(), OrbitCamBindingsError> {
        let simple = OrbitCamPreset::SimpleMouse.to_bindings()?;
        assert!(simple.mouse_wheel_zoom().is_some());
        assert_eq!(simple.trackpad_zoom().len(), 1);
        assert_eq!(simple.pinch_zoom(), PinchGestureZoom::Enabled);
        assert!(simple.touch().is_none());

        let blender = OrbitCamPreset::BlenderLike.to_bindings()?;
        assert_eq!(blender.orbit().len(), 1);
        assert_eq!(blender.pan().len(), 1);
        assert_eq!(blender.trackpad_orbit().len(), 1);
        assert_eq!(blender.trackpad_pan().len(), 1);
        assert_eq!(blender.trackpad_zoom().len(), 1);
        assert!(blender.mouse_wheel_zoom().is_some());
        assert_eq!(blender.pinch_zoom(), PinchGestureZoom::Enabled);

        let [pan] = blender.pan().entries() else {
            assert_eq!(blender.pan().entries().len(), 1);
            return Ok(());
        };
        assert_eq!(
            pan.engagement_descriptor().mouse_button_engagement(),
            Some((MouseButton::Middle, ModKeys::SHIFT))
        );

        Ok(())
    }

    #[test]
    fn empty_bindings_are_valid() -> Result<(), OrbitCamBindingsError> {
        let bindings = OrbitCamBindings::builder().build()?;

        assert!(bindings.orbit().is_empty());
        assert!(bindings.pan().is_empty());
        assert!(bindings.zoom_smooth().is_empty());
        assert!(bindings.zoom_coarse().is_empty());
        assert!(bindings.trackpad_orbit().is_empty());
        assert!(bindings.trackpad_pan().is_empty());
        assert!(bindings.trackpad_zoom().is_empty());
        assert_eq!(bindings.pinch_zoom(), PinchGestureZoom::Disabled);
        assert!(bindings.mouse_wheel_zoom().is_none());

        Ok(())
    }

    #[test]
    fn held_motion_without_engagement_is_rejected() {
        let mut descriptor = descriptor_with_no_bindings();
        descriptor.orbit.push(HeldBindingDescriptor {
            motion:             OrbitCamInputBinding::from(Binding::mouse_motion()).descriptor(),
            engagement:         None,
            sources:            CameraInteractionSources::MOUSE,
            engagement_sources: CameraInteractionSources::MOUSE,
            route:              BindingRoutePolicy::CursorPosition,
        });

        assert_eq!(
            validate_bindings(&descriptor),
            Err(OrbitCamBindingsError::HeldMotionMissingEngagement {
                action: ORBIT_ACTION_NAME,
            })
        );
    }

    #[test]
    fn held_source_mismatch_is_rejected() {
        let mut descriptor = descriptor_with_no_bindings();
        descriptor.pan.push(HeldBindingDescriptor {
            motion:             OrbitCamInputBinding::from(Binding::mouse_motion()).descriptor(),
            engagement:         Some(OrbitCamInputBinding::from(KeyCode::ShiftLeft).descriptor()),
            sources:            CameraInteractionSources::MOUSE,
            engagement_sources: CameraInteractionSources::KEYBOARD,
            route:              BindingRoutePolicy::CursorPosition,
        });

        assert_eq!(
            validate_bindings(&descriptor),
            Err(OrbitCamBindingsError::HeldSourceMismatch {
                action: PAN_ACTION_NAME,
            })
        );
    }

    #[test]
    fn impulse_engagement_is_rejected() {
        let mut descriptor = descriptor_with_no_bindings();
        descriptor.zoom_coarse.push(ActionBindingDescriptor {
            binding:    OrbitCamInputBinding::from(KeyCode::Space).descriptor(),
            sources:    CameraInteractionSources::KEYBOARD,
            route:      BindingRoutePolicy::NoPosition,
            engagement: BindingEngagement::Held,
        });

        assert_eq!(
            validate_bindings(&descriptor),
            Err(OrbitCamBindingsError::ImpulseEngagement {
                action: ZOOM_COARSE_ACTION_NAME,
            })
        );
    }

    #[test]
    fn held_binding_preserves_bei_bindings() -> Result<(), OrbitCamBindingsError> {
        let binding = OrbitCamHeldBinding::new(KeyCode::KeyA, KeyCode::ShiftLeft);

        let bindings = OrbitCamBindings::builder().orbit(binding).build()?;
        let [entry] = bindings.orbit().entries() else {
            assert_eq!(bindings.orbit().entries().len(), 1);
            return Ok(());
        };

        assert!(entry.sources().contains(CameraInteractionSources::KEYBOARD));
        assert_eq!(entry.route(), BindingRoutePolicy::NoPosition);

        Ok(())
    }
}
