use bevy::prelude::*;

use super::OrbitCamBlenderLikeKeyboardPreset;
use super::OrbitCamBlenderLikePreset;
use super::OrbitCamGamepadPreset;
use super::OrbitCamKeyboardPreset;
use super::OrbitCamSimpleMouseKeyboardPreset;
use super::OrbitCamSimpleMousePreset;
#[cfg(feature = "reflect-input-modes")]
use super::blender_like::OrbitCamBlenderLikePresetDraft;
#[cfg(feature = "reflect-input-modes")]
use super::blender_like_keyboard::OrbitCamBlenderLikeKeyboardPresetDraft;
#[cfg(feature = "reflect-input-modes")]
use super::gamepad::OrbitCamGamepadPresetDraft;
#[cfg(feature = "reflect-input-modes")]
use super::keyboard::OrbitCamKeyboardPresetDraft;
#[cfg(feature = "reflect-input-modes")]
use super::simple_mouse::OrbitCamSimpleMousePresetDraft;
#[cfg(feature = "reflect-input-modes")]
use super::simple_mouse_keyboard::OrbitCamSimpleMouseKeyboardPresetDraft;
use crate::input::bindings::OrbitCamBindings;
#[cfg(feature = "reflect-input-modes")]
use crate::input::bindings::OrbitCamSensitivity;
use crate::input::bindings::error::OrbitCamBindingsError;

/// Setting-insensitive identity for a built-in orbit-camera input preset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Default)]
#[non_exhaustive]
pub enum OrbitCamPresetKind {
    /// Mouse-oriented default controls.
    #[default]
    SimpleMouse,
    /// Editor-oriented controls modeled after Blender navigation.
    BlenderLike,
    /// Keyboard-only camera controls.
    Keyboard,
    /// Simple mouse controls plus keyboard camera controls.
    SimpleMouseKeyboard,
    /// Blender-like pointer controls plus keyboard camera controls.
    BlenderLikeKeyboard,
    /// Gamepad camera controls.
    Gamepad,
}

impl OrbitCamPresetKind {
    /// Returns the preset kind's display name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SimpleMouse => "SimpleMouse",
            Self::BlenderLike => "BlenderLike",
            Self::Keyboard => "Keyboard",
            Self::SimpleMouseKeyboard => "SimpleMouseKeyboard",
            Self::BlenderLikeKeyboard => "BlenderLikeKeyboard",
            Self::Gamepad => "Gamepad",
        }
    }
}

/// Reflected draft for a built-in orbit-camera input preset.
#[cfg(feature = "reflect-input-modes")]
#[derive(Clone, Debug, PartialEq, Reflect)]
#[reflect(Default)]
#[non_exhaustive]
pub enum OrbitCamPresetDraft {
    /// Mouse-oriented default controls.
    SimpleMouse(OrbitCamSimpleMousePresetDraft),
    /// Editor-oriented controls modeled after Blender navigation.
    BlenderLike(OrbitCamBlenderLikePresetDraft),
    /// Keyboard-only camera controls.
    Keyboard(OrbitCamKeyboardPresetDraft),
    /// Simple mouse controls plus keyboard camera controls.
    SimpleMouseKeyboard(OrbitCamSimpleMouseKeyboardPresetDraft),
    /// Blender-like pointer controls plus keyboard camera controls.
    BlenderLikeKeyboard(OrbitCamBlenderLikeKeyboardPresetDraft),
    /// Gamepad camera controls.
    Gamepad(OrbitCamGamepadPresetDraft),
}

#[cfg(feature = "reflect-input-modes")]
impl OrbitCamPresetDraft {
    /// Builds a reflected draft from an authored runtime preset payload.
    #[must_use]
    pub fn from_preset(preset: &OrbitCamPreset) -> Self { Self::from(preset) }

    /// Returns the preset draft's setting-insensitive identity.
    #[must_use]
    pub const fn kind(&self) -> OrbitCamPresetKind {
        match self {
            Self::SimpleMouse(_) => OrbitCamPresetKind::SimpleMouse,
            Self::BlenderLike(_) => OrbitCamPresetKind::BlenderLike,
            Self::Keyboard(_) => OrbitCamPresetKind::Keyboard,
            Self::SimpleMouseKeyboard(_) => OrbitCamPresetKind::SimpleMouseKeyboard,
            Self::BlenderLikeKeyboard(_) => OrbitCamPresetKind::BlenderLikeKeyboard,
            Self::Gamepad(_) => OrbitCamPresetKind::Gamepad,
        }
    }
}

#[cfg(feature = "reflect-input-modes")]
impl Default for OrbitCamPresetDraft {
    fn default() -> Self { Self::from_preset(&OrbitCamPreset::default()) }
}

#[cfg(feature = "reflect-input-modes")]
impl TryFrom<OrbitCamPresetDraft> for OrbitCamPreset {
    type Error = OrbitCamBindingsError;

