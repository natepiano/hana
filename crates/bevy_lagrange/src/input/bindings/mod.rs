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
pub use builder::OrbitCamBindingWithInputGain;
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
pub use descriptor::ActionBindingDescriptor;
pub use descriptor::InputAxisTransform;
pub use descriptor::InputBindingDescriptor;
pub use descriptor::InputBindingEntry;
pub use descriptor::InputBindingModifiers;
pub use descriptor::InputBindingScale;
pub use descriptor::InputDeadZone;
pub use descriptor::InputDeltaScale;
pub use descriptor::InputGain;
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
pub use preset::GamepadSensitivity;
pub use preset::MouseSensitivity;
pub use preset::OrbitCamBlenderLikeKeyboardPreset;
pub use preset::OrbitCamBlenderLikePreset;
pub use preset::OrbitCamGamepadPreset;
pub use preset::OrbitCamGamepadPresetBuilder;
pub use preset::OrbitCamKeyboardPreset;
pub use preset::OrbitCamPreset;
pub use preset::OrbitCamPresetKind;
pub use preset::OrbitCamSimpleMouseKeyboardPreset;
pub use preset::OrbitCamSimpleMousePreset;
pub use preset::SmoothScrollSensitivity;
pub use validate::validate_bindings;

/// Validated runtime binding specification for an `OrbitCam`.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[reflect(opaque)]
pub struct OrbitCamBindings {
    pub(super) orbit:            OrbitCamOrbitActionBindings,
    pub(super) pan:              OrbitCamPanActionBindings,
    pub(super) zoom_smooth:      OrbitCamZoomSmoothActionBindings,
    pub(super) zoom_coarse:      OrbitCamZoomCoarseActionBindings,
    pub(super) trackpad_orbit:   Vec<OrbitCamBindingWithInputGain<OrbitCamTrackpadScroll>>,
    pub(super) trackpad_pan:     Vec<OrbitCamBindingWithInputGain<OrbitCamTrackpadScroll>>,
    pub(super) trackpad_zoom:    Vec<OrbitCamBindingWithInputGain<OrbitCamTrackpadScroll>>,
    pub(super) mouse_wheel_zoom: Option<OrbitCamBindingWithInputGain<OrbitCamMouseWheelZoom>>,
    pub(super) pinch_zoom:       Option<OrbitCamBindingWithInputGain<OrbitCamPinchZoom>>,
    pub(super) touch:            Option<OrbitCamTouchBindingConfig>,
    pub(super) gamepad:          CameraInputGamepadSelectionPolicy,
    pub(super) zoom_inversion:   ZoomInversion,
    pub(super) button_drag_zoom: Option<OrbitCamBindingWithInputGain<OrbitCamButtonDragZoom>>,
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
    pub fn trackpad_orbit(&self) -> &[OrbitCamBindingWithInputGain<OrbitCamTrackpadScroll>] {
        &self.trackpad_orbit
    }

    /// Returns trackpad orbit bindings that participate in runtime input, with
    /// their authored indexes preserved.
    pub(crate) fn enabled_trackpad_orbit(
        &self,
    ) -> impl Iterator<Item = (usize, OrbitCamBindingWithInputGain<OrbitCamTrackpadScroll>)> + '_
    {
        enabled_sensitive_entries(&self.trackpad_orbit)
    }

    /// Returns trackpad pan bindings.
    #[must_use]
    pub fn trackpad_pan(&self) -> &[OrbitCamBindingWithInputGain<OrbitCamTrackpadScroll>] {
        &self.trackpad_pan
    }

    /// Returns trackpad pan bindings that participate in runtime input, with
    /// their authored indexes preserved.
    pub(crate) fn enabled_trackpad_pan(
        &self,
    ) -> impl Iterator<Item = (usize, OrbitCamBindingWithInputGain<OrbitCamTrackpadScroll>)> + '_
    {
        enabled_sensitive_entries(&self.trackpad_pan)
    }

    /// Returns trackpad zoom bindings.
    #[must_use]
    pub fn trackpad_zoom(&self) -> &[OrbitCamBindingWithInputGain<OrbitCamTrackpadScroll>] {
        &self.trackpad_zoom
    }

