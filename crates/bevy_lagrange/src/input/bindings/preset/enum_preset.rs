use bevy::prelude::*;

use super::OrbitCamBlenderLikeKeyboardPreset;
use super::OrbitCamBlenderLikePreset;
use super::OrbitCamGamepadPreset;
use super::OrbitCamKeyboardPreset;
use super::OrbitCamSimpleMouseKeyboardPreset;
use super::OrbitCamSimpleMousePreset;
use crate::input::bindings::OrbitCamBindings;
use crate::input::bindings::error::OrbitCamBindingsError;

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
    /// Converts this preset into validated custom bindings.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] if the preset construction violates a
    /// binding invariant.
    pub fn to_bindings(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
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
