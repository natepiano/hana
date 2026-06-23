use bevy::prelude::*;

use super::config::OrbitCamPresetConfig;
use super::source_input_gain::MouseInputGain;
use super::source_input_gain::SmoothScrollInputGain;
use crate::input::bindings::OrbitCamBindings;
use crate::input::bindings::OrbitCamBindingsBuilder;
use crate::input::bindings::OrbitCamInputGain;
use crate::input::bindings::OrbitCamMouseDrag;
use crate::input::bindings::OrbitCamMouseWheelZoom;
use crate::input::bindings::OrbitCamPinchZoom;
use crate::input::bindings::OrbitCamTrackpadScroll;
use crate::input::bindings::error::OrbitCamBindingsError;

/// Configures the default mouse-oriented orbit-camera preset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Default)]
pub struct OrbitCamSimpleMousePreset {
    mouse_input_gain:         OrbitCamInputGain,
    smooth_scroll_input_gain: OrbitCamInputGain,
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

    /// Sets source input gain for mouse-drag and line-wheel input.
    #[must_use]
    pub const fn mouse_input_gain(mut self, input_gain: OrbitCamInputGain) -> Self {
        self.mouse_input_gain = input_gain;
        self
    }

    /// Sets source input gain for Bevy pixel-scroll input.
    #[must_use]
    pub const fn smooth_scroll_input_gain(mut self, input_gain: OrbitCamInputGain) -> Self {
        self.smooth_scroll_input_gain = input_gain;
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
        self.mouse_input_gain.validate()?;
        self.smooth_scroll_input_gain.validate()
    }

    fn add_to(self, builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
        builder
            .orbit(
                OrbitCamMouseDrag::new(MouseButton::Left)
                    .with_input_gain(self.mouse_input_gain.orbit_input_gain().value()),
            )
            .pan(
                OrbitCamMouseDrag::new(MouseButton::Right)
                    .with_input_gain(self.mouse_input_gain.pan_input_gain().value()),
            )
            .zoom(
                OrbitCamMouseWheelZoom
                    .with_input_gain(self.mouse_input_gain.zoom_input_gain().value()),
            )
            .zoom(
                OrbitCamTrackpadScroll::default()
                    .with_input_gain(self.smooth_scroll_input_gain.zoom_input_gain().value()),
            )
            .zoom(OrbitCamPinchZoom)
    }
}

impl MouseInputGain for OrbitCamSimpleMousePreset {
    type Gain = OrbitCamInputGain;

    fn mouse_input_gain(self, input_gain: Self::Gain) -> Self {
        Self::mouse_input_gain(self, input_gain)
    }
}

impl SmoothScrollInputGain for OrbitCamSimpleMousePreset {
    type Gain = OrbitCamInputGain;

    fn smooth_scroll_input_gain(self, input_gain: Self::Gain) -> Self {
        Self::smooth_scroll_input_gain(self, input_gain)
    }
}

impl OrbitCamPresetConfig for OrbitCamSimpleMousePreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_into(OrbitCamBindings::builder())?.build()
    }
}
