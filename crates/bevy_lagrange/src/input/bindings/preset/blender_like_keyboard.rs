use bevy::prelude::*;

use super::blender_like::OrbitCamBlenderLikePreset;
use super::config::OrbitCamPresetConfig;
use super::keyboard::OrbitCamKeyboardPreset;
use crate::input::bindings::OrbitCamBindings;
use crate::input::bindings::OrbitCamBindingsBuilder;
use crate::input::bindings::error::OrbitCamBindingsError;

/// Configures Blender-like pointer controls plus keyboard camera controls.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct OrbitCamBlenderLikeKeyboardPreset {
    pointer:  OrbitCamBlenderLikePreset,
    keyboard: OrbitCamKeyboardPreset,
}

impl OrbitCamBlenderLikeKeyboardPreset {
    /// Builds the Blender-like plus keyboard preset.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when generated descriptors fail
    /// validation.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        <Self as OrbitCamPresetConfig>::build(self)
    }

    /// Replaces the Blender-like child preset.
    #[must_use]
    pub const fn blender_like(mut self, preset: OrbitCamBlenderLikePreset) -> Self {
        self.pointer = preset;
        self
    }

    /// Replaces the keyboard child preset.
    #[must_use]
    pub const fn keyboard(mut self, preset: OrbitCamKeyboardPreset) -> Self {
        self.keyboard = preset;
        self
    }

    pub(super) fn build_into(
        self,
        builder: OrbitCamBindingsBuilder,
    ) -> Result<OrbitCamBindingsBuilder, OrbitCamBindingsError> {
        let builder = self.pointer.build_into(builder)?;
        Ok(self.keyboard.build_into(builder))
    }
}

impl Default for OrbitCamBlenderLikeKeyboardPreset {
    fn default() -> Self {
        Self {
            pointer:  OrbitCamBlenderLikePreset::default(),
            keyboard: OrbitCamKeyboardPreset,
        }
    }
}

impl OrbitCamPresetConfig for OrbitCamBlenderLikeKeyboardPreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_into(OrbitCamBindings::builder())?.build()
    }
}
