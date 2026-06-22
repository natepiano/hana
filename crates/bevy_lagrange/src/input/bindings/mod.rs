//! Orbit-camera binding model: presets, builder, validated bindings, and the supporting
//! descriptor and entry types consumed by `crate::input::adapter` and
//! `crate::input::control_summary`.
//!
//! Submodules:
//! - [`preset`] — built-in [`OrbitCamPreset`] keymaps.
//! - [`builder`] — [`OrbitCamBindingsBuilder`], [`OrbitCamBindingsDescriptor`], dispatch enums, and
//!   the user-facing concrete binding kinds (mouse drag, trackpad, mouse wheel, pinch, button drag,
//!   touch, gamepad policy, zoom inversion).
//! - [`held_binding`] — [`OrbitCamHeldBinding`] / [`OrbitCamInputBinding`] primitives.
//! - [`action_set`] — per-action binding-set newtypes and entry types written by the validator and
//!   read by the adapter.
//! - [`descriptor`] — internal descriptor and entry types plus the runtime binding-active
//!   predicates.
//! - [`error`] — [`OrbitCamBindingsError`].
//! - [`validate`] — descriptor → [`OrbitCamBindings`] lowering.
//!
//! This file holds the validated runtime [`OrbitCamBindings`] value, the
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
pub use builder::OrbitCamBindingWithSensitivity;
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
pub use builder::OrbitCamTouchBindingConfig;
pub use builder::OrbitCamTrackpadScroll;
pub use builder::OrbitCamZoomBinding;
pub use builder::ZoomInversion;
#[cfg(test)]
pub(crate) use builder::invalid_bindings_descriptor_for_tests;
pub use descriptor::ActionBindingDescriptor;
pub use descriptor::InputAxisTransform;
pub use descriptor::InputBindingDescriptor;
pub use descriptor::InputBindingEntry;
pub use descriptor::InputBindingModifiers;
pub use descriptor::InputBindingScale;
pub use descriptor::InputDeadZone;
pub use descriptor::InputDeltaScale;
pub use descriptor::InputSensitivity;
pub use descriptor::OrbitCamScalePolicy;
pub use descriptor::OrbitCamSensitivity;
pub use descriptor::OrbitCamSlowMode;
pub(crate) use descriptor::mod_keys_pressed;
pub use error::OrbitCamBindingsError;
pub use held_binding::BindingGates;
pub use held_binding::OrbitCamBindingGate;
pub use held_binding::OrbitCamGateInput;
pub use held_binding::OrbitCamGatePolarity;
pub use held_binding::OrbitCamHeldBinding;
pub use held_binding::OrbitCamInputBinding;
pub use preset::OrbitCamBlenderLikeKeyboardPreset;
pub use preset::OrbitCamBlenderLikePreset;
pub use preset::OrbitCamGamepadPreset;
pub use preset::OrbitCamGamepadPresetBuilder;
pub use preset::OrbitCamKeyboardPreset;
pub use preset::OrbitCamPreset;
pub use preset::OrbitCamPresetKind;
pub use preset::OrbitCamSimpleMouseKeyboardPreset;
pub use preset::OrbitCamSimpleMousePreset;
pub use validate::validate_bindings;

/// Validated runtime binding specification for an `OrbitCam`.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[reflect(opaque)]
pub struct OrbitCamBindings {
    pub(super) orbit:            OrbitCamOrbitActionBindings,
    pub(super) pan:              OrbitCamPanActionBindings,
    pub(super) zoom_smooth:      OrbitCamZoomSmoothActionBindings,
    pub(super) zoom_coarse:      OrbitCamZoomCoarseActionBindings,
    pub(super) trackpad_orbit:   Vec<OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>>,
    pub(super) trackpad_pan:     Vec<OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>>,
    pub(super) trackpad_zoom:    Vec<OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>>,
    pub(super) mouse_wheel_zoom: Option<OrbitCamBindingWithSensitivity<OrbitCamMouseWheelZoom>>,
    pub(super) pinch_zoom:       Option<OrbitCamBindingWithSensitivity<OrbitCamPinchZoom>>,
    pub(super) touch:            Option<OrbitCamTouchBindingConfig>,
    pub(super) gamepad:          CameraInputGamepadSelectionPolicy,
    pub(super) zoom_inversion:   ZoomInversion,
    pub(super) button_drag_zoom: Option<OrbitCamBindingWithSensitivity<OrbitCamButtonDragZoom>>,
    pub(super) slow_mode:        Option<OrbitCamSlowMode>,
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
    pub fn trackpad_orbit(&self) -> &[OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>] {
        &self.trackpad_orbit
    }

    /// Returns trackpad pan bindings.
    #[must_use]
    pub fn trackpad_pan(&self) -> &[OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>] {
        &self.trackpad_pan
    }

