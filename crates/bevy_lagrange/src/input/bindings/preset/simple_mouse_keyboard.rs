use bevy::prelude::*;

use super::config::OrbitCamPresetConfig;
use super::keyboard::OrbitCamKeyboardPreset;
#[cfg(feature = "reflect-input-modes")]
use super::keyboard::OrbitCamKeyboardPresetDraft;
use super::simple_mouse::OrbitCamSimpleMousePreset;
#[cfg(feature = "reflect-input-modes")]
use super::simple_mouse::OrbitCamSimpleMousePresetDraft;
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

/// Reflected draft for the simple mouse plus keyboard preset payload.
#[cfg(feature = "reflect-input-modes")]
#[derive(Clone, Debug, PartialEq, Reflect)]
#[reflect(Default)]
pub struct OrbitCamSimpleMouseKeyboardPresetDraft {
    /// Simple mouse child preset draft.
    pub pointer:  OrbitCamSimpleMousePresetDraft,
    /// Keyboard child preset draft.
    pub keyboard: OrbitCamKeyboardPresetDraft,
}

#[cfg(feature = "reflect-input-modes")]
impl Default for OrbitCamSimpleMouseKeyboardPresetDraft {
    fn default() -> Self { Self::from(OrbitCamSimpleMouseKeyboardPreset::default()) }
}

#[cfg(feature = "reflect-input-modes")]
impl TryFrom<OrbitCamSimpleMouseKeyboardPresetDraft> for OrbitCamSimpleMouseKeyboardPreset {
    type Error = OrbitCamBindingsError;

    fn try_from(draft: OrbitCamSimpleMouseKeyboardPresetDraft) -> Result<Self, Self::Error> {
        Ok(Self {
            pointer:  draft.pointer.try_into()?,
            keyboard: draft.keyboard.into(),
        })
    }
}

#[cfg(feature = "reflect-input-modes")]
impl From<OrbitCamSimpleMouseKeyboardPreset> for OrbitCamSimpleMouseKeyboardPresetDraft {
    fn from(preset: OrbitCamSimpleMouseKeyboardPreset) -> Self {
        Self {
            pointer:  preset.pointer.into(),
            keyboard: preset.keyboard.into(),
        }
    }
}
