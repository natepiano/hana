//! Programmatic construction of [`super::OrbitCamBindings`] plus the user-facing input-kind
//! types accepted by the builder.
//!
//! Types:
//! - [`OrbitCamBindingsBuilder`] — fluent builder that pushes binding descriptors into a
//!   [`OrbitCamBindingsDescriptor`] and runs [`super::validate::validate_bindings`].
//! - [`OrbitCamBindingsDescriptor`] — reflectable draft binding specification for editor and keymap
//!   tooling.
//! - Dispatch enums consumed by `.orbit()` / `.pan()` / `.zoom()` builder methods:
//!   [`OrbitCamOrbitBinding`] / [`OrbitCamPanBinding`] / [`OrbitCamZoomBinding`].
//! - Concrete binding kinds: [`OrbitCamMouseDrag`], [`OrbitCamTrackpadScroll`],
//!   [`OrbitCamMouseWheelZoom`], [`OrbitCamPinchZoom`], [`OrbitCamButtonDragZoom`] (+
//!   [`OrbitCamButtonDragZoomAxis`]), [`OrbitCamTouchBinding`], [`ZoomInversion`],
//!   [`CameraInputGamepadSelectionPolicy`].

use std::ops::Deref;

use bevy::prelude::*;
use bevy_enhanced_input::prelude::Binding;
use bevy_enhanced_input::prelude::ModKeys;

use super::OrbitCamBindings;
use super::action_set::BindingRoutePolicy;
use super::descriptor::ActionBindingDescriptor;
use super::descriptor::HeldBindingDescriptor;
use super::descriptor::InputSensitivity;
use super::descriptor::OrbitCamSensitivity;
use super::descriptor::OrbitCamSlowMode;
use super::error::OrbitCamBindingsError;
#[cfg(test)]
use super::held_binding::BindingGates;
use super::held_binding::OrbitCamHeldBinding;
use super::held_binding::OrbitCamInputBinding;
use super::validate;
use crate::input::CameraInteractionSources;
#[cfg(test)]
use crate::input::ControlSpeed;

/// Reflectable draft binding specification for editor and keymap tooling.
#[derive(Clone, Debug, Default, PartialEq, Reflect)]
pub struct OrbitCamBindingsDescriptor {
    pub(super) orbit:            Vec<HeldBindingDescriptor>,
    pub(super) pan:              Vec<HeldBindingDescriptor>,
    pub(super) zoom_smooth:      Vec<HeldBindingDescriptor>,
    pub(super) zoom_coarse:      Vec<ActionBindingDescriptor>,
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

impl TryFrom<OrbitCamBindingsDescriptor> for OrbitCamBindings {
    type Error = OrbitCamBindingsError;

    fn try_from(descriptor: OrbitCamBindingsDescriptor) -> Result<Self, Self::Error> {
        validate::validate_bindings(&descriptor)
    }
}

#[cfg(test)]
pub(crate) fn invalid_bindings_descriptor_for_tests() -> OrbitCamBindingsDescriptor {
    let mut descriptor = OrbitCamBindingsDescriptor::default();
    descriptor.orbit.push(HeldBindingDescriptor {
        motion:             OrbitCamInputBinding::from(Binding::mouse_motion()).descriptor(),
        engagement:         None,
        gates:              BindingGates::default(),
        sources:            CameraInteractionSources::MOUSE,
        engagement_sources: CameraInteractionSources::MOUSE,
        route:              BindingRoutePolicy::CursorPosition,
        speed:              ControlSpeed::Normal,
    });
    descriptor
}

/// Builder for `OrbitCamBindings`.
#[derive(Clone, Debug, Default)]
pub struct OrbitCamBindingsBuilder {
    descriptor: OrbitCamBindingsDescriptor,
}

impl OrbitCamBindingsBuilder {
    /// Adds a binding that produces orbit intent.
    #[must_use]
    pub fn orbit(mut self, binding: impl Into<OrbitCamOrbitBinding>) -> Self {
        match binding.into() {
            OrbitCamOrbitBinding::Held(binding) => self.descriptor.orbit.push(binding.into()),
            OrbitCamOrbitBinding::Trackpad(binding) => self.descriptor.trackpad_orbit.push(binding),
        }
        self
    }