    fn try_from(draft: OrbitCamPresetDraft) -> Result<Self, Self::Error> {
        match draft {
            OrbitCamPresetDraft::SimpleMouse(draft) => Ok(Self::SimpleMouse(draft.try_into()?)),
            OrbitCamPresetDraft::BlenderLike(draft) => Ok(Self::BlenderLike(draft.try_into()?)),
            OrbitCamPresetDraft::Keyboard(draft) => Ok(Self::Keyboard(draft.into())),
            OrbitCamPresetDraft::SimpleMouseKeyboard(draft) => {
                Ok(Self::SimpleMouseKeyboard(draft.try_into()?))
            },
            OrbitCamPresetDraft::BlenderLikeKeyboard(draft) => {
                Ok(Self::BlenderLikeKeyboard(draft.try_into()?))
            },
            OrbitCamPresetDraft::Gamepad(draft) => Ok(Self::Gamepad(draft.try_into()?)),
        }
    }
}

#[cfg(feature = "reflect-input-modes")]
impl From<&OrbitCamPreset> for OrbitCamPresetDraft {
    fn from(preset: &OrbitCamPreset) -> Self {
        match preset {
            OrbitCamPreset::SimpleMouse(preset) => Self::SimpleMouse((*preset).into()),
            OrbitCamPreset::BlenderLike(preset) => Self::BlenderLike((*preset).into()),
            OrbitCamPreset::Keyboard(preset) => Self::Keyboard((*preset).into()),
            OrbitCamPreset::SimpleMouseKeyboard(preset) => {
                Self::SimpleMouseKeyboard((*preset).into())
            },
            OrbitCamPreset::BlenderLikeKeyboard(preset) => {
                Self::BlenderLikeKeyboard((*preset).into())
            },
            OrbitCamPreset::Gamepad(preset) => Self::Gamepad((*preset).into()),
        }
    }
}

#[cfg(feature = "reflect-input-modes")]
impl From<OrbitCamPreset> for OrbitCamPresetDraft {
    fn from(preset: OrbitCamPreset) -> Self { Self::from(&preset) }
}

/// Reflected draft for per-action input sensitivity.
#[cfg(feature = "reflect-input-modes")]
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Default)]
pub struct OrbitCamSensitivityDraft {
    /// Orbit input multiplier.
    pub orbit: f32,
    /// Pan input multiplier.
    pub pan:   f32,
    /// Zoom input multiplier.
    pub zoom:  f32,
}

#[cfg(feature = "reflect-input-modes")]
impl Default for OrbitCamSensitivityDraft {
    fn default() -> Self {
        Self {
            orbit: 1.0,
            pan:   1.0,
            zoom:  1.0,
        }
    }
}

#[cfg(feature = "reflect-input-modes")]
impl TryFrom<OrbitCamSensitivityDraft> for OrbitCamSensitivity {
    type Error = OrbitCamBindingsError;

    fn try_from(draft: OrbitCamSensitivityDraft) -> Result<Self, Self::Error> {
        let sensitivity = Self::new()
            .orbit(draft.orbit)
            .pan(draft.pan)
            .zoom(draft.zoom);
        sensitivity.validate()?;
        Ok(sensitivity)
    }
}

#[cfg(feature = "reflect-input-modes")]
impl From<OrbitCamSensitivity> for OrbitCamSensitivityDraft {
    fn from(sensitivity: OrbitCamSensitivity) -> Self {
        Self {
            orbit: sensitivity.orbit_sensitivity().value(),
            pan:   sensitivity.pan_sensitivity().value(),
            zoom:  sensitivity.zoom_sensitivity().value(),
        }
    }
}

/// Built-in orbit-camera input presets.
#[derive(Clone, Debug, PartialEq, Reflect)]
#[reflect(Default)]
#[non_exhaustive]
pub enum OrbitCamPreset {
    /// Mouse-oriented default controls.
    SimpleMouse(OrbitCamSimpleMousePreset),
    /// Editor-oriented controls modeled after Blender navigation.
    BlenderLike(OrbitCamBlenderLikePreset),
    /// Keyboard-only camera controls.
    Keyboard(OrbitCamKeyboardPreset),
    /// Simple mouse controls plus keyboard camera controls.
    SimpleMouseKeyboard(OrbitCamSimpleMouseKeyboardPreset),
    /// Blender-like pointer controls plus keyboard camera controls.
    BlenderLikeKeyboard(OrbitCamBlenderLikeKeyboardPreset),
    /// Gamepad camera controls.
    Gamepad(OrbitCamGamepadPreset),
}

impl OrbitCamPreset {
    /// Builds the simple mouse input preset.
    #[must_use]
    pub fn simple_mouse() -> Self { OrbitCamSimpleMousePreset::default().into() }

    /// Builds the Blender-like input preset.
    #[must_use]
    pub fn blender_like() -> Self { OrbitCamBlenderLikePreset::default().into() }

