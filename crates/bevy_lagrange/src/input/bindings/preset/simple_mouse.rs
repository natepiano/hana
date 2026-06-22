use bevy::prelude::*;

use super::config::OrbitCamPresetConfig;
#[cfg(feature = "reflect-input-modes")]
use super::enum_preset::OrbitCamSensitivityDraft;
use super::source_sensitivity::MouseSensitivity;
use super::source_sensitivity::SmoothScrollSensitivity;
use crate::input::bindings::OrbitCamBindings;
use crate::input::bindings::OrbitCamBindingsBuilder;
use crate::input::bindings::OrbitCamMouseDrag;
use crate::input::bindings::OrbitCamMouseWheelZoom;
use crate::input::bindings::OrbitCamPinchZoom;
use crate::input::bindings::OrbitCamSensitivity;
use crate::input::bindings::OrbitCamTrackpadScroll;
use crate::input::bindings::error::OrbitCamBindingsError;

/// Configures the default mouse-oriented orbit-camera preset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Default)]
pub struct OrbitCamSimpleMousePreset {
    mouse_sensitivity:         OrbitCamSensitivity,
    smooth_scroll_sensitivity: OrbitCamSensitivity,
}

impl OrbitCamSimpleMousePreset {
    /// Builds the simple mouse preset.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when generated descriptors fail
    /// validation.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        <Self as OrbitCamPresetConfig>::build(self)
    }

    /// Sets source sensitivity for mouse-drag and line-wheel input.
    #[must_use]
    pub const fn mouse_sensitivity(mut self, sensitivity: OrbitCamSensitivity) -> Self {
        self.mouse_sensitivity = sensitivity;
        self
    }

    /// Sets source sensitivity for Bevy pixel-scroll input.
    #[must_use]
    pub const fn smooth_scroll_sensitivity(mut self, sensitivity: OrbitCamSensitivity) -> Self {
        self.smooth_scroll_sensitivity = sensitivity;
        self
    }

    pub(super) fn build_into(
        self,
        builder: OrbitCamBindingsBuilder,
    ) -> Result<OrbitCamBindingsBuilder, OrbitCamBindingsError> {
        self.validate()?;
        Ok(self.add_to(builder))
    }

    fn validate(&self) -> Result<(), OrbitCamBindingsError> {
        self.mouse_sensitivity.validate()?;
        self.smooth_scroll_sensitivity.validate()
    }

    fn add_to(self, builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
        builder
            .orbit(
                OrbitCamMouseDrag::new(MouseButton::Left)
                    .with_sensitivity(self.mouse_sensitivity.orbit_sensitivity().value()),
            )
            .pan(
                OrbitCamMouseDrag::new(MouseButton::Right)
                    .with_sensitivity(self.mouse_sensitivity.pan_sensitivity().value()),
            )
            .zoom(
                OrbitCamMouseWheelZoom
                    .with_sensitivity(self.mouse_sensitivity.zoom_sensitivity().value()),
            )
            .zoom(
                OrbitCamTrackpadScroll::default()
                    .with_sensitivity(self.smooth_scroll_sensitivity.zoom_sensitivity().value()),
            )
            .zoom(OrbitCamPinchZoom)
    }
}

impl MouseSensitivity for OrbitCamSimpleMousePreset {
    type Sensitivity = OrbitCamSensitivity;

    fn mouse_sensitivity(self, sensitivity: Self::Sensitivity) -> Self {
        Self::mouse_sensitivity(self, sensitivity)
    }
}

impl SmoothScrollSensitivity for OrbitCamSimpleMousePreset {
    type Sensitivity = OrbitCamSensitivity;

    fn smooth_scroll_sensitivity(self, sensitivity: Self::Sensitivity) -> Self {
        Self::smooth_scroll_sensitivity(self, sensitivity)
    }
}

impl OrbitCamPresetConfig for OrbitCamSimpleMousePreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_into(OrbitCamBindings::builder())?.build()
    }
}

/// Reflected draft for the simple mouse preset payload.
#[cfg(feature = "reflect-input-modes")]
#[derive(Clone, Debug, PartialEq, Reflect)]
#[reflect(Default)]
pub struct OrbitCamSimpleMousePresetDraft {
    /// Source sensitivity for mouse-drag and line-wheel input.
    pub mouse_sensitivity:         OrbitCamSensitivityDraft,
    /// Source sensitivity for Bevy pixel-scroll input.
    pub smooth_scroll_sensitivity: OrbitCamSensitivityDraft,
}

#[cfg(feature = "reflect-input-modes")]
impl Default for OrbitCamSimpleMousePresetDraft {
    fn default() -> Self { Self::from(OrbitCamSimpleMousePreset::default()) }
}

#[cfg(feature = "reflect-input-modes")]
impl TryFrom<OrbitCamSimpleMousePresetDraft> for OrbitCamSimpleMousePreset {
    type Error = OrbitCamBindingsError;

    fn try_from(draft: OrbitCamSimpleMousePresetDraft) -> Result<Self, Self::Error> {
        Ok(Self {
            mouse_sensitivity:         draft.mouse_sensitivity.try_into()?,
            smooth_scroll_sensitivity: draft.smooth_scroll_sensitivity.try_into()?,
        })
    }
}

#[cfg(feature = "reflect-input-modes")]
impl From<OrbitCamSimpleMousePreset> for OrbitCamSimpleMousePresetDraft {
    fn from(preset: OrbitCamSimpleMousePreset) -> Self {
        Self {
            mouse_sensitivity:         preset.mouse_sensitivity.into(),
            smooth_scroll_sensitivity: preset.smooth_scroll_sensitivity.into(),
        }
    }
}
