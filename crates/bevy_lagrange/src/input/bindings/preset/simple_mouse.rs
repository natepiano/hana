use bevy::prelude::*;

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

/// Configures the default mouse-oriented orbit-camera preset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct OrbitCamSimpleMousePreset;

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

    pub(super) fn build_into(self, builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
        let Self = self;
        Self::add_to(builder)
    }

    fn add_to(builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
        builder
            .orbit(OrbitCamMouseDrag::new(MouseButton::Left))
            .pan(OrbitCamMouseDrag::new(MouseButton::Right))
            .zoom(OrbitCamMouseWheelZoom)
            .zoom(OrbitCamTrackpadScroll::default())
            .zoom(OrbitCamPinchZoom)
    }
}

impl OrbitCamPresetConfig for OrbitCamSimpleMousePreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_into(OrbitCamBindings::builder())
            .profile(OrbitCamBindingsProfile::LayeredPreset {
                layers: PresetLayerSet::simple_mouse(),
            })
            .build()
    }
}
