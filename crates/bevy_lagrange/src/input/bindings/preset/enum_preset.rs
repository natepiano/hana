use bevy::prelude::*;

use super::OrbitCamBlenderLikeKeyboardPreset;
use super::OrbitCamBlenderLikePreset;
use super::OrbitCamGamepadPreset;
use super::OrbitCamKeyboardPreset;
use super::OrbitCamSimpleMouseKeyboardPreset;
use super::OrbitCamSimpleMousePreset;
use crate::input::bindings::OrbitCamBindings;
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

/// Built-in orbit-camera input presets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Default)]
#[non_exhaustive]
pub enum OrbitCamPreset {
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

impl OrbitCamPreset {
    /// Builds the simple mouse input preset.
    #[must_use]
    pub const fn simple_mouse() -> Self { Self::SimpleMouse }

    /// Builds the Blender-like input preset.
    #[must_use]
    pub const fn blender_like() -> Self { Self::BlenderLike }

    /// Builds the keyboard input preset.
    #[must_use]
    pub const fn keyboard() -> Self { Self::Keyboard }

    /// Builds the simple mouse plus keyboard input preset.
    #[must_use]
    pub const fn simple_mouse_keyboard() -> Self { Self::SimpleMouseKeyboard }

    /// Builds the Blender-like plus keyboard input preset.
    #[must_use]
    pub const fn blender_like_keyboard() -> Self { Self::BlenderLikeKeyboard }

    /// Builds the gamepad input preset.
    #[must_use]
    pub const fn gamepad() -> Self { Self::Gamepad }

    /// Returns the preset's setting-insensitive identity.
    #[must_use]
    pub const fn kind(&self) -> OrbitCamPresetKind {
        match self {
            Self::SimpleMouse => OrbitCamPresetKind::SimpleMouse,
            Self::BlenderLike => OrbitCamPresetKind::BlenderLike,
            Self::Keyboard => OrbitCamPresetKind::Keyboard,
            Self::SimpleMouseKeyboard => OrbitCamPresetKind::SimpleMouseKeyboard,
            Self::BlenderLikeKeyboard => OrbitCamPresetKind::BlenderLikeKeyboard,
            Self::Gamepad => OrbitCamPresetKind::Gamepad,
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
            Self::SimpleMouse => OrbitCamSimpleMousePreset.build(),
            Self::BlenderLike => OrbitCamBlenderLikePreset::default().build(),
            Self::Keyboard => OrbitCamKeyboardPreset.build(),
            Self::SimpleMouseKeyboard => OrbitCamSimpleMouseKeyboardPreset::default().build(),
            Self::BlenderLikeKeyboard => OrbitCamBlenderLikeKeyboardPreset::default().build(),
            Self::Gamepad => OrbitCamGamepadPreset::default().build(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_return_current_unit_variants() {
        assert_eq!(OrbitCamPreset::simple_mouse(), OrbitCamPreset::SimpleMouse);
        assert_eq!(OrbitCamPreset::blender_like(), OrbitCamPreset::BlenderLike);
        assert_eq!(OrbitCamPreset::keyboard(), OrbitCamPreset::Keyboard);
        assert_eq!(
            OrbitCamPreset::simple_mouse_keyboard(),
            OrbitCamPreset::SimpleMouseKeyboard
        );
        assert_eq!(
            OrbitCamPreset::blender_like_keyboard(),
            OrbitCamPreset::BlenderLikeKeyboard
        );
        assert_eq!(OrbitCamPreset::gamepad(), OrbitCamPreset::Gamepad);
    }

    #[test]
    fn blender_like_kind_reports_identity_name() {
        let preset = OrbitCamPreset::blender_like();

        assert_eq!(preset.kind(), OrbitCamPresetKind::BlenderLike);
        assert_eq!(preset.kind().name(), "BlenderLike");
        assert_eq!(preset.name(), "BlenderLike");
    }
}