    /// Returns trackpad zoom bindings.
    #[must_use]
    pub fn trackpad_zoom(&self) -> &[OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>] {
        &self.trackpad_zoom
    }

    /// Returns mouse wheel zoom binding.
    #[must_use]
    pub const fn mouse_wheel_zoom(
        &self,
    ) -> Option<OrbitCamBindingWithSensitivity<OrbitCamMouseWheelZoom>> {
        self.mouse_wheel_zoom
    }

    /// Returns whether pinch zoom is enabled.
    #[must_use]
    pub const fn pinch_zoom(&self) -> PinchGestureZoom {
        match self.pinch_zoom {
            Some(_) => PinchGestureZoom::Enabled,
            None => PinchGestureZoom::Disabled,
        }
    }

    /// Returns pinch zoom binding plus authored sensitivity.
    #[must_use]
    pub const fn pinch_zoom_binding(
        &self,
    ) -> Option<OrbitCamBindingWithSensitivity<OrbitCamPinchZoom>> {
        self.pinch_zoom
    }

    /// Returns touch policy.
    #[must_use]
    pub const fn touch(&self) -> Option<OrbitCamTouchBinding> {
        match self.touch {
            Some(touch) => Some(touch.binding()),
            None => None,
        }
    }

    /// Returns touch policy plus authored sensitivity.
    #[must_use]
    pub const fn touch_config(&self) -> Option<OrbitCamTouchBindingConfig> { self.touch }

    /// Returns gamepad selection policy.
    #[must_use]
    pub const fn gamepad(&self) -> CameraInputGamepadSelectionPolicy { self.gamepad }

    /// Returns the zoom inversion policy.
    #[must_use]
    pub const fn zoom_inversion(&self) -> ZoomInversion { self.zoom_inversion }

    /// Returns button-drag zoom policy.
    #[must_use]
    pub const fn button_drag_zoom(
        &self,
    ) -> Option<OrbitCamBindingWithSensitivity<OrbitCamButtonDragZoom>> {
        self.button_drag_zoom
    }

    /// Returns the slow-mode policy.
    #[must_use]
    pub const fn slow_mode(&self) -> Option<&OrbitCamSlowMode> { self.slow_mode.as_ref() }
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
    use crate::input::ControlSpeed;
    use crate::input::constants::ORBIT_ACTION_NAME;
    use crate::input::constants::PAN_ACTION_NAME;
    use crate::input::constants::ZOOM_COARSE_ACTION_NAME;

    const BUTTON_DRAG_SENSITIVITY: f32 = 0.6;
    const CUSTOM_DEFAULT_SENSITIVITY: f32 = InputSensitivity::DEFAULT.0;
    const DISABLED_SENSITIVITY: f32 = InputSensitivity::DISABLED.0;
    const INVALID_NEGATIVE_SENSITIVITY: f32 = -0.01;
    const MOUSE_DRAG_SENSITIVITY: f32 = 0.2;
    const PINCH_SENSITIVITY: f32 = 0.5;
    const TOUCH_ORBIT_SENSITIVITY: f32 = 0.7;
    const TOUCH_PAN_SENSITIVITY: f32 = 0.8;
    const TOUCH_ZOOM_SENSITIVITY: f32 = 0.9;
    const TRACKPAD_SENSITIVITY: f32 = 0.3;
    const WHEEL_SENSITIVITY: f32 = 0.25;

    fn descriptor_with_no_bindings() -> OrbitCamBindingsDescriptor {
        OrbitCamBindingsDescriptor::default()
    }