    /// Adds a binding that produces pan intent.
    #[must_use]
    pub fn pan(mut self, binding: impl Into<OrbitCamPanBinding>) -> Self {
        match binding.into() {
            OrbitCamPanBinding::Held(binding) => self.descriptor.pan.push(binding.into()),
            OrbitCamPanBinding::Trackpad(binding) => self.descriptor.trackpad_pan.push(binding),
        }
        self
    }

    /// Adds a binding that produces zoom intent.
    #[must_use]
    pub fn zoom(mut self, binding: impl Into<OrbitCamZoomBinding>) -> Self {
        match binding.into() {
            OrbitCamZoomBinding::Held(binding) => {
                self.descriptor.zoom_smooth.push(binding.into());
            },
            OrbitCamZoomBinding::Trackpad(binding) => self.descriptor.trackpad_zoom.push(binding),
            OrbitCamZoomBinding::MouseWheel(binding) => {
                self.descriptor.mouse_wheel_zoom = Some(binding);
            },
            OrbitCamZoomBinding::Pinch(binding) => {
                self.descriptor.pinch_zoom = Some(binding);
            },
            OrbitCamZoomBinding::ButtonDrag(binding) => {
                self.descriptor.button_drag_zoom = Some(binding);
            },
        }
        self
    }

    /// Sets the touch policy.
    #[must_use]
    pub const fn touch(mut self, touch: Option<OrbitCamTouchBinding>) -> Self {
        self.descriptor.touch = match touch {
            Some(touch) => Some(OrbitCamTouchBindingConfig::new(touch)),
            None => None,
        };
        self
    }

    /// Sets the touch policy with explicit per-action sensitivity.
    #[must_use]
    pub const fn touch_config(mut self, touch: Option<OrbitCamTouchBindingConfig>) -> Self {
        self.descriptor.touch = touch;
        self
    }

    /// Sets the gamepad selection policy.
    #[must_use]
    pub const fn gamepad(mut self, gamepad: CameraInputGamepadSelectionPolicy) -> Self {
        self.descriptor.gamepad = gamepad;
        self
    }

    /// Sets the zoom inversion policy.
    #[must_use]
    pub const fn zoom_inversion(mut self, zoom_inversion: ZoomInversion) -> Self {
        self.descriptor.zoom_inversion = zoom_inversion;
        self
    }

    /// Sets the slow-mode policy.
    #[must_use]
    pub const fn slow_mode(mut self, slow_mode: OrbitCamSlowMode) -> Self {
        self.descriptor.slow_mode = Some(slow_mode);
        self
    }

    /// Builds validated `OrbitCamBindings`.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when the descriptor violates a binding
    /// invariant.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        validate::validate_bindings(&self.descriptor)
    }
}

/// A binding value plus its authored input sensitivity.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct OrbitCamBindingWithSensitivity<T> {
    binding:     T,
    sensitivity: InputSensitivity,
}

impl<T> OrbitCamBindingWithSensitivity<T> {
    /// Creates a binding wrapper with explicit sensitivity.
    #[must_use]
    pub const fn new(binding: T, sensitivity: f32) -> Self {
        Self {
            binding,
            sensitivity: InputSensitivity(sensitivity),
        }
    }

    /// Returns the wrapped binding.
    #[must_use]
    pub const fn binding(&self) -> &T { &self.binding }

    /// Returns the authored sensitivity.
    #[must_use]
    pub const fn sensitivity(&self) -> InputSensitivity { self.sensitivity }

    /// Replaces the authored sensitivity.
    #[must_use]
    pub const fn with_sensitivity(mut self, sensitivity: f32) -> Self {
        self.sensitivity = InputSensitivity(sensitivity);
        self
    }
}

impl<T> Deref for OrbitCamBindingWithSensitivity<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target { self.binding() }
}

impl<T> From<T> for OrbitCamBindingWithSensitivity<T> {
    fn from(binding: T) -> Self {
        Self {
            binding,
            sensitivity: InputSensitivity::DEFAULT,
        }
    }
}

impl OrbitCamBindingWithSensitivity<OrbitCamMouseDrag> {
    /// Requires keyboard modifiers on both mouse motion and button engagement.
    #[must_use]
    pub const fn with_mod_keys(mut self, mod_keys: ModKeys) -> Self {
        self.binding = self.binding.with_mod_keys(mod_keys);
        self
    }
}

