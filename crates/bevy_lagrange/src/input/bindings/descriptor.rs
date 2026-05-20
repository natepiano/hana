//! Internal descriptor and entry types used to feed [`super::validate::validate_bindings`].
//!
//! Types:
//! - [`HeldBindingDescriptor`] / [`ActionBindingDescriptor`] — reflectable descriptor entries
//!   stored on [`super::OrbitCamBindingsDescriptor`] (the editor/keymap-facing draft).
//! - [`InputBindingDescriptor`] / [`InputBindingEntry`] / [`InputBindingTransform`] — flattened
//!   list of native `bevy_enhanced_input` bindings plus the per-entry transform applied by the
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
use crate::input::CameraInteractionSources;

#[derive(Clone, Debug, PartialEq, Reflect)]
pub(super) struct HeldBindingDescriptor {
    pub(super) motion:             InputBindingDescriptor,
    pub(super) engagement:         Option<InputBindingDescriptor>,
    pub(super) sources:            CameraInteractionSources,
    pub(super) engagement_sources: CameraInteractionSources,
    pub(super) route:              BindingRoutePolicy,
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
            entries: vec![InputBindingEntry::new(binding, InputBindingTransform::None)],
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
    pub(crate) binding:   Binding,
    pub(crate) transform: InputBindingTransform,
}

impl InputBindingEntry {
    pub(super) const fn new(binding: Binding, transform: InputBindingTransform) -> Self {
        Self { binding, transform }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum InputBindingTransform {
    None,
    Negate,
    Swizzle,
    SwizzleNegate,
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
