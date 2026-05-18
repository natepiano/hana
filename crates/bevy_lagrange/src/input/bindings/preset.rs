//! Built-in orbit-camera input presets that compile down to validated
//! [`super::OrbitCamBindings`].
//!
//! Types:
//! - [`OrbitCamPreset`] — `Component` selecting a built-in keymap. When inserted on an `OrbitCam`,
//!   the modes system in `crate::input::modes` resolves it through [`OrbitCamPreset::to_bindings`]
//!   and installs the resulting [`super::OrbitCamBindings`].

use bevy::prelude::*;
use bevy_enhanced_input::prelude::ModKeys;

use super::OrbitCamBindings;
use super::builder::OrbitCamMouseDrag;
use super::builder::OrbitCamMouseWheelZoom;
use super::builder::OrbitCamPinchZoom;
use super::builder::OrbitCamTrackpadScroll;
use super::error::OrbitCamBindingsError;

/// Built-in orbit-camera input presets.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
#[non_exhaustive]
pub enum OrbitCamPreset {
    /// Mouse-oriented default controls.
    #[default]
    SimpleMouse,
    /// Editor-oriented controls modeled after Blender navigation.
    BlenderLike,
}

impl OrbitCamPreset {
    /// Converts this preset into validated custom bindings.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] if the preset construction violates the
    /// shared binding validator.
    pub fn to_bindings(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        match self {
            Self::SimpleMouse => OrbitCamBindings::builder()
                .orbit(OrbitCamMouseDrag::new(MouseButton::Left))
                .pan(OrbitCamMouseDrag::new(MouseButton::Right))
                .zoom(OrbitCamMouseWheelZoom::default())
                .zoom(OrbitCamTrackpadScroll::default())
                .zoom(OrbitCamPinchZoom)
                .build(),
            Self::BlenderLike => OrbitCamBindings::builder()
                .orbit(OrbitCamMouseDrag::new(MouseButton::Middle))
                .orbit(OrbitCamTrackpadScroll::default())
                .pan(OrbitCamMouseDrag::new(MouseButton::Middle).with_mod_keys(ModKeys::SHIFT))
                .pan(OrbitCamTrackpadScroll::default().with_mod_keys(ModKeys::SHIFT))
                .zoom(OrbitCamMouseWheelZoom::default())
                .zoom(OrbitCamTrackpadScroll::default().with_mod_keys(ModKeys::CONTROL))
                .zoom(OrbitCamPinchZoom)
                .build(),
        }
    }
}