impl OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll> {
    /// Requires keyboard modifiers on smooth-scroll input.
    #[must_use]
    pub const fn with_mod_keys(mut self, mod_keys: ModKeys) -> Self {
        self.binding = self.binding.with_mod_keys(mod_keys);
        self
    }
}

impl OrbitCamBindingWithSensitivity<OrbitCamButtonDragZoom> {
    /// Sets the axis used for button-drag zoom.
    #[must_use]
    pub const fn with_axis(mut self, axis: OrbitCamButtonDragZoomAxis) -> Self {
        self.binding = self.binding.with_axis(axis);
        self
    }
}

/// Binding that can produce orbit intent.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamOrbitBinding {
    /// Held enhanced-input binding.
    Held(OrbitCamHeldBinding),
    /// Trackpad smooth-scroll binding.
    Trackpad(OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>),
}

impl From<OrbitCamHeldBinding> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamHeldBinding) -> Self { Self::Held(value) }
}

impl From<OrbitCamMouseDrag> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamMouseDrag) -> Self { Self::Held(value.into()) }
}

impl From<OrbitCamBindingWithSensitivity<OrbitCamMouseDrag>> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamBindingWithSensitivity<OrbitCamMouseDrag>) -> Self {
        Self::Held(value.into())
    }
}

impl From<OrbitCamInputBinding> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamInputBinding) -> Self { Self::Held(OrbitCamHeldBinding::same(value)) }
}

impl From<OrbitCamTrackpadScroll> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamTrackpadScroll) -> Self { Self::Trackpad(value.into()) }
}

impl From<OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>) -> Self {
        Self::Trackpad(value)
    }
}

/// Binding that can produce pan intent.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamPanBinding {
    /// Held enhanced-input binding.
    Held(OrbitCamHeldBinding),
    /// Trackpad smooth-scroll binding.
    Trackpad(OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>),
}

impl From<OrbitCamHeldBinding> for OrbitCamPanBinding {
    fn from(value: OrbitCamHeldBinding) -> Self { Self::Held(value) }
}

impl From<OrbitCamMouseDrag> for OrbitCamPanBinding {
    fn from(value: OrbitCamMouseDrag) -> Self { Self::Held(value.into()) }
}

impl From<OrbitCamBindingWithSensitivity<OrbitCamMouseDrag>> for OrbitCamPanBinding {
    fn from(value: OrbitCamBindingWithSensitivity<OrbitCamMouseDrag>) -> Self {
        Self::Held(value.into())
    }
}

impl From<OrbitCamInputBinding> for OrbitCamPanBinding {
    fn from(value: OrbitCamInputBinding) -> Self { Self::Held(OrbitCamHeldBinding::same(value)) }
}

impl From<OrbitCamTrackpadScroll> for OrbitCamPanBinding {
    fn from(value: OrbitCamTrackpadScroll) -> Self { Self::Trackpad(value.into()) }
}

impl From<OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>> for OrbitCamPanBinding {
    fn from(value: OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>) -> Self {
        Self::Trackpad(value)
    }
}

/// Binding that can produce zoom intent.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamZoomBinding {
    /// Held enhanced-input binding.
    Held(OrbitCamHeldBinding),
    /// Trackpad smooth-scroll binding.
    Trackpad(OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>),
    /// Mouse wheel zoom binding.
    MouseWheel(OrbitCamBindingWithSensitivity<OrbitCamMouseWheelZoom>),
    /// Pinch gesture zoom binding.
    Pinch(OrbitCamBindingWithSensitivity<OrbitCamPinchZoom>),
    /// Button-drag zoom binding.
    ButtonDrag(OrbitCamBindingWithSensitivity<OrbitCamButtonDragZoom>),
}

impl From<OrbitCamHeldBinding> for OrbitCamZoomBinding {
    fn from(value: OrbitCamHeldBinding) -> Self { Self::Held(value) }
}

impl From<OrbitCamInputBinding> for OrbitCamZoomBinding {
    fn from(value: OrbitCamInputBinding) -> Self { Self::Held(OrbitCamHeldBinding::same(value)) }
}

impl From<OrbitCamTrackpadScroll> for OrbitCamZoomBinding {
    fn from(value: OrbitCamTrackpadScroll) -> Self { Self::Trackpad(value.into()) }
}

impl From<OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>> for OrbitCamZoomBinding {
    fn from(value: OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>) -> Self {
        Self::Trackpad(value)
    }
}