    /// Builds the keyboard input preset.
    #[must_use]
    pub fn keyboard() -> Self { OrbitCamKeyboardPreset.into() }

    /// Builds the simple mouse plus keyboard input preset.
    #[must_use]
    pub fn simple_mouse_keyboard() -> Self { OrbitCamSimpleMouseKeyboardPreset::default().into() }

    /// Builds the Blender-like plus keyboard input preset.
    #[must_use]
    pub fn blender_like_keyboard() -> Self { OrbitCamBlenderLikeKeyboardPreset::default().into() }

    /// Builds the gamepad input preset.
    #[must_use]
    pub fn gamepad() -> Self { OrbitCamGamepadPreset::default().into() }

    /// Returns the preset's setting-insensitive identity.
    #[must_use]
    pub const fn kind(&self) -> OrbitCamPresetKind {
        match self {
            Self::SimpleMouse(_) => OrbitCamPresetKind::SimpleMouse,
            Self::BlenderLike(_) => OrbitCamPresetKind::BlenderLike,
            Self::Keyboard(_) => OrbitCamPresetKind::Keyboard,
            Self::SimpleMouseKeyboard(_) => OrbitCamPresetKind::SimpleMouseKeyboard,
            Self::BlenderLikeKeyboard(_) => OrbitCamPresetKind::BlenderLikeKeyboard,
            Self::Gamepad(_) => OrbitCamPresetKind::Gamepad,
        }
    }

    /// Returns the preset's display name.
    #[must_use]
    pub const fn name(&self) -> &'static str { self.kind().name() }

    /// Converts this preset into validated custom bindings.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] if the preset construction violates a
    /// binding invariant.
    pub fn to_bindings(&self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        match self {
            Self::SimpleMouse(preset) => preset.build(),
            Self::BlenderLike(preset) => preset.build(),
            Self::Keyboard(preset) => preset.build(),
            Self::SimpleMouseKeyboard(preset) => preset.build(),
            Self::BlenderLikeKeyboard(preset) => preset.build(),
            Self::Gamepad(preset) => preset.build(),
        }
    }
}

impl Default for OrbitCamPreset {
    fn default() -> Self { Self::simple_mouse() }
}

impl From<OrbitCamSimpleMousePreset> for OrbitCamPreset {
    fn from(preset: OrbitCamSimpleMousePreset) -> Self { Self::SimpleMouse(preset) }
}

impl From<OrbitCamBlenderLikePreset> for OrbitCamPreset {
    fn from(preset: OrbitCamBlenderLikePreset) -> Self { Self::BlenderLike(preset) }
}

impl From<OrbitCamKeyboardPreset> for OrbitCamPreset {
    fn from(preset: OrbitCamKeyboardPreset) -> Self { Self::Keyboard(preset) }
}

impl From<OrbitCamSimpleMouseKeyboardPreset> for OrbitCamPreset {
    fn from(preset: OrbitCamSimpleMouseKeyboardPreset) -> Self { Self::SimpleMouseKeyboard(preset) }
}

impl From<OrbitCamBlenderLikeKeyboardPreset> for OrbitCamPreset {
    fn from(preset: OrbitCamBlenderLikeKeyboardPreset) -> Self { Self::BlenderLikeKeyboard(preset) }
}

impl From<OrbitCamGamepadPreset> for OrbitCamPreset {
    fn from(preset: OrbitCamGamepadPreset) -> Self { Self::Gamepad(preset) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_return_default_payload_variants() {
        assert_eq!(
            OrbitCamPreset::simple_mouse(),
            OrbitCamPreset::SimpleMouse(OrbitCamSimpleMousePreset::default())
        );
        assert_eq!(
            OrbitCamPreset::blender_like(),
            OrbitCamPreset::BlenderLike(OrbitCamBlenderLikePreset::default())
        );
        assert_eq!(
            OrbitCamPreset::keyboard(),
            OrbitCamPreset::Keyboard(OrbitCamKeyboardPreset)
        );
        assert_eq!(
            OrbitCamPreset::simple_mouse_keyboard(),
            OrbitCamPreset::SimpleMouseKeyboard(OrbitCamSimpleMouseKeyboardPreset::default())
        );
        assert_eq!(
            OrbitCamPreset::blender_like_keyboard(),
            OrbitCamPreset::BlenderLikeKeyboard(OrbitCamBlenderLikeKeyboardPreset::default())
        );
        assert_eq!(
            OrbitCamPreset::gamepad(),
            OrbitCamPreset::Gamepad(OrbitCamGamepadPreset::default())
        );
    }

    #[test]
    fn blender_like_kind_reports_identity_name() {
        let preset = OrbitCamPreset::blender_like();

        assert_eq!(preset.kind(), OrbitCamPresetKind::BlenderLike);
        assert_eq!(preset.kind().name(), "BlenderLike");
        assert_eq!(preset.name(), "BlenderLike");
    }
}