    /// Returns trackpad zoom bindings that participate in runtime input, with
    /// their authored indexes preserved.
    pub(crate) fn enabled_trackpad_zoom(
        &self,
    ) -> impl Iterator<Item = (usize, OrbitCamBindingWithInputGain<OrbitCamTrackpadScroll>)> + '_
    {
        enabled_sensitive_entries(&self.trackpad_zoom)
    }

    /// Returns mouse wheel zoom binding.
    #[must_use]
    pub const fn mouse_wheel_zoom(
        &self,
    ) -> Option<OrbitCamBindingWithInputGain<OrbitCamMouseWheelZoom>> {
        self.mouse_wheel_zoom
    }

    /// Returns mouse wheel zoom binding when it participates in runtime input.
    #[must_use]
    pub(crate) const fn enabled_mouse_wheel_zoom(
        &self,
    ) -> Option<OrbitCamBindingWithInputGain<OrbitCamMouseWheelZoom>> {
        enabled_sensitive_option(self.mouse_wheel_zoom)
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
    ) -> Option<OrbitCamBindingWithInputGain<OrbitCamPinchZoom>> {
        self.pinch_zoom
    }

    /// Returns pinch zoom binding when it participates in runtime input.
    #[must_use]
    pub(crate) const fn enabled_pinch_zoom_binding(
        &self,
    ) -> Option<OrbitCamBindingWithInputGain<OrbitCamPinchZoom>> {
        enabled_sensitive_option(self.pinch_zoom)
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

    /// Returns touch policy plus sensitivity when any touch action participates
    /// in runtime input.
    #[must_use]
    pub(crate) const fn enabled_touch_config(&self) -> Option<OrbitCamTouchBindingConfig> {
        match self.touch {
            Some(touch) if touch.has_enabled_action() => Some(touch),
            Some(_) | None => None,
        }
    }

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
    ) -> Option<OrbitCamBindingWithInputGain<OrbitCamButtonDragZoom>> {
        self.button_drag_zoom
    }

    /// Returns button-drag zoom policy when it participates in runtime input.
    #[must_use]
    pub(crate) const fn enabled_button_drag_zoom(
        &self,
    ) -> Option<OrbitCamBindingWithInputGain<OrbitCamButtonDragZoom>> {
        enabled_sensitive_option(self.button_drag_zoom)
    }

    /// Returns the slow-mode policy.
    #[must_use]
    pub const fn slow_mode(&self) -> Option<&OrbitCamSlowMode> { self.slow_mode.as_ref() }
}

fn enabled_sensitive_entries<T: Copy>(
    entries: &[OrbitCamBindingWithInputGain<T>],
) -> impl Iterator<Item = (usize, OrbitCamBindingWithInputGain<T>)> + '_ {
    entries
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, entry)| entry.sensitivity().is_enabled())
}