impl From<OrbitCamMouseWheelZoom> for OrbitCamZoomBinding {
    fn from(value: OrbitCamMouseWheelZoom) -> Self { Self::MouseWheel(value.into()) }
}

impl From<OrbitCamBindingWithSensitivity<OrbitCamMouseWheelZoom>> for OrbitCamZoomBinding {
    fn from(value: OrbitCamBindingWithSensitivity<OrbitCamMouseWheelZoom>) -> Self {
        Self::MouseWheel(value)
    }
}

impl From<OrbitCamPinchZoom> for OrbitCamZoomBinding {
    fn from(value: OrbitCamPinchZoom) -> Self { Self::Pinch(value.into()) }
}

impl From<OrbitCamBindingWithSensitivity<OrbitCamPinchZoom>> for OrbitCamZoomBinding {
    fn from(value: OrbitCamBindingWithSensitivity<OrbitCamPinchZoom>) -> Self { Self::Pinch(value) }
}

impl From<OrbitCamButtonDragZoom> for OrbitCamZoomBinding {
    fn from(value: OrbitCamButtonDragZoom) -> Self { Self::ButtonDrag(value.into()) }
}

impl From<OrbitCamBindingWithSensitivity<OrbitCamButtonDragZoom>> for OrbitCamZoomBinding {
    fn from(value: OrbitCamBindingWithSensitivity<OrbitCamButtonDragZoom>) -> Self {
        Self::ButtonDrag(value)
    }
}

/// Mouse-drag binding for orbit or pan behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub struct OrbitCamMouseDrag {
    /// Mouse button that engages the drag.
    pub button:   MouseButton,
    /// Keyboard modifiers required by both motion and button engagement.
    pub mod_keys: ModKeys,
}

impl OrbitCamMouseDrag {
    /// Creates a mouse-drag binding without keyboard modifiers.
    #[must_use]
    pub const fn new(button: MouseButton) -> Self {
        Self {
            button,
            mod_keys: ModKeys::empty(),
        }
    }

    /// Requires keyboard modifiers on both mouse motion and button engagement.
    #[must_use]
    pub const fn with_mod_keys(mut self, mod_keys: ModKeys) -> Self {
        self.mod_keys = mod_keys;
        self
    }

    /// Sets the authored sensitivity for this mouse-drag binding.
    #[must_use]
    pub const fn with_sensitivity(self, sensitivity: f32) -> OrbitCamBindingWithSensitivity<Self> {
        OrbitCamBindingWithSensitivity::new(self, sensitivity)
    }
}

impl From<OrbitCamMouseDrag> for OrbitCamHeldBinding {
    fn from(value: OrbitCamMouseDrag) -> Self {
        Self::new(
            Binding::MouseMotion {
                mod_keys: value.mod_keys,
            },
            Binding::MouseButton {
                button:   value.button,
                mod_keys: value.mod_keys,
            },
        )
        .with_sources(CameraInteractionSources::MOUSE)
        .with_route(BindingRoutePolicy::CursorPosition)
    }
}

impl From<OrbitCamBindingWithSensitivity<OrbitCamMouseDrag>> for OrbitCamHeldBinding {
    fn from(value: OrbitCamBindingWithSensitivity<OrbitCamMouseDrag>) -> Self {
        Self::from(*value.binding()).with_sensitivity(value.sensitivity().value())
    }
}

/// Trackpad smooth-scroll binding for orbit, pan, or zoom behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct OrbitCamTrackpadScroll {
    /// Keyboard modifiers required by the smooth-scroll binding.
    pub mod_keys: ModKeys,
}

impl OrbitCamTrackpadScroll {
    /// Requires keyboard modifiers on smooth-scroll input.
    #[must_use]
    pub const fn with_mod_keys(mut self, mod_keys: ModKeys) -> Self {
        self.mod_keys = mod_keys;
        self
    }

    /// Sets the authored sensitivity for this smooth-scroll binding.
    #[must_use]
    pub const fn with_sensitivity(self, sensitivity: f32) -> OrbitCamBindingWithSensitivity<Self> {
        OrbitCamBindingWithSensitivity::new(self, sensitivity)
    }
}

/// Mouse-wheel zoom binding. Zoom direction is governed by the camera's
/// [`ZoomInversion`]; this binding only enables wheel zoom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct OrbitCamMouseWheelZoom;

