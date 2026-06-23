use bevy::prelude::*;

use super::config::OrbitCamPresetConfig;
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