    #[test]
    fn presets_validate_through_shared_path() -> Result<(), OrbitCamBindingsError> {
        let simple = OrbitCamPreset::simple_mouse().to_bindings()?;
        assert!(simple.mouse_wheel_zoom().is_some());
        assert_eq!(simple.trackpad_zoom().len(), 1);
        assert_eq!(simple.pinch_zoom(), PinchGestureZoom::Enabled);
        assert!(simple.touch().is_none());
        assert!(simple.slow_mode().is_none());

        let blender = OrbitCamPreset::blender_like().to_bindings()?;
        assert_eq!(blender.orbit().len(), 1);
        assert_eq!(blender.pan().len(), 1);
        assert_eq!(blender.trackpad_orbit().len(), 1);
        assert_eq!(blender.trackpad_pan().len(), 1);
        assert_eq!(blender.trackpad_zoom().len(), 1);
        assert!(blender.mouse_wheel_zoom().is_some());
        assert_eq!(blender.pinch_zoom(), PinchGestureZoom::Enabled);
        assert!(blender.slow_mode().is_some());

        let [pan] = blender.pan().entries() else {
            assert_eq!(blender.pan().entries().len(), 1);
            return Ok(());
        };
        assert_eq!(
            pan.engagement_descriptor().mouse_button_engagement(),
            Some((MouseButton::Middle, ModKeys::SHIFT))
        );

        let keyboard = OrbitCamPreset::keyboard().to_bindings()?;
        assert_eq!(keyboard.orbit().len(), 1);
        assert_eq!(keyboard.pan().len(), 1);
        assert_eq!(keyboard.zoom_smooth().len(), 1);
        assert!(keyboard.slow_mode().is_none());

        let blender_keyboard = OrbitCamBlenderLikeKeyboardPreset::default().build()?;
        assert!(blender_keyboard.slow_mode().is_some());

        let gamepad = OrbitCamPreset::gamepad().to_bindings()?;
        assert_eq!(gamepad.gamepad(), CameraInputGamepadSelectionPolicy::Active);
        assert_eq!(gamepad.orbit().len(), 2);
        assert_eq!(gamepad.pan().len(), 2);
        assert_eq!(gamepad.zoom_smooth().len(), 4);
        assert!(gamepad.slow_mode().is_none());

        Ok(())
    }

    #[test]
    fn blender_like_default_builds_slow_mode() -> Result<(), OrbitCamBindingsError> {
        let bindings = OrbitCamBlenderLikePreset::default().build()?;

        assert_eq!(
            bindings
                .slow_mode()
                .map(|slow_mode| (slow_mode.toggle_key, slow_mode.mod_keys)),
            Some((KeyCode::KeyS, ModKeys::ALT))
        );
        Ok(())
    }

    #[test]
    fn blender_like_preset_validates_slow_scale() {
        assert!(
            OrbitCamBlenderLikePreset::default()
                .slow_scale(0.25)
                .build()
                .is_ok()
        );
        assert_eq!(
            OrbitCamBlenderLikePreset::default().slow_scale(2.0).build(),
            Err(OrbitCamBindingsError::InvalidScale)
        );
    }

    #[test]
    fn gamepad_preset_rejects_slow_scales_above_fast_scales() {
        assert_eq!(
            OrbitCamGamepadPreset::default()
                .customize()
                .slow_orbit_scale(2000.0)
                .build(),
            Err(OrbitCamBindingsError::InvalidScale)
        );
    }

