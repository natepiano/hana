use bevy::prelude::*;
use bevy_enhanced_input::prelude::ModKeys;

use super::config::OrbitCamPresetConfig;
use super::enum_preset::OrbitCamBindingsProfile;
use super::enum_preset::PresetLayerSet;
use crate::input::bindings::OrbitCamBindings;
use crate::input::bindings::OrbitCamBindingsBuilder;
use crate::input::bindings::OrbitCamMouseDrag;
use crate::input::bindings::OrbitCamMouseWheelZoom;
use crate::input::bindings::OrbitCamPinchZoom;
use crate::input::bindings::OrbitCamTrackpadScroll;
use crate::input::bindings::error::OrbitCamBindingsError;

/// Configures Blender-style pointer and smooth-scroll camera controls.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct OrbitCamBlenderLikePreset {
    zoom_mod_keys:   ModKeys,
    slow_toggle_key: Option<KeyCode>,
    slow_scale:      f32,
}

impl OrbitCamBlenderLikePreset {
    const DEFAULT_SLOW_SCALE: f32 = 0.15;
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

    /// Sets the scale applied to all inputs when slow mode is active.
    #[must_use]
    pub const fn slow_scale(mut self, slow_scale: f32) -> Self {
        self.slow_scale = slow_scale;
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
        builder
            .orbit(OrbitCamMouseDrag::new(MouseButton::Middle))
            .orbit(OrbitCamTrackpadScroll::default())
            .pan(OrbitCamMouseDrag::new(MouseButton::Middle).with_mod_keys(ModKeys::SHIFT))
            .pan(OrbitCamTrackpadScroll::default().with_mod_keys(ModKeys::SHIFT))
            .zoom(OrbitCamMouseWheelZoom)
            .zoom(OrbitCamTrackpadScroll::default().with_mod_keys(self.zoom_mod_keys))
            .zoom(OrbitCamPinchZoom)
    }
}

impl Default for OrbitCamBlenderLikePreset {
    fn default() -> Self {
        Self {
            zoom_mod_keys:   ModKeys::CONTROL,
            slow_toggle_key: Some(KeyCode::CapsLock),
            slow_scale:      Self::DEFAULT_SLOW_SCALE,
        }
    }
}

impl OrbitCamPresetConfig for OrbitCamBlenderLikePreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_into(OrbitCamBindings::builder())?
            .profile(OrbitCamBindingsProfile::LayeredPreset {
                layers: PresetLayerSet::blender_like(),
            })
            .build()
    }
}