impl OrbitCamMouseWheelZoom {
    /// Sets the authored sensitivity for this mouse-wheel zoom binding.
    #[must_use]
    pub const fn with_sensitivity(self, sensitivity: f32) -> OrbitCamBindingWithSensitivity<Self> {
        OrbitCamBindingWithSensitivity::new(self, sensitivity)
    }
}

/// Pinch gesture zoom binding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct OrbitCamPinchZoom;

impl OrbitCamPinchZoom {
    /// Sets the authored sensitivity for this pinch zoom binding.
    #[must_use]
    pub const fn with_sensitivity(self, sensitivity: f32) -> OrbitCamBindingWithSensitivity<Self> {
        OrbitCamBindingWithSensitivity::new(self, sensitivity)
    }
}

/// Button-drag zoom binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub struct OrbitCamButtonDragZoom {
    /// Mouse button that engages button-drag zoom.
    pub button: MouseButton,
    /// Axis used for button-drag zoom.
    pub axis:   OrbitCamButtonDragZoomAxis,
}

impl OrbitCamButtonDragZoom {
    /// Creates a button-drag zoom binding using vertical motion.
    #[must_use]
    pub const fn new(button: MouseButton) -> Self {
        Self {
            button,
            axis: OrbitCamButtonDragZoomAxis::Y,
        }
    }

    /// Sets the axis used for button-drag zoom.
    #[must_use]
    pub const fn with_axis(mut self, axis: OrbitCamButtonDragZoomAxis) -> Self {
        self.axis = axis;
        self
    }

    /// Sets the authored sensitivity for this button-drag zoom binding.
    #[must_use]
    pub const fn with_sensitivity(self, sensitivity: f32) -> OrbitCamBindingWithSensitivity<Self> {
        OrbitCamBindingWithSensitivity::new(self, sensitivity)
    }
}

/// Touch gesture policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamTouchBinding {
    /// One finger orbits and two fingers pan and zoom.
    OneFingerOrbit,
    /// One finger pans and two fingers orbit and zoom.
    TwoFingerOrbit,
}

impl OrbitCamTouchBinding {
    /// Sets per-action authored sensitivity for this touch policy.
    #[must_use]
    pub const fn with_sensitivity(
        self,
        sensitivity: OrbitCamSensitivity,
    ) -> OrbitCamTouchBindingConfig {
        OrbitCamTouchBindingConfig {
            binding: self,
            sensitivity,
        }
    }
}

/// Touch gesture policy plus per-action authored sensitivity.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct OrbitCamTouchBindingConfig {
    binding:     OrbitCamTouchBinding,
    sensitivity: OrbitCamSensitivity,
}

impl OrbitCamTouchBindingConfig {
    /// Creates a touch policy using default sensitivity for every action.
    #[must_use]
    pub const fn new(binding: OrbitCamTouchBinding) -> Self {
        Self {
            binding,
            sensitivity: OrbitCamSensitivity::new(),
        }
    }

    /// Returns the touch policy.
    #[must_use]
    pub const fn binding(self) -> OrbitCamTouchBinding { self.binding }

    /// Returns the authored per-action touch sensitivity.
    #[must_use]
    pub const fn sensitivity(self) -> OrbitCamSensitivity { self.sensitivity }
}

impl From<OrbitCamTouchBinding> for OrbitCamTouchBindingConfig {
    fn from(binding: OrbitCamTouchBinding) -> Self { Self::new(binding) }
}

/// Whether scroll-based zoom (mouse wheel, pinch, smooth-scroll) is inverted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum ZoomInversion {
    /// Scroll-based zoom runs in its default direction.
    #[default]
    Normal,
    /// Scroll-based zoom is inverted: each gesture zooms the opposite way.
    Inverted,
}

/// Axis used for button-drag zoom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamButtonDragZoomAxis {
    /// Horizontal motion controls zoom.
    X,
    /// Vertical motion controls zoom.
    #[default]
    Y,
    /// Horizontal plus vertical motion controls zoom.
    XY,
}

/// Gamepad routing policy for camera input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum CameraInputGamepadSelectionPolicy {
    /// Ignore gamepad input.
    #[default]
    Disabled,
    /// Route a single gamepad through active camera routing.
    Active,
}