const fn enabled_sensitive_option<T: Copy>(
    entry: Option<OrbitCamBindingWithInputGain<T>>,
) -> Option<OrbitCamBindingWithInputGain<T>> {
    match entry {
        Some(entry) if entry.sensitivity().is_enabled() => Some(entry),
        Some(_) | None => None,
    }
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
    use crate::input::HeldCameraAction;
    use crate::input::constants::DISABLED_SENSITIVITY;
    use crate::input::constants::ORBIT_ACTION_NAME;
    use crate::input::constants::PAN_ACTION_NAME;
    use crate::input::constants::PINCH_SENSITIVITY;
    use crate::input::constants::WHEEL_SENSITIVITY;
    use crate::input::constants::ZOOM_COARSE_ACTION_NAME;

    const BUTTON_DRAG_SENSITIVITY: f32 = 0.6;
    const CUSTOM_DEFAULT_SENSITIVITY: f32 = InputGain::DEFAULT.0;
    const GAMEPAD_SOURCE_SENSITIVITY: f32 = 0.5;
    const GAMEPAD_TUNED_ORBIT_SCALE: f32 = 321.0;
    const INVALID_NEGATIVE_SENSITIVITY: f32 = -0.01;
    const MOUSE_ORBIT_SENSITIVITY: f32 = 0.2;
    const MOUSE_PAN_SENSITIVITY: f32 = 0.3;
    const MOUSE_ZOOM_SENSITIVITY: f32 = 0.4;
    const MOUSE_DRAG_SENSITIVITY: f32 = 0.2;
    const REPLACEMENT_SENSITIVITY: f32 = 0.55;
    const SMOOTH_SCROLL_ORBIT_SENSITIVITY: f32 = 0.6;
    const SMOOTH_SCROLL_PAN_SENSITIVITY: f32 = 0.7;
    const SMOOTH_SCROLL_ZOOM_SENSITIVITY: f32 = 0.8;
    const TOUCH_ORBIT_SENSITIVITY: f32 = 0.7;
    const TOUCH_PAN_SENSITIVITY: f32 = 0.8;
    const TOUCH_ZOOM_SENSITIVITY: f32 = 0.9;
    const TRACKPAD_SENSITIVITY: f32 = 0.3;

    fn descriptor_with_no_bindings() -> OrbitCamBindingsDescriptor {
        OrbitCamBindingsDescriptor::default()
    }

    fn first_motion_sensitivity<A: HeldCameraAction>(
        entry: &HeldActionBindingEntry<A>,
    ) -> Option<InputGain> {
        entry
            .motion_descriptor()
            .entries_slice()
            .first()
            .map(InputBindingEntry::sensitivity)
    }

    fn first_motion_install_scale<A: HeldCameraAction>(
        entry: &HeldActionBindingEntry<A>,
    ) -> Option<f32> {
        entry
            .motion_descriptor()
            .entries_slice()
            .first()
            .and_then(|entry| entry.install_modifiers().scale())
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
    fn default_simple_mouse_preset_matches_implicit_bindings() -> Result<(), OrbitCamBindingsError>
    {
        let preset = OrbitCamPreset::simple_mouse().to_bindings()?;
        let expected = OrbitCamBindings::builder()
            .orbit(OrbitCamMouseDrag::new(MouseButton::Left))
            .pan(OrbitCamMouseDrag::new(MouseButton::Right))
            .zoom(OrbitCamMouseWheelZoom)
            .zoom(OrbitCamTrackpadScroll::default())
            .zoom(OrbitCamPinchZoom)
            .build()?;

        assert_eq!(preset, expected);

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
    fn tuned_simple_mouse_preset_lowers_pointer_source_sensitivity()
    -> Result<(), OrbitCamBindingsError> {
        let mouse_sensitivity = OrbitCamSensitivity::new()
            .orbit(MOUSE_ORBIT_SENSITIVITY)
            .pan(MOUSE_PAN_SENSITIVITY)
            .zoom(MOUSE_ZOOM_SENSITIVITY);
        let smooth_scroll_sensitivity =
            OrbitCamSensitivity::new().zoom(SMOOTH_SCROLL_ZOOM_SENSITIVITY);
        let bindings = OrbitCamSimpleMousePreset::default()
            .mouse_sensitivity(mouse_sensitivity)
            .smooth_scroll_sensitivity(smooth_scroll_sensitivity)
            .build()?;

        let [orbit] = bindings.orbit().entries() else {
            assert_eq!(bindings.orbit().entries().len(), 1);
            return Ok(());
        };
        assert_eq!(
            first_motion_sensitivity(orbit),
            Some(InputGain(MOUSE_ORBIT_SENSITIVITY))
        );

        let [pan] = bindings.pan().entries() else {
            assert_eq!(bindings.pan().entries().len(), 1);
            return Ok(());
        };
        assert_eq!(
            first_motion_sensitivity(pan),
            Some(InputGain(MOUSE_PAN_SENSITIVITY))
        );

        let Some(wheel) = bindings.mouse_wheel_zoom() else {
            assert!(bindings.mouse_wheel_zoom().is_some());
            return Ok(());
        };
        assert_eq!(wheel.sensitivity(), InputGain(MOUSE_ZOOM_SENSITIVITY));

        let [smooth_scroll_zoom] = bindings.trackpad_zoom() else {
            assert_eq!(bindings.trackpad_zoom().len(), 1);
            return Ok(());
        };
        assert_eq!(
            smooth_scroll_zoom.sensitivity(),
            InputGain(SMOOTH_SCROLL_ZOOM_SENSITIVITY)
        );

        let Some(pinch) = bindings.pinch_zoom_binding() else {
            assert!(bindings.pinch_zoom_binding().is_some());
            return Ok(());
        };
        assert_eq!(pinch.sensitivity(), InputGain::DEFAULT);

        Ok(())
    }

    #[test]
    fn tuned_blender_like_preset_lowers_mouse_and_smooth_scroll_sensitivity()
    -> Result<(), OrbitCamBindingsError> {
        let mouse_sensitivity = OrbitCamSensitivity::new()
            .orbit(MOUSE_ORBIT_SENSITIVITY)
            .pan(MOUSE_PAN_SENSITIVITY)
            .zoom(MOUSE_ZOOM_SENSITIVITY);
        let smooth_scroll_sensitivity = OrbitCamSensitivity::new()
            .orbit(SMOOTH_SCROLL_ORBIT_SENSITIVITY)
            .pan(SMOOTH_SCROLL_PAN_SENSITIVITY)
            .zoom(SMOOTH_SCROLL_ZOOM_SENSITIVITY);
        let bindings = OrbitCamBlenderLikePreset::default()
            .mouse_sensitivity(mouse_sensitivity)
            .smooth_scroll_sensitivity(smooth_scroll_sensitivity)
            .build()?;

        let [orbit] = bindings.orbit().entries() else {
            assert_eq!(bindings.orbit().entries().len(), 1);
            return Ok(());
        };
        assert_eq!(
            first_motion_sensitivity(orbit),
            Some(InputGain(MOUSE_ORBIT_SENSITIVITY))
        );

        let [smooth_scroll_orbit] = bindings.trackpad_orbit() else {
            assert_eq!(bindings.trackpad_orbit().len(), 1);
            return Ok(());
        };
        assert_eq!(
            smooth_scroll_orbit.sensitivity(),
            InputGain(SMOOTH_SCROLL_ORBIT_SENSITIVITY)
        );

        let [pan] = bindings.pan().entries() else {
            assert_eq!(bindings.pan().entries().len(), 1);
            return Ok(());
        };
        assert_eq!(
            first_motion_sensitivity(pan),
            Some(InputGain(MOUSE_PAN_SENSITIVITY))
        );

        let [smooth_scroll_pan] = bindings.trackpad_pan() else {
            assert_eq!(bindings.trackpad_pan().len(), 1);
            return Ok(());
        };
        assert_eq!(
            smooth_scroll_pan.sensitivity(),
            InputGain(SMOOTH_SCROLL_PAN_SENSITIVITY)
        );

        let Some(wheel) = bindings.mouse_wheel_zoom() else {
            assert!(bindings.mouse_wheel_zoom().is_some());
            return Ok(());
        };
        assert_eq!(wheel.sensitivity(), InputGain(MOUSE_ZOOM_SENSITIVITY));

        let [smooth_scroll_zoom] = bindings.trackpad_zoom() else {
            assert_eq!(bindings.trackpad_zoom().len(), 1);
            return Ok(());
        };
        assert_eq!(
            smooth_scroll_zoom.sensitivity(),
            InputGain(SMOOTH_SCROLL_ZOOM_SENSITIVITY)
        );

        let Some(pinch) = bindings.pinch_zoom_binding() else {
            assert!(bindings.pinch_zoom_binding().is_some());
            return Ok(());
        };
        assert_eq!(pinch.sensitivity(), InputGain::DEFAULT);

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
    fn gamepad_preset_validates_source_sensitivity() {
        assert_eq!(
            OrbitCamGamepadPreset::default()
                .gamepad_sensitivity(OrbitCamSensitivity::uniform(INVALID_NEGATIVE_SENSITIVITY))
                .build(),
            Err(OrbitCamBindingsError::InvalidScale)
        );
    }

    #[test]
    fn zero_gamepad_source_sensitivity_preserves_payload_and_disables_runtime_entries()
    -> Result<(), OrbitCamBindingsError> {
        let gamepad_preset = OrbitCamGamepadPreset::default()
            .gamepad_sensitivity(OrbitCamSensitivity::uniform(DISABLED_SENSITIVITY));
        let preset = OrbitCamPreset::from(gamepad_preset);

        assert_eq!(preset, OrbitCamPreset::Gamepad(gamepad_preset));

        let bindings = preset.to_bindings()?;
        assert_eq!(bindings.orbit().enabled_entries().count(), 0);
        assert_eq!(bindings.pan().enabled_entries().count(), 0);
        assert_eq!(bindings.zoom_smooth().enabled_entries().count(), 0);
        assert_eq!(
            bindings.gamepad(),
            CameraInputGamepadSelectionPolicy::Active
        );

        Ok(())
    }

    #[test]
    fn gamepad_scale_tuning_stays_in_preset_payload() -> Result<(), OrbitCamBindingsError> {
        let gamepad_preset = OrbitCamGamepadPreset::default()
            .customize()
            .orbit_scale(GAMEPAD_TUNED_ORBIT_SCALE)
            .into_preset();
        let preset = OrbitCamPreset::from(gamepad_preset);

        assert_eq!(preset, OrbitCamPreset::Gamepad(gamepad_preset));

        let bindings = preset.to_bindings()?;
        let [fast_orbit, _slow_orbit] = bindings.orbit().entries() else {
            assert_eq!(bindings.orbit().entries().len(), 2);
            return Ok(());
        };
        assert_eq!(
            first_motion_install_scale(fast_orbit),
            Some(GAMEPAD_TUNED_ORBIT_SCALE)
        );

        Ok(())
    }

    #[test]
    fn gamepad_source_sensitivity_scales_normal_and_slow_entries()
    -> Result<(), OrbitCamBindingsError> {
        let default = OrbitCamGamepadPreset::default().build()?;
        let tuned = OrbitCamGamepadPreset::default()
            .gamepad_sensitivity(OrbitCamSensitivity::uniform(GAMEPAD_SOURCE_SENSITIVITY))
            .build()?;

        let [default_fast_orbit, default_slow_orbit] = default.orbit().entries() else {
            assert_eq!(default.orbit().entries().len(), 2);
            return Ok(());
        };
        let [tuned_fast_orbit, tuned_slow_orbit] = tuned.orbit().entries() else {
            assert_eq!(tuned.orbit().entries().len(), 2);
            return Ok(());
        };
        assert_eq!(tuned_fast_orbit.speed(), ControlSpeed::Normal);
        assert_eq!(tuned_slow_orbit.speed(), ControlSpeed::Slow);
        assert_eq!(
            first_motion_install_scale(tuned_fast_orbit),
            first_motion_install_scale(default_fast_orbit)
                .map(|scale| scale * GAMEPAD_SOURCE_SENSITIVITY)
        );
        assert_eq!(
            first_motion_install_scale(tuned_slow_orbit),
            first_motion_install_scale(default_slow_orbit)
                .map(|scale| scale * GAMEPAD_SOURCE_SENSITIVITY)
        );

        let [default_fast_pan, default_slow_pan] = default.pan().entries() else {
            assert_eq!(default.pan().entries().len(), 2);
            return Ok(());
        };
        let [tuned_fast_pan, tuned_slow_pan] = tuned.pan().entries() else {
            assert_eq!(tuned.pan().entries().len(), 2);
            return Ok(());
        };
        assert_eq!(tuned_fast_pan.speed(), ControlSpeed::Normal);
        assert_eq!(tuned_slow_pan.speed(), ControlSpeed::Slow);
        assert_eq!(
            first_motion_install_scale(tuned_fast_pan),
            first_motion_install_scale(default_fast_pan)
                .map(|scale| scale * GAMEPAD_SOURCE_SENSITIVITY)
        );
        assert_eq!(
            first_motion_install_scale(tuned_slow_pan),
            first_motion_install_scale(default_slow_pan)
                .map(|scale| scale * GAMEPAD_SOURCE_SENSITIVITY)
        );

        let [
            default_zoom_in,
            default_zoom_out,
            default_slow_zoom_in,
            default_slow_zoom_out,
        ] = default.zoom_smooth().entries()
        else {
            assert_eq!(default.zoom_smooth().entries().len(), 4);
            return Ok(());
        };
        let [
            tuned_zoom_in,
            tuned_zoom_out,
            tuned_slow_zoom_in,
            tuned_slow_zoom_out,
        ] = tuned.zoom_smooth().entries()
        else {
            assert_eq!(tuned.zoom_smooth().entries().len(), 4);
            return Ok(());
        };
        for entry in [tuned_zoom_in, tuned_zoom_out] {
            assert_eq!(entry.speed(), ControlSpeed::Normal);
        }
        for entry in [tuned_slow_zoom_in, tuned_slow_zoom_out] {
            assert_eq!(entry.speed(), ControlSpeed::Slow);
        }
        for (tuned, default) in [
            (tuned_zoom_in, default_zoom_in),
            (tuned_zoom_out, default_zoom_out),
            (tuned_slow_zoom_in, default_slow_zoom_in),
            (tuned_slow_zoom_out, default_slow_zoom_out),
        ] {
            assert_eq!(
                first_motion_install_scale(tuned),
                first_motion_install_scale(default).map(|scale| scale * GAMEPAD_SOURCE_SENSITIVITY)
            );
        }

        Ok(())
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
        assert_eq!(wheel.sensitivity(), InputGain::DISABLED);

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
            InputGain(TRACKPAD_SENSITIVITY)
        );

        let Some(wheel) = bindings.mouse_wheel_zoom() else {
            assert!(bindings.mouse_wheel_zoom().is_some());
            return Ok(());
        };
        assert_eq!(wheel.sensitivity(), InputGain(WHEEL_SENSITIVITY));

        let [trackpad_zoom] = bindings.trackpad_zoom() else {
            assert_eq!(bindings.trackpad_zoom().len(), 1);
            return Ok(());
        };
        assert_eq!(trackpad_zoom.binding().mod_keys, ModKeys::CONTROL);
        assert_eq!(trackpad_zoom.sensitivity(), InputGain(TRACKPAD_SENSITIVITY));

        let Some(pinch) = bindings.pinch_zoom_binding() else {
            assert!(bindings.pinch_zoom_binding().is_some());
            return Ok(());
        };
        assert_eq!(pinch.sensitivity(), InputGain(PINCH_SENSITIVITY));

        let Some(button_drag) = bindings.button_drag_zoom() else {
            assert!(bindings.button_drag_zoom().is_some());
            return Ok(());
        };
        assert_eq!(button_drag.binding().button, MouseButton::Middle);
        assert_eq!(
            button_drag.sensitivity(),
            InputGain(BUTTON_DRAG_SENSITIVITY)
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
    fn adapter_enabled_views_filter_disabled_entries() -> Result<(), OrbitCamBindingsError> {
        let touch_sensitivity = OrbitCamSensitivity::new()
            .orbit(DISABLED_SENSITIVITY)
            .pan(TOUCH_PAN_SENSITIVITY)
            .zoom(DISABLED_SENSITIVITY);
        let bindings = OrbitCamBindings::builder()
            .orbit(OrbitCamTrackpadScroll::default().with_sensitivity(DISABLED_SENSITIVITY))
            .orbit(
                OrbitCamTrackpadScroll::default()
                    .with_mod_keys(ModKeys::SHIFT)
                    .with_sensitivity(TRACKPAD_SENSITIVITY),
            )
            .pan(OrbitCamTrackpadScroll::default().with_sensitivity(DISABLED_SENSITIVITY))
            .zoom(OrbitCamTrackpadScroll::default().with_sensitivity(DISABLED_SENSITIVITY))
            .zoom(
                OrbitCamTrackpadScroll::default()
                    .with_mod_keys(ModKeys::CONTROL)
                    .with_sensitivity(TRACKPAD_SENSITIVITY),
            )
            .zoom(OrbitCamMouseWheelZoom.with_sensitivity(DISABLED_SENSITIVITY))
            .zoom(OrbitCamPinchZoom.with_sensitivity(DISABLED_SENSITIVITY))
            .zoom(
                OrbitCamButtonDragZoom::new(MouseButton::Middle)
                    .with_sensitivity(DISABLED_SENSITIVITY),
            )
            .touch_config(Some(
                OrbitCamTouchBinding::OneFingerOrbit.with_sensitivity(touch_sensitivity),
            ))
            .build()?;

        let enabled_orbit = bindings.enabled_trackpad_orbit().collect::<Vec<_>>();
        let enabled_pan_count = bindings.enabled_trackpad_pan().count();
        let enabled_zoom = bindings.enabled_trackpad_zoom().collect::<Vec<_>>();

        assert_eq!(bindings.trackpad_orbit().len(), 2);
        assert_eq!(enabled_orbit.len(), 1);
        assert_eq!(enabled_orbit[0].0, 1);
        assert_eq!(enabled_orbit[0].1.binding().mod_keys, ModKeys::SHIFT);
        assert_eq!(enabled_pan_count, 0);
        assert_eq!(enabled_zoom.len(), 1);
        assert_eq!(enabled_zoom[0].0, 1);
        assert_eq!(enabled_zoom[0].1.binding().mod_keys, ModKeys::CONTROL);
        assert!(bindings.enabled_mouse_wheel_zoom().is_none());
        assert!(bindings.enabled_pinch_zoom_binding().is_none());
        assert!(bindings.enabled_button_drag_zoom().is_none());

        let Some(touch) = bindings.enabled_touch_config() else {
            assert!(bindings.enabled_touch_config().is_some());
            return Ok(());
        };
        assert_eq!(touch.binding(), OrbitCamTouchBinding::OneFingerOrbit);
        assert!(!touch.orbit_enabled());
        assert!(touch.pan_enabled());
        assert!(!touch.zoom_enabled());

        Ok(())
    }

    #[test]
    fn singleton_adapter_builder_calls_use_last_write() -> Result<(), OrbitCamBindingsError> {
        let replacement_touch_sensitivity = OrbitCamSensitivity::uniform(REPLACEMENT_SENSITIVITY);
        let bindings = OrbitCamBindings::builder()
            .zoom(OrbitCamMouseWheelZoom.with_sensitivity(WHEEL_SENSITIVITY))
            .zoom(OrbitCamMouseWheelZoom.with_sensitivity(REPLACEMENT_SENSITIVITY))
            .zoom(OrbitCamPinchZoom.with_sensitivity(PINCH_SENSITIVITY))
            .zoom(OrbitCamPinchZoom.with_sensitivity(REPLACEMENT_SENSITIVITY))
            .zoom(
                OrbitCamButtonDragZoom::new(MouseButton::Left)
                    .with_sensitivity(BUTTON_DRAG_SENSITIVITY),
            )
            .zoom(
                OrbitCamButtonDragZoom::new(MouseButton::Right)
                    .with_sensitivity(REPLACEMENT_SENSITIVITY),
            )
            .touch(Some(OrbitCamTouchBinding::OneFingerOrbit))
            .touch_config(Some(
                OrbitCamTouchBinding::TwoFingerOrbit
                    .with_sensitivity(replacement_touch_sensitivity),
            ))
            .build()?;

        let Some(wheel) = bindings.mouse_wheel_zoom() else {
            assert!(bindings.mouse_wheel_zoom().is_some());
            return Ok(());
        };
        assert_eq!(wheel.sensitivity(), InputGain(REPLACEMENT_SENSITIVITY));

        let Some(pinch) = bindings.pinch_zoom_binding() else {
            assert!(bindings.pinch_zoom_binding().is_some());
            return Ok(());
        };
        assert_eq!(pinch.sensitivity(), InputGain(REPLACEMENT_SENSITIVITY));

        let Some(button_drag) = bindings.button_drag_zoom() else {
            assert!(bindings.button_drag_zoom().is_some());
            return Ok(());
        };
        assert_eq!(button_drag.binding().button, MouseButton::Right);
        assert_eq!(
            button_drag.sensitivity(),
            InputGain(REPLACEMENT_SENSITIVITY)
        );

        let Some(touch) = bindings.touch_config() else {
            assert!(bindings.touch_config().is_some());
            return Ok(());
        };
        assert_eq!(touch.binding(), OrbitCamTouchBinding::TwoFingerOrbit);
        assert_eq!(touch.sensitivity(), replacement_touch_sensitivity);

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
        assert_eq!(
            OrbitCamInputBinding::from(GamepadAxis::RightStickX)
                .with_scale(-2.0)
                .with_sensitivity(MOUSE_DRAG_SENSITIVITY),
            OrbitCamInputBinding::from(GamepadAxis::RightStickX)
                .with_sensitivity(MOUSE_DRAG_SENSITIVITY)
                .with_scale(-2.0)
        );
    }

    #[test]
    fn native_scale_and_sensitivity_compose_for_installation() {
        let positive = OrbitCamInputBinding::from(GamepadAxis::RightStickX)
            .with_scale(2.0)
            .with_sensitivity(WHEEL_SENSITIVITY)
            .descriptor();
        let [entry] = positive.entries_slice() else {
            assert_eq!(positive.entries_slice().len(), 1);
            return;
        };
        assert_eq!(entry.install_modifiers().scale(), Some(0.5));

        let negative = OrbitCamInputBinding::from(GamepadAxis::RightStickX)
            .with_scale(-2.0)
            .with_sensitivity(WHEEL_SENSITIVITY)
            .descriptor();
        let [entry] = negative.entries_slice() else {
            assert_eq!(negative.entries_slice().len(), 1);
            return;
        };
        assert_eq!(entry.install_modifiers().scale(), Some(-0.5));

        let sensitivity_only = OrbitCamInputBinding::from(GamepadAxis::RightStickX)
            .with_sensitivity(WHEEL_SENSITIVITY)
            .descriptor();
        let [entry] = sensitivity_only.entries_slice() else {
            assert_eq!(sensitivity_only.entries_slice().len(), 1);
            return;
        };
        assert_eq!(entry.install_modifiers().scale(), Some(WHEEL_SENSITIVITY));

        let default = OrbitCamInputBinding::from(GamepadAxis::RightStickX).descriptor();
        let [entry] = default.entries_slice() else {
            assert_eq!(default.entries_slice().len(), 1);
            return;
        };
        assert_eq!(entry.install_modifiers().scale(), None);
    }

    #[test]
    fn per_axis_native_scale_and_sensitivity_compose_for_installation() {
        let binding = OrbitCamInputBinding::gamepad_axes_2d(
            GamepadAxis::RightStickX,
            GamepadAxis::RightStickY,
        )
        .with_scale(Vec2::new(-2.0, 4.0))
        .with_sensitivity(WHEEL_SENSITIVITY)
        .descriptor();
        let entries = binding.entries_slice();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].install_modifiers().scale(), Some(-0.5));
        assert_eq!(entries[1].install_modifiers().scale(), Some(1.0));
    }

    #[test]
    fn zero_sensitive_native_entries_stay_authored_but_leave_enabled_views()
    -> Result<(), OrbitCamBindingsError> {
        let bindings = OrbitCamBindings::builder()
            .orbit(OrbitCamMouseDrag::new(MouseButton::Left).with_sensitivity(DISABLED_SENSITIVITY))
            .build()?;

        assert_eq!(bindings.orbit().entries().len(), 1);
        assert_eq!(bindings.orbit().enabled_entries().count(), 0);
        let [entry] = bindings.orbit().entries() else {
            assert_eq!(bindings.orbit().entries().len(), 1);
            return Ok(());
        };
        let [motion] = entry.motion_descriptor().entries_slice() else {
            assert_eq!(entry.motion_descriptor().entries_slice().len(), 1);
            return Ok(());
        };
        assert_eq!(motion.sensitivity(), InputGain::DISABLED);
        assert_eq!(entry.enabled_motion_entries().count(), 0);
        assert_eq!(entry.engagement_descriptor().enabled_entries().count(), 1);

        Ok(())
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
