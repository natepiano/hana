use bevy::prelude::*;

use super::config::OrbitCamPresetConfig;
use crate::input::bindings::OrbitCamBindings;
use crate::input::bindings::OrbitCamBindingsBuilder;
use crate::input::bindings::OrbitCamInputBinding;
use crate::input::bindings::error::OrbitCamBindingsError;

/// Configures keyboard-only orbit-camera controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Default)]
pub struct OrbitCamKeyboardPreset;

impl OrbitCamKeyboardPreset {
    /// Builds the keyboard preset.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when generated descriptors fail
    /// validation.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        <Self as OrbitCamPresetConfig>::build(self)
    }

    pub(super) fn build_into(self, builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
        let Self = self;
        Self::add_to(builder)
    }

    fn add_to(builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
        let orbit_keys = OrbitCamInputBinding::cardinal_keys(
            KeyCode::ArrowUp,
            KeyCode::ArrowRight,
            KeyCode::ArrowDown,
            KeyCode::ArrowLeft,
        );
        let pan_keys = OrbitCamInputBinding::cardinal_keys(
            KeyCode::KeyW,
            KeyCode::KeyD,
            KeyCode::KeyS,
            KeyCode::KeyA,
        );
        let zoom_keys = OrbitCamInputBinding::bidirectional_keys(KeyCode::Equal, KeyCode::Minus);
        builder.orbit(orbit_keys).pan(pan_keys).zoom(zoom_keys)
    }
}

impl Default for OrbitCamKeyboardPreset {
    fn default() -> Self { Self }
}

impl OrbitCamPresetConfig for OrbitCamKeyboardPreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_into(OrbitCamBindings::builder()).build()
    }
}

/// Reflected draft for the keyboard-only preset payload.
#[cfg(feature = "reflect-input-modes")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Default)]
pub struct OrbitCamKeyboardPresetDraft;

#[cfg(feature = "reflect-input-modes")]
impl Default for OrbitCamKeyboardPresetDraft {
    fn default() -> Self { Self::from(OrbitCamKeyboardPreset) }
}

#[cfg(feature = "reflect-input-modes")]
impl From<OrbitCamKeyboardPresetDraft> for OrbitCamKeyboardPreset {
    fn from(draft: OrbitCamKeyboardPresetDraft) -> Self {
        let OrbitCamKeyboardPresetDraft = draft;
        Self
    }
}

#[cfg(feature = "reflect-input-modes")]
impl From<OrbitCamKeyboardPreset> for OrbitCamKeyboardPresetDraft {
    fn from(preset: OrbitCamKeyboardPreset) -> Self {
        let OrbitCamKeyboardPreset = preset;
        Self
    }
}
