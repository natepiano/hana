use bevy::prelude::*;

use super::config::OrbitCamPresetConfig;
use super::keyboard::OrbitCamKeyboardPreset;
use super::simple_mouse::OrbitCamSimpleMousePreset;
use super::source_sensitivity::MouseSensitivity;
use super::source_sensitivity::SmoothScrollSensitivity;
use crate::input::bindings::OrbitCamBindings;
use crate::input::bindings::OrbitCamBindingsBuilder;
use crate::input::bindings::OrbitCamSensitivity;
use crate::input::bindings::error::OrbitCamBindingsError;

/// Configures simple mouse controls plus keyboard camera controls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Default)]
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

    /// Sets source sensitivity for mouse-drag and line-wheel input.
    #[must_use]
    pub const fn mouse_sensitivity(mut self, sensitivity: OrbitCamSensitivity) -> Self {
        self.pointer = self.pointer.mouse_sensitivity(sensitivity);
        self
    }

    /// Sets source sensitivity for Bevy pixel-scroll input.
    #[must_use]
    pub const fn smooth_scroll_sensitivity(mut self, sensitivity: OrbitCamSensitivity) -> Self {
        self.pointer = self.pointer.smooth_scroll_sensitivity(sensitivity);
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

impl MouseSensitivity for OrbitCamSimpleMouseKeyboardPreset {
    type Sensitivity = OrbitCamSensitivity;

    fn mouse_sensitivity(self, sensitivity: Self::Sensitivity) -> Self {
        Self::mouse_sensitivity(self, sensitivity)
    }
}

impl SmoothScrollSensitivity for OrbitCamSimpleMouseKeyboardPreset {
    type Sensitivity = OrbitCamSensitivity;

    fn smooth_scroll_sensitivity(self, sensitivity: Self::Sensitivity) -> Self {
        Self::smooth_scroll_sensitivity(self, sensitivity)
    }
}

impl OrbitCamPresetConfig for OrbitCamSimpleMouseKeyboardPreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_into(OrbitCamBindings::builder())?.build()
    }
}