    #[test]
    fn preset_enum_delegates_to_blender_like_config() -> Result<(), OrbitCamBindingsError> {
        assert_eq!(
            OrbitCamPreset::blender_like().to_bindings()?,
            OrbitCamBlenderLikePreset::default().build()?
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
    fn invalid_binding_sensitivity_is_rejected() {
        for sensitivity in [
            INVALID_NEGATIVE_SENSITIVITY,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            assert_eq!(
                OrbitCamBindings::builder()
                    .zoom(OrbitCamMouseWheelZoom.with_sensitivity(sensitivity))
                    .build(),
                Err(OrbitCamBindingsError::InvalidScale)
            );
        }
    }

    #[test]
    fn zero_binding_sensitivity_is_preserved() -> Result<(), OrbitCamBindingsError> {
        let bindings = OrbitCamBindings::builder()
            .zoom(OrbitCamMouseWheelZoom.with_sensitivity(DISABLED_SENSITIVITY))
            .build()?;

        let Some(wheel) = bindings.mouse_wheel_zoom() else {
            assert!(bindings.mouse_wheel_zoom().is_some());
            return Ok(());
        };
        assert_eq!(wheel.sensitivity(), InputSensitivity::DISABLED);

        Ok(())
    }

    #[test]
    fn default_sensitivity_matches_implicit_custom_bindings() -> Result<(), OrbitCamBindingsError> {
        let implicit = OrbitCamBindings::builder()
            .orbit(OrbitCamMouseDrag::new(MouseButton::Middle))
            .zoom(OrbitCamMouseWheelZoom)
            .build()?;
        let explicit = OrbitCamBindings::builder()
            .orbit(
                OrbitCamMouseDrag::new(MouseButton::Middle)
                    .with_sensitivity(CUSTOM_DEFAULT_SENSITIVITY),
            )
            .zoom(OrbitCamMouseWheelZoom.with_sensitivity(CUSTOM_DEFAULT_SENSITIVITY))
            .build()?;

        assert_eq!(implicit, explicit);

        Ok(())
    }

    #[test]
    fn adapter_backed_bindings_preserve_authored_sensitivity() -> Result<(), OrbitCamBindingsError>
    {
        let touch_sensitivity = OrbitCamSensitivity::new()
            .orbit(TOUCH_ORBIT_SENSITIVITY)
            .pan(TOUCH_PAN_SENSITIVITY)
            .zoom(TOUCH_ZOOM_SENSITIVITY);

        let bindings = OrbitCamBindings::builder()
            .orbit(OrbitCamTrackpadScroll::default().with_sensitivity(TRACKPAD_SENSITIVITY))
            .zoom(OrbitCamMouseWheelZoom.with_sensitivity(WHEEL_SENSITIVITY))
            .zoom(
                OrbitCamTrackpadScroll::default()
                    .with_mod_keys(ModKeys::CONTROL)
                    .with_sensitivity(TRACKPAD_SENSITIVITY),
            )
            .zoom(OrbitCamPinchZoom.with_sensitivity(PINCH_SENSITIVITY))
            .zoom(
                OrbitCamButtonDragZoom::new(MouseButton::Middle)
                    .with_sensitivity(BUTTON_DRAG_SENSITIVITY),
            )
            .touch_config(Some(
                OrbitCamTouchBinding::OneFingerOrbit.with_sensitivity(touch_sensitivity),
            ))
            .build()?;

        let [trackpad_orbit] = bindings.trackpad_orbit() else {
            assert_eq!(bindings.trackpad_orbit().len(), 1);
            return Ok(());
        };
        assert_eq!(
            trackpad_orbit.sensitivity(),
            InputSensitivity(TRACKPAD_SENSITIVITY)
        );

        let Some(wheel) = bindings.mouse_wheel_zoom() else {
            assert!(bindings.mouse_wheel_zoom().is_some());
            return Ok(());
        };
        assert_eq!(wheel.sensitivity(), InputSensitivity(WHEEL_SENSITIVITY));

        let [trackpad_zoom] = bindings.trackpad_zoom() else {
            assert_eq!(bindings.trackpad_zoom().len(), 1);
            return Ok(());
        };
        assert_eq!(trackpad_zoom.binding().mod_keys, ModKeys::CONTROL);
        assert_eq!(
            trackpad_zoom.sensitivity(),
            InputSensitivity(TRACKPAD_SENSITIVITY)
        );

        let Some(pinch) = bindings.pinch_zoom_binding() else {
            assert!(bindings.pinch_zoom_binding().is_some());
            return Ok(());
        };
        assert_eq!(pinch.sensitivity(), InputSensitivity(PINCH_SENSITIVITY));

        let Some(button_drag) = bindings.button_drag_zoom() else {
            assert!(bindings.button_drag_zoom().is_some());
            return Ok(());
        };
        assert_eq!(button_drag.binding().button, MouseButton::Middle);
        assert_eq!(
            button_drag.sensitivity(),
            InputSensitivity(BUTTON_DRAG_SENSITIVITY)
        );

        let Some(touch) = bindings.touch_config() else {
            assert!(bindings.touch_config().is_some());
            return Ok(());
        };
        assert_eq!(touch.binding(), OrbitCamTouchBinding::OneFingerOrbit);
        assert_eq!(touch.sensitivity(), touch_sensitivity);

        Ok(())
    }

    #[test]
    fn sensitivity_and_modifier_setter_order_matches() {
        assert_eq!(
            OrbitCamTrackpadScroll::default()
                .with_sensitivity(TRACKPAD_SENSITIVITY)
                .with_mod_keys(ModKeys::SHIFT),
            OrbitCamTrackpadScroll::default()
                .with_mod_keys(ModKeys::SHIFT)
                .with_sensitivity(TRACKPAD_SENSITIVITY)
        );
        assert_eq!(
            OrbitCamMouseDrag::new(MouseButton::Middle)
                .with_sensitivity(MOUSE_DRAG_SENSITIVITY)
                .with_mod_keys(ModKeys::ALT),
            OrbitCamMouseDrag::new(MouseButton::Middle)
                .with_mod_keys(ModKeys::ALT)
                .with_sensitivity(MOUSE_DRAG_SENSITIVITY)
        );
    }

    #[test]
    fn held_motion_without_engagement_is_rejected() {
        let mut descriptor = descriptor_with_no_bindings();
        descriptor.orbit.push(HeldBindingDescriptor {
            motion:             OrbitCamInputBinding::from(Binding::mouse_motion()).descriptor(),
            engagement:         None,
            gates:              BindingGates::default(),
            sources:            CameraInteractionSources::MOUSE,
            engagement_sources: CameraInteractionSources::MOUSE,
            route:              BindingRoutePolicy::CursorPosition,
            speed:              ControlSpeed::Normal,
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
            gates:              BindingGates::default(),
            sources:            CameraInteractionSources::MOUSE,
            engagement_sources: CameraInteractionSources::KEYBOARD,
            route:              BindingRoutePolicy::CursorPosition,
            speed:              ControlSpeed::Normal,
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
