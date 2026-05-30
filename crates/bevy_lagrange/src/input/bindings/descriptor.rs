//! Internal descriptor and entry types used to feed [`super::validate::validate_bindings`].
//!
//! Types:
//! - [`HeldBindingDescriptor`] / [`ActionBindingDescriptor`] — reflectable descriptor entries
//!   stored on [`super::OrbitCamBindingsDescriptor`] (the editor/keymap-facing draft).
//! - [`InputBindingDescriptor`] / [`InputBindingEntry`] / [`InputBindingModifiers`] — flattened
//!   list of native `bevy_enhanced_input` bindings plus the per-entry modifiers applied by the
//!   adapter when it spawns enhanced-input binding entities.
//!
//! Functions:
//! - [`binding_active`] / [`mod_keys_pressed`] — runtime predicates evaluated against `ButtonInput`
//!   to decide whether a binding is currently held.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::Binding;
use bevy_enhanced_input::prelude::ModKeys;

use super::action_set::BindingEngagement;
use super::action_set::BindingRoutePolicy;
use super::held_binding::BindingGates;
use crate::input::CameraInteractionSources;
use crate::input::ControlSpeed;

#[derive(Clone, Debug, PartialEq, Reflect)]
pub(super) struct HeldBindingDescriptor {
    pub(super) motion:             InputBindingDescriptor,
    pub(super) engagement:         Option<InputBindingDescriptor>,
    pub(super) gates:              BindingGates,
    pub(super) sources:            CameraInteractionSources,
    pub(super) engagement_sources: CameraInteractionSources,
    pub(super) route:              BindingRoutePolicy,
    pub(super) speed:              ControlSpeed,
}

/// Reflectable descriptor for an impulse action binding.
#[derive(Clone, Debug, PartialEq, Reflect)]
pub struct ActionBindingDescriptor {
    pub(super) binding:    InputBindingDescriptor,
    pub(super) sources:    CameraInteractionSources,
    pub(super) route:      BindingRoutePolicy,
    pub(super) engagement: BindingEngagement,
}

#[derive(Clone, Debug, Default, PartialEq, Reflect)]
pub struct InputBindingDescriptor {
    entries: Vec<InputBindingEntry>,
}

impl InputBindingDescriptor {
    pub(super) fn single(binding: Binding) -> Self {
        Self {
            entries: vec![InputBindingEntry::new(binding, InputAxisTransform::None)],
        }
    }

    pub(super) fn entries<const N: usize>(entries: [InputBindingEntry; N]) -> Self {
        Self {
            entries: entries.into(),
        }
    }

    /// Returns the flattened binding entries.
    pub fn entries_slice(&self) -> &[InputBindingEntry] { &self.entries }

    pub(super) const fn is_empty(&self) -> bool { self.entries.is_empty() }

    pub(super) fn with_entry_modifiers(
        mut self,
        modifiers: InputBindingModifiers,
        scale: Option<InputBindingScale>,
    ) -> Self {
        for entry in &mut self.entries {
            entry.modifiers = entry.modifiers.combine(modifiers, scale, entry.output_axis);
        }
        self
    }

    /// Returns `true` when any entry's binding is currently pressed.
    pub fn is_active(
        &self,
        keyboard: Option<&ButtonInput<KeyCode>>,
        mouse_buttons: Option<&ButtonInput<MouseButton>>,
    ) -> bool {
        self.entries
            .iter()
            .any(|entry| binding_active(entry.binding, keyboard, mouse_buttons))
    }

