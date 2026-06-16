use bevy::prelude::*;

use super::config::OrbitCamPresetConfig;
use super::enum_preset::OrbitCamBindingsProfile;
use crate::input::bindings::OrbitCamBindings;
use crate::input::bindings::OrbitCamBindingsBuilder;
use crate::input::bindings::OrbitCamInputBinding;
use crate::input::bindings::error::OrbitCamBindingsError;

/// Configures keyboard-only orbit-camera controls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct OrbitCamKeyboardPreset;

impl OrbitCamKeyboardPreset {
    /// Builds the keyboard preset.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when generated descriptors fail
    /// validation.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        <Self as OrbitCamPresetConfig>::build(self)
    }

    pub(super) fn build_into(self, builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
        let Self = self;
        Self::add_to(builder)
    }

    fn add_to(builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
        let orbit_keys = OrbitCamInputBinding::cardinal_keys(
            KeyCode::ArrowUp,
            KeyCode::ArrowRight,
            KeyCode::ArrowDown,
            KeyCode::ArrowLeft,
        );
        let pan_keys = OrbitCamInputBinding::cardinal_keys(
            KeyCode::KeyW,
            KeyCode::KeyD,
            KeyCode::KeyS,
            KeyCode::KeyA,
        );
        let zoom_keys = OrbitCamInputBinding::bidirectional_keys(KeyCode::Equal, KeyCode::Minus);
        builder.orbit(orbit_keys).pan(pan_keys).zoom(zoom_keys)
    }
}

impl OrbitCamPresetConfig for OrbitCamKeyboardPreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_into(OrbitCamBindings::builder())
            .profile(OrbitCamBindingsProfile::KeyboardPreset { customized: false })
            .build()
    }
}
