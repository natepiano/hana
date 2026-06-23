use bevy::prelude::*;
use bevy_enhanced_input::prelude::ModKeys;

use super::config::OrbitCamPresetConfig;
use super::source_input_gain::MouseInputGain;
use super::source_input_gain::SmoothScrollInputGain;
use crate::input::bindings::OrbitCamBindings;
use crate::input::bindings::OrbitCamBindingsBuilder;
use crate::input::bindings::OrbitCamInputGain;
use crate::input::bindings::OrbitCamMouseDrag;
use crate::input::bindings::OrbitCamMouseWheelZoom;
use crate::input::bindings::OrbitCamPinchZoom;
use crate::input::bindings::OrbitCamScalePolicy;
use crate::input::bindings::OrbitCamSlowMode;
use crate::input::bindings::OrbitCamTrackpadScroll;
use crate::input::bindings::error::OrbitCamBindingsError;

/// Configures Blender-style pointer and smooth-scroll camera controls.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Default)]
pub struct OrbitCamBlenderLikePreset {
    mouse_input_gain:         OrbitCamInputGain,
    smooth_scroll_input_gain: OrbitCamInputGain,
    zoom_mod_keys:            ModKeys,
    slow_toggle_key:          Option<KeyCode>,
    slow_toggle_mod_keys:     ModKeys,
    slow_scale:               f32,
}

impl OrbitCamBlenderLikePreset {
    const DEFAULT_NORMAL_SCALE: f32 = 1.0;
    const DEFAULT_SLOW_SCALE: f32 = 0.05;
    const MAX_SLOW_SCALE: f32 = 1.0;
    const MIN_SLOW_SCALE: f32 = 0.0;

    /// Builds the Blender-like preset.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when generated descriptors fail
    /// validation.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        <Self as OrbitCamPresetConfig>::build(self)
    }

    /// Sets the keyboard modifiers required for trackpad zoom.
    #[must_use]
    pub const fn zoom_mod_keys(mut self, zoom_mod_keys: ModKeys) -> Self {
        self.zoom_mod_keys = zoom_mod_keys;
        self
    }

    /// Sets the key that toggles slow mode on or off for this camera.
    #[must_use]
    pub const fn slow_toggle_key(mut self, slow_toggle_key: Option<KeyCode>) -> Self {
        self.slow_toggle_key = slow_toggle_key;
        self
    }

    /// Sets the modifier keys held with the toggle key to fire the slow-mode toggle.
    #[must_use]
    pub const fn slow_toggle_mod_keys(mut self, slow_toggle_mod_keys: ModKeys) -> Self {
        self.slow_toggle_mod_keys = slow_toggle_mod_keys;
        self
    }

    /// Sets the scale applied to all inputs when slow mode is active.
    #[must_use]
    pub const fn slow_scale(mut self, slow_scale: f32) -> Self {
        self.slow_scale = slow_scale;
        self
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
        self.smooth_scroll_input_gain.validate()?;
        if self.slow_toggle_key.is_some()
            && (!self.slow_scale.is_finite()
                || self.slow_scale <= Self::MIN_SLOW_SCALE
                || self.slow_scale > Self::MAX_SLOW_SCALE)
        {
            return Err(OrbitCamBindingsError::InvalidScale);
        }
        Ok(())
    }

    fn add_to(self, builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
        let builder = if let Some(toggle_key) = self.slow_toggle_key {
            builder.slow_mode(OrbitCamSlowMode {
                toggle_key,
                mod_keys: self.slow_toggle_mod_keys,
                scale: OrbitCamScalePolicy {
                    normal: Self::DEFAULT_NORMAL_SCALE,
                    slow:   self.slow_scale,
                },
            })
        } else {
            builder
        };

        builder
            .orbit(
                OrbitCamMouseDrag::new(MouseButton::Middle)
                    .with_input_gain(self.mouse_input_gain.orbit_input_gain().value()),
            )
            .orbit(
                OrbitCamTrackpadScroll::default()
                    .with_input_gain(self.smooth_scroll_input_gain.orbit_input_gain().value()),
            )
            .pan(
                OrbitCamMouseDrag::new(MouseButton::Middle)
                    .with_mod_keys(ModKeys::SHIFT)
                    .with_input_gain(self.mouse_input_gain.pan_input_gain().value()),
            )
            .pan(
                OrbitCamTrackpadScroll::default()
                    .with_mod_keys(ModKeys::SHIFT)
                    .with_input_gain(self.smooth_scroll_input_gain.pan_input_gain().value()),
            )
            .zoom(
                OrbitCamMouseWheelZoom
                    .with_input_gain(self.mouse_input_gain.zoom_input_gain().value()),
            )
            .zoom(
                OrbitCamTrackpadScroll::default()
                    .with_mod_keys(self.zoom_mod_keys)
                    .with_input_gain(self.smooth_scroll_input_gain.zoom_input_gain().value()),
            )
            .zoom(OrbitCamPinchZoom)
    }
}

impl Default for OrbitCamBlenderLikePreset {
    fn default() -> Self {
        Self {
            mouse_input_gain:         OrbitCamInputGain::default(),
            smooth_scroll_input_gain: OrbitCamInputGain::default(),
            zoom_mod_keys:            ModKeys::CONTROL,
            slow_toggle_key:          Some(KeyCode::KeyS),
            slow_toggle_mod_keys:     ModKeys::ALT,
            slow_scale:               Self::DEFAULT_SLOW_SCALE,
        }
    }
}

impl MouseInputGain for OrbitCamBlenderLikePreset {
    type Gain = OrbitCamInputGain;

    fn mouse_input_gain(self, input_gain: Self::Gain) -> Self {
        Self::mouse_input_gain(self, input_gain)
    }
}

impl SmoothScrollInputGain for OrbitCamBlenderLikePreset {
    type Gain = OrbitCamInputGain;

    fn smooth_scroll_input_gain(self, input_gain: Self::Gain) -> Self {
        Self::smooth_scroll_input_gain(self, input_gain)
    }
}

impl OrbitCamPresetConfig for OrbitCamBlenderLikePreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_into(OrbitCamBindings::builder())?.build()
    }
}
