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
//!   [`OrbitCamMouseWheelZoom`] (+ [`WheelZoomPolarity`]), [`OrbitCamPinchZoom`],
//!   [`OrbitCamButtonDragZoom`] (+ [`OrbitCamButtonDragZoomAxis`]), [`OrbitCamTouchBinding`],
//!   [`ZoomDirection`], [`CameraInputGamepadSelectionPolicy`].

use bevy::prelude::*;
use bevy_enhanced_input::prelude::Binding;
use bevy_enhanced_input::prelude::ModKeys;

use super::OrbitCamBindings;
use super::PinchGestureZoom;
use super::action_set::BindingRoutePolicy;
use super::descriptor::ActionBindingDescriptor;
use super::descriptor::HeldBindingDescriptor;
use super::error::OrbitCamBindingsError;
use super::held_binding::OrbitCamHeldBinding;
use super::held_binding::OrbitCamInputBinding;
use super::validate;
use crate::input::CameraInteractionSources;

/// Reflectable draft binding specification for editor and keymap tooling.
#[derive(Clone, Debug, Default, PartialEq, Reflect)]
pub struct OrbitCamBindingsDescriptor {
    pub(super) orbit:            Vec<HeldBindingDescriptor>,
    pub(super) pan:              Vec<HeldBindingDescriptor>,
    pub(super) zoom_smooth:      Vec<HeldBindingDescriptor>,
    pub(super) zoom_coarse:      Vec<ActionBindingDescriptor>,
    pub(super) trackpad_orbit:   Vec<OrbitCamTrackpadScroll>,
    pub(super) trackpad_pan:     Vec<OrbitCamTrackpadScroll>,
    pub(super) trackpad_zoom:    Vec<OrbitCamTrackpadScroll>,
    pub(super) mouse_wheel_zoom: Option<OrbitCamMouseWheelZoom>,
    pub(super) pinch_zoom:       super::PinchGestureZoom,
    pub(super) touch:            Option<OrbitCamTouchBinding>,
    pub(super) gamepad:          CameraInputGamepadSelectionPolicy,
    pub(super) zoom_direction:   ZoomDirection,
    pub(super) button_drag_zoom: Option<OrbitCamButtonDragZoom>,
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
        sources:            CameraInteractionSources::MOUSE,
        engagement_sources: CameraInteractionSources::MOUSE,
        route:              BindingRoutePolicy::CursorPosition,
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
            OrbitCamZoomBinding::Pinch(_) => {
                self.descriptor.pinch_zoom = PinchGestureZoom::Enabled;
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
        self.descriptor.touch = touch;
        self
    }

    /// Sets the gamepad selection policy.
    #[must_use]
    pub const fn gamepad(mut self, gamepad: CameraInputGamepadSelectionPolicy) -> Self {
        self.descriptor.gamepad = gamepad;
        self
    }

    /// Sets the zoom direction policy.
    #[must_use]
    pub const fn zoom_direction(mut self, zoom_direction: ZoomDirection) -> Self {
        self.descriptor.zoom_direction = zoom_direction;
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

/// Binding that can produce orbit intent.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamOrbitBinding {
    /// Held enhanced-input binding.
    Held(OrbitCamHeldBinding),
    /// Trackpad smooth-scroll binding.
    Trackpad(OrbitCamTrackpadScroll),
}

impl From<OrbitCamHeldBinding> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamHeldBinding) -> Self { Self::Held(value) }
}

impl From<OrbitCamMouseDrag> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamMouseDrag) -> Self { Self::Held(value.into()) }
}

impl From<OrbitCamInputBinding> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamInputBinding) -> Self { Self::Held(OrbitCamHeldBinding::same(value)) }
}

impl From<OrbitCamTrackpadScroll> for OrbitCamOrbitBinding {
    fn from(value: OrbitCamTrackpadScroll) -> Self { Self::Trackpad(value) }
}

/// Binding that can produce pan intent.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamPanBinding {
    /// Held enhanced-input binding.
    Held(OrbitCamHeldBinding),
    /// Trackpad smooth-scroll binding.
    Trackpad(OrbitCamTrackpadScroll),
}

impl From<OrbitCamHeldBinding> for OrbitCamPanBinding {
    fn from(value: OrbitCamHeldBinding) -> Self { Self::Held(value) }
}

impl From<OrbitCamMouseDrag> for OrbitCamPanBinding {
    fn from(value: OrbitCamMouseDrag) -> Self { Self::Held(value.into()) }
}

impl From<OrbitCamInputBinding> for OrbitCamPanBinding {
    fn from(value: OrbitCamInputBinding) -> Self { Self::Held(OrbitCamHeldBinding::same(value)) }
}

impl From<OrbitCamTrackpadScroll> for OrbitCamPanBinding {
    fn from(value: OrbitCamTrackpadScroll) -> Self { Self::Trackpad(value) }
}

/// Binding that can produce zoom intent.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamZoomBinding {
    /// Held enhanced-input binding.
    Held(OrbitCamHeldBinding),
    /// Trackpad smooth-scroll binding.
    Trackpad(OrbitCamTrackpadScroll),
    /// Mouse wheel zoom binding.
    MouseWheel(OrbitCamMouseWheelZoom),
    /// Pinch gesture zoom binding.
    Pinch(OrbitCamPinchZoom),
    /// Button-drag zoom binding.
    ButtonDrag(OrbitCamButtonDragZoom),
}

impl From<OrbitCamHeldBinding> for OrbitCamZoomBinding {
    fn from(value: OrbitCamHeldBinding) -> Self { Self::Held(value) }
}

impl From<OrbitCamInputBinding> for OrbitCamZoomBinding {
    fn from(value: OrbitCamInputBinding) -> Self { Self::Held(OrbitCamHeldBinding::same(value)) }
}

impl From<OrbitCamTrackpadScroll> for OrbitCamZoomBinding {
    fn from(value: OrbitCamTrackpadScroll) -> Self { Self::Trackpad(value) }
}

impl From<OrbitCamMouseWheelZoom> for OrbitCamZoomBinding {
    fn from(value: OrbitCamMouseWheelZoom) -> Self { Self::MouseWheel(value) }
}

impl From<OrbitCamPinchZoom> for OrbitCamZoomBinding {
    fn from(value: OrbitCamPinchZoom) -> Self { Self::Pinch(value) }
}

impl From<OrbitCamButtonDragZoom> for OrbitCamZoomBinding {
    fn from(value: OrbitCamButtonDragZoom) -> Self { Self::ButtonDrag(value) }
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
}

/// Mouse-wheel zoom binding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct OrbitCamMouseWheelZoom {
    /// Wheel polarity applied before zoom direction.
    pub polarity: WheelZoomPolarity,
}

/// Wheel polarity applied before zoom direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum WheelZoomPolarity {
    /// Wheel value passes through unchanged.
    #[default]
    Normal,
    /// Wheel value is negated before zoom direction is applied.
    Inverted,
}

/// Pinch gesture zoom binding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct OrbitCamPinchZoom;

/// Button-drag zoom binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub struct OrbitCamButtonDragZoom {
    /// Mouse button that engages button-drag zoom.
    pub button: MouseButton,
    /// Axis used for button-drag zoom.
    pub axis:   OrbitCamButtonDragZoomAxis,
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

/// Direction of scroll/zoom input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum ZoomDirection {
    /// Scrolling zooms in the default direction.
    #[default]
    Normal,
    /// Scrolling zooms in the opposite direction.
    Reversed,
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
