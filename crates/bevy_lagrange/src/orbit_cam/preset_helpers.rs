use bevy::prelude::*;

use super::OrbitCam;
use crate::input::OrbitCamBindings;
use crate::input::OrbitCamInputMode;
use crate::input::OrbitCamPreset;

impl OrbitCam {
    /// Returns an `OrbitCam` bundle using the simple mouse input preset.
    #[must_use]
    pub fn simple_mouse() -> impl Bundle {
        (
            Self::default(),
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        )
    }

    /// Returns an `OrbitCam` bundle using the Blender-like input preset.
    #[must_use]
    pub fn blender_like() -> impl Bundle {
        (
            Self::default(),
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        )
    }

    /// Returns an `OrbitCam` bundle using the gamepad input preset.
    #[must_use]
    pub fn gamepad() -> impl Bundle {
        (
            Self::default(),
            OrbitCamInputMode::Preset(OrbitCamPreset::Gamepad),
        )
    }

    /// Returns an `OrbitCam` bundle using the keyboard input preset.
    #[must_use]
    pub fn keyboard() -> impl Bundle {
        (
            Self::default(),
            OrbitCamInputMode::Preset(OrbitCamPreset::Keyboard),
        )
    }

    /// Returns an `OrbitCam` bundle using simple mouse and keyboard input presets.
    #[must_use]
    pub fn simple_mouse_keyboard() -> impl Bundle {
        (
            Self::default(),
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouseKeyboard),
        )
    }

    /// Returns an `OrbitCam` bundle using Blender-like and keyboard input presets.
    #[must_use]
    pub fn blender_like_keyboard() -> impl Bundle {
        (
            Self::default(),
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLikeKeyboard),
        )
    }

    /// Returns an `OrbitCam` bundle using app-owned validated bindings.
    #[must_use]
    pub fn with_bindings(bindings: OrbitCamBindings) -> impl Bundle {
        (Self::default(), OrbitCamInputMode::Bindings(bindings))
    }

    /// Returns an `OrbitCam` bundle using manual app-authored input.
    #[must_use]
    pub fn manual() -> impl Bundle { (Self::default(), OrbitCamInputMode::Manual) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::OrbitCamBindingsError;

    #[test]
    fn preset_helpers_insert_requested_input_modes() {
        let mut world = World::new();
        let simple_mouse = world.spawn(OrbitCam::simple_mouse()).id();
        let blender_like = world.spawn(OrbitCam::blender_like()).id();
        let manual = world.spawn(OrbitCam::manual()).id();

        assert_eq!(
            world.get::<OrbitCamInputMode>(simple_mouse),
            Some(&OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse))
        );
        assert_eq!(
            world.get::<OrbitCamInputMode>(blender_like),
            Some(&OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike))
        );
        assert_eq!(
            world.get::<OrbitCamInputMode>(manual),
            Some(&OrbitCamInputMode::Manual)
        );
    }

    #[test]
    fn with_bindings_inserts_bindings_input_mode() -> Result<(), OrbitCamBindingsError> {
        let mut world = World::new();
        let bindings = OrbitCamPreset::SimpleMouse.to_bindings()?;
        let camera = world.spawn(OrbitCam::with_bindings(bindings)).id();

        assert!(matches!(
            world.get::<OrbitCamInputMode>(camera),
            Some(OrbitCamInputMode::Bindings(_))
        ));

        Ok(())
    }
}
