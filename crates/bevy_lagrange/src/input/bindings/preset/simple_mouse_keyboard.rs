use bevy::prelude::*;

use super::config::OrbitCamPresetConfig;
use super::keyboard::OrbitCamKeyboardPreset;
use super::simple_mouse::OrbitCamSimpleMousePreset;
use super::source_input_gain::MouseInputGain;
use super::source_input_gain::SmoothScrollInputGain;
use crate::input::bindings::OrbitCamBindings;
use crate::input::bindings::OrbitCamBindingsBuilder;
use crate::input::bindings::OrbitCamInputGain;
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

    /// Sets source input gain for mouse-drag and line-wheel input.
    #[must_use]
    pub const fn mouse_input_gain(mut self, input_gain: OrbitCamInputGain) -> Self {
        self.pointer = self.pointer.mouse_input_gain(input_gain);
        self
    }

    /// Sets source input gain for Bevy pixel-scroll input.
    #[must_use]
    pub const fn smooth_scroll_input_gain(mut self, input_gain: OrbitCamInputGain) -> Self {
        self.pointer = self.pointer.smooth_scroll_input_gain(input_gain);
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

impl MouseInputGain for OrbitCamSimpleMouseKeyboardPreset {
    type Gain = OrbitCamInputGain;

    fn mouse_input_gain(self, input_gain: Self::Gain) -> Self {
        Self::mouse_input_gain(self, input_gain)
    }
}

impl SmoothScrollInputGain for OrbitCamSimpleMouseKeyboardPreset {
    type Gain = OrbitCamInputGain;

    fn smooth_scroll_input_gain(self, input_gain: Self::Gain) -> Self {
        Self::smooth_scroll_input_gain(self, input_gain)
    }
}

impl OrbitCamPresetConfig for OrbitCamSimpleMouseKeyboardPreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_into(OrbitCamBindings::builder())?.build()
    }
}
