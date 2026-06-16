use bevy::prelude::*;

use super::config::OrbitCamPresetConfig;
use super::enum_preset::OrbitCamBindingsProfile;
use super::enum_preset::OrbitCamPresetLayer;
use super::enum_preset::PresetLayerSet;
use super::keyboard::OrbitCamKeyboardPreset;
use super::simple_mouse::OrbitCamSimpleMousePreset;
use crate::input::bindings::OrbitCamBindings;
use crate::input::bindings::OrbitCamBindingsBuilder;
use crate::input::bindings::error::OrbitCamBindingsError;

/// Configures simple mouse controls plus keyboard camera controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub struct OrbitCamSimpleMouseKeyboardPreset {
    pointer:  OrbitCamSimpleMousePreset,
    keyboard: OrbitCamKeyboardPreset,
}

impl OrbitCamSimpleMouseKeyboardPreset {
    /// Builds the simple mouse plus keyboard preset.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when generated descriptors fail
    /// validation.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        <Self as OrbitCamPresetConfig>::build(self)
    }

    /// Replaces the simple mouse child preset.
    #[must_use]
    pub const fn simple_mouse(mut self, preset: OrbitCamSimpleMousePreset) -> Self {
        self.pointer = preset;
        self
    }

    /// Replaces the keyboard child preset.
    #[must_use]
    pub const fn keyboard(mut self, preset: OrbitCamKeyboardPreset) -> Self {
        self.keyboard = preset;
        self
    }

    pub(super) fn build_into(self, builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
        let builder = self.pointer.build_into(builder);
        self.keyboard.build_into(builder)
    }
}

impl Default for OrbitCamSimpleMouseKeyboardPreset {
    fn default() -> Self {
        Self {
            pointer:  OrbitCamSimpleMousePreset,
            keyboard: OrbitCamKeyboardPreset,
        }
    }
}

impl OrbitCamPresetConfig for OrbitCamSimpleMouseKeyboardPreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_into(OrbitCamBindings::builder())
            .profile(OrbitCamBindingsProfile::LayeredPreset {
                layers: PresetLayerSet::empty()
                    .with_layer(OrbitCamPresetLayer::SimpleMouse)
                    .with_layer(OrbitCamPresetLayer::Keyboard),
            })
            .build()
    }
}
