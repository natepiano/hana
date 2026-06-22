use bevy::prelude::*;
use bevy_enhanced_input::prelude::ModKeys;

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
use crate::input::bindings::OrbitCamScalePolicy;
use crate::input::bindings::OrbitCamSensitivity;
use crate::input::bindings::OrbitCamSlowMode;
use crate::input::bindings::OrbitCamTrackpadScroll;
use crate::input::bindings::error::OrbitCamBindingsError;

/// Configures Blender-style pointer and smooth-scroll camera controls.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Default)]
pub struct OrbitCamBlenderLikePreset {
    mouse_sensitivity:         OrbitCamSensitivity,
    smooth_scroll_sensitivity: OrbitCamSensitivity,
    zoom_mod_keys:             ModKeys,
    slow_toggle_key:           Option<KeyCode>,
    slow_toggle_mod_keys:      ModKeys,
    slow_scale:                f32,
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
        self.smooth_scroll_sensitivity.validate()?;
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
                    .with_sensitivity(self.mouse_sensitivity.orbit_sensitivity().value()),
            )
            .orbit(
                OrbitCamTrackpadScroll::default()
                    .with_sensitivity(self.smooth_scroll_sensitivity.orbit_sensitivity().value()),
            )
            .pan(
                OrbitCamMouseDrag::new(MouseButton::Middle)
                    .with_mod_keys(ModKeys::SHIFT)
                    .with_sensitivity(self.mouse_sensitivity.pan_sensitivity().value()),
            )
            .pan(
                OrbitCamTrackpadScroll::default()
                    .with_mod_keys(ModKeys::SHIFT)
                    .with_sensitivity(self.smooth_scroll_sensitivity.pan_sensitivity().value()),
            )
            .zoom(
                OrbitCamMouseWheelZoom
                    .with_sensitivity(self.mouse_sensitivity.zoom_sensitivity().value()),
            )
            .zoom(
                OrbitCamTrackpadScroll::default()
                    .with_mod_keys(self.zoom_mod_keys)
                    .with_sensitivity(self.smooth_scroll_sensitivity.zoom_sensitivity().value()),
            )
            .zoom(OrbitCamPinchZoom)
    }
}

impl Default for OrbitCamBlenderLikePreset {
    fn default() -> Self {
        Self {
            mouse_sensitivity:         OrbitCamSensitivity::default(),
            smooth_scroll_sensitivity: OrbitCamSensitivity::default(),
            zoom_mod_keys:             ModKeys::CONTROL,
            slow_toggle_key:           Some(KeyCode::KeyS),
            slow_toggle_mod_keys:      ModKeys::ALT,
            slow_scale:                Self::DEFAULT_SLOW_SCALE,
        }
    }
}

impl MouseSensitivity for OrbitCamBlenderLikePreset {
    type Sensitivity = OrbitCamSensitivity;

    fn mouse_sensitivity(self, sensitivity: Self::Sensitivity) -> Self {
        Self::mouse_sensitivity(self, sensitivity)
    }
}

impl SmoothScrollSensitivity for OrbitCamBlenderLikePreset {
    type Sensitivity = OrbitCamSensitivity;

    fn smooth_scroll_sensitivity(self, sensitivity: Self::Sensitivity) -> Self {
        Self::smooth_scroll_sensitivity(self, sensitivity)
    }
}

impl OrbitCamPresetConfig for OrbitCamBlenderLikePreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_into(OrbitCamBindings::builder())?.build()
    }
}

/// Reflected draft for the Blender-like preset payload.
#[cfg(feature = "reflect-input-modes")]
#[derive(Clone, Debug, PartialEq, Reflect)]
#[reflect(Default)]
pub struct OrbitCamBlenderLikePresetDraft {
    /// Source sensitivity for mouse-drag and line-wheel input.
    pub mouse_sensitivity:         OrbitCamSensitivityDraft,
    /// Source sensitivity for Bevy pixel-scroll input.
    pub smooth_scroll_sensitivity: OrbitCamSensitivityDraft,
    /// Keyboard modifiers required for trackpad zoom.
    pub zoom_mod_keys:             ModKeys,
    /// Key that toggles slow mode on or off for this camera.
    pub slow_toggle_key:           Option<KeyCode>,
    /// Modifier keys held with the toggle key to fire the slow-mode toggle.
    pub slow_toggle_mod_keys:      ModKeys,
    /// Scale applied to all inputs when slow mode is active.
    pub slow_scale:                f32,
}

#[cfg(feature = "reflect-input-modes")]
impl Default for OrbitCamBlenderLikePresetDraft {
    fn default() -> Self {
        let preset = OrbitCamBlenderLikePreset::default();
        Self::from(preset)
    }
}

#[cfg(feature = "reflect-input-modes")]
impl TryFrom<OrbitCamBlenderLikePresetDraft> for OrbitCamBlenderLikePreset {
    type Error = OrbitCamBindingsError;

    fn try_from(draft: OrbitCamBlenderLikePresetDraft) -> Result<Self, Self::Error> {
        Ok(Self {
            mouse_sensitivity:         draft.mouse_sensitivity.try_into()?,
            smooth_scroll_sensitivity: draft.smooth_scroll_sensitivity.try_into()?,
            zoom_mod_keys:             draft.zoom_mod_keys,
            slow_toggle_key:           draft.slow_toggle_key,
            slow_toggle_mod_keys:      draft.slow_toggle_mod_keys,
            slow_scale:                draft.slow_scale,
        })
    }
}

#[cfg(feature = "reflect-input-modes")]
impl From<OrbitCamBlenderLikePreset> for OrbitCamBlenderLikePresetDraft {
    fn from(preset: OrbitCamBlenderLikePreset) -> Self {
        Self {
            mouse_sensitivity:         preset.mouse_sensitivity.into(),
            smooth_scroll_sensitivity: preset.smooth_scroll_sensitivity.into(),
            zoom_mod_keys:             preset.zoom_mod_keys,
            slow_toggle_key:           preset.slow_toggle_key,
            slow_toggle_mod_keys:      preset.slow_toggle_mod_keys,
            slow_scale:                preset.slow_scale,
        }
    }
}