    /// Returns the first mouse-button binding entry's button and modifier keys.
    pub fn mouse_button_engagement(&self) -> Option<(MouseButton, ModKeys)> {
        self.entries.iter().find_map(|entry| match entry.binding {
            Binding::MouseButton { button, mod_keys } => Some((button, mod_keys)),
            Binding::Keyboard { .. }
            | Binding::MouseMotion { .. }
            | Binding::MouseWheel { .. }
            | Binding::GamepadButton(_)
            | Binding::GamepadAxis(_)
            | Binding::AnyKey
            | Binding::Custom(_)
            | Binding::None => None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct InputBindingEntry {
    binding:     Binding,
    modifiers:   InputBindingModifiers,
    output_axis: InputBindingOutputAxis,
}

impl InputBindingEntry {
    pub(super) const fn new(binding: Binding, axis_transform: InputAxisTransform) -> Self {
        Self {
            binding,
            modifiers: InputBindingModifiers::new(axis_transform),
            output_axis: InputBindingOutputAxis::X,
        }
    }

    pub(super) const fn with_output_axis(mut self, output_axis: InputBindingOutputAxis) -> Self {
        self.output_axis = output_axis;
        self
    }

    /// Returns the underlying enhanced-input binding.
    #[must_use]
    pub const fn binding(&self) -> Binding { self.binding }

    /// Returns the modifiers installed with this binding entry.
    #[must_use]
    pub const fn modifiers(&self) -> InputBindingModifiers { self.modifiers }
}

/// Canonical modifier descriptor attached to a flattened binding entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct InputBindingModifiers {
    dead_zone:      Option<InputDeadZone>,
    scale:          Option<f32>,
    delta_scale:    InputDeltaScale,
    axis_transform: InputAxisTransform,
}

impl InputBindingModifiers {
    pub(super) const fn new(axis_transform: InputAxisTransform) -> Self {
        Self {
            dead_zone: None,
            scale: None,
            delta_scale: InputDeltaScale::Disabled,
            axis_transform,
        }
    }

    pub(super) fn combine(
        self,
        modifiers: Self,
        scale: Option<InputBindingScale>,
        output_axis: InputBindingOutputAxis,
    ) -> Self {
        Self {
            dead_zone:      modifiers.dead_zone.or(self.dead_zone),
            scale:          scale_component(scale, output_axis).or(self.scale),
            delta_scale:    modifiers.delta_scale,
            axis_transform: self.axis_transform,
        }
    }

    pub(super) const fn with_dead_zone(mut self, dead_zone: InputDeadZone) -> Self {
        self.dead_zone = Some(dead_zone);
        self
    }

    pub(super) const fn with_delta_scale(mut self) -> Self {
        self.delta_scale = InputDeltaScale::Auto;
        self
    }

    /// Returns the axial dead-zone modifier.
    #[must_use]
    pub const fn dead_zone(self) -> Option<InputDeadZone> { self.dead_zone }

    /// Returns the scalar modifier.
    #[must_use]
    pub const fn scale(self) -> Option<f32> { self.scale }

    /// Returns the frame-delta scale mode.
    #[must_use]
    pub const fn delta_scale(self) -> InputDeltaScale { self.delta_scale }

    /// Returns the axis transform.
    #[must_use]
    pub const fn axis_transform(self) -> InputAxisTransform { self.axis_transform }
}

/// Scale descriptor that can apply uniformly or per logical 2D axis.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub enum InputBindingScale {
    /// Same scale for every expanded entry.
    Uniform(f32),
    /// Per-axis scale for 2D composite bindings.
    Axes2d(Vec2),
}

impl From<f32> for InputBindingScale {
    fn from(value: f32) -> Self { Self::Uniform(value) }
}

impl From<Vec2> for InputBindingScale {
    fn from(value: Vec2) -> Self { Self::Axes2d(value) }
}

/// Axial dead-zone thresholds for analog bindings.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct InputDeadZone {
    /// Threshold below which input is ignored.
    pub lower_threshold: f32,
    /// Threshold above which input is clamped to full strength.
    pub upper_threshold: f32,
}

impl InputDeadZone {
    /// Creates an axial dead-zone descriptor.
    #[must_use]
    pub const fn new(lower_threshold: f32, upper_threshold: f32) -> Self {
        Self {
            lower_threshold,
            upper_threshold,
        }
    }
}

/// Whether a held binding is scaled by frame delta.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum InputDeltaScale {
    /// Leave the value in source units.
    #[default]
    Disabled,
    /// Multiply the value by the current frame delta.
    Auto,
}

/// Intrinsic axis transform applied after dead zone, scale, and delta scale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum InputAxisTransform {
    /// Leave the input axis unchanged.
    #[default]
    None,
    /// Negate every axis.
    Negate,
    /// Swizzle X into Y.
    Swizzle,
    /// Swizzle X into Y and negate it.
    SwizzleNegate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub(super) enum InputBindingOutputAxis {
    #[default]
    X,
    Y,
}

const fn scale_component(
    scale: Option<InputBindingScale>,
    output_axis: InputBindingOutputAxis,
) -> Option<f32> {
    match scale {
        Some(InputBindingScale::Uniform(value)) => Some(value),
        Some(InputBindingScale::Axes2d(value)) => match output_axis {
            InputBindingOutputAxis::X => Some(value.x),
            InputBindingOutputAxis::Y => Some(value.y),
        },
        None => None,
    }
}

fn binding_active(
    binding: Binding,
    keyboard: Option<&ButtonInput<KeyCode>>,
    mouse_buttons: Option<&ButtonInput<MouseButton>>,
) -> bool {
    match binding {
        Binding::Keyboard { key, mod_keys } => keyboard
            .is_some_and(|keyboard| keyboard.pressed(key) && mod_keys_pressed(keyboard, mod_keys)),
        Binding::MouseButton { button, mod_keys } => {
            mouse_buttons.is_some_and(|buttons| buttons.pressed(button))
                && keyboard.is_some_and(|keyboard| mod_keys_pressed(keyboard, mod_keys))
        },
        Binding::AnyKey => {
            keyboard.is_some_and(|keyboard| keyboard.get_pressed().next().is_some())
                || mouse_buttons
                    .is_some_and(|mouse_buttons| mouse_buttons.get_pressed().next().is_some())
        },
        Binding::MouseMotion { .. }
        | Binding::MouseWheel { .. }
        | Binding::GamepadButton(_)
        | Binding::GamepadAxis(_)
        | Binding::Custom(_)
        | Binding::None => false,
    }
}

/// Returns `true` when every required modifier key is currently pressed.
pub(crate) fn mod_keys_pressed(keyboard: &ButtonInput<KeyCode>, mod_keys: ModKeys) -> bool {
    mod_keys.iter_keys().all(|keys| keyboard.any_pressed(keys))
}
