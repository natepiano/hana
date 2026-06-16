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

/// Descriptive profile metadata carried by validated camera bindings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamBindingsProfile {
    /// App-authored bindings with no preset profile.
    #[default]
    Custom,
    /// Gamepad preset bindings, possibly tuned by the gamepad preset builder.
    GamepadPreset {
        /// Whether a tuning builder changed the default preset values.
        customized: bool,
    },
    /// Keyboard preset bindings, possibly tuned by a future keyboard builder.
    KeyboardPreset {
        /// Whether a tuning builder changed the default preset values.
        customized: bool,
    },
    /// Bindings made from one or more preset layers.
    LayeredPreset {
        /// Layer set used to build the bindings.
        layers: PresetLayerSet,
    },
}

/// One named preset layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamPresetLayer {
    /// Mouse-oriented pointer layer.
    SimpleMouse,
    /// Blender-like pointer layer.
    BlenderLike,
    /// Keyboard camera-control layer.
    Keyboard,
    /// Gamepad camera-control layer.
    Gamepad,
}

/// Validated set of preset layers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct PresetLayerSet {
    bits: u8,
}

impl PresetLayerSet {
    const BLENDER_LIKE: u8 = 1 << 1;
    const GAMEPAD: u8 = 1 << 3;
    const KEYBOARD: u8 = 1 << 2;
    const SIMPLE_MOUSE: u8 = 1 << 0;

    /// Returns an empty layer set.
    #[must_use]
    pub const fn empty() -> Self { Self { bits: 0 } }

    /// Returns a set containing the `SimpleMouse` layer.
    #[must_use]
    pub const fn simple_mouse() -> Self {
        Self {
            bits: Self::SIMPLE_MOUSE,
        }
    }

    /// Returns a set containing the `BlenderLike` layer.
    #[must_use]
    pub const fn blender_like() -> Self {
        Self {
            bits: Self::BLENDER_LIKE,
        }
    }

    /// Returns `true` when this set includes `layer`.
    #[must_use]
    pub const fn contains(self, layer: OrbitCamPresetLayer) -> bool {
        self.bits & layer_bit(layer) == layer_bit(layer)
    }

    pub(super) const fn with_layer(self, layer: OrbitCamPresetLayer) -> Self {
        Self {
            bits: self.bits | layer_bit(layer),
        }
    }
}

const fn layer_bit(layer: OrbitCamPresetLayer) -> u8 {
    match layer {
        OrbitCamPresetLayer::SimpleMouse => PresetLayerSet::SIMPLE_MOUSE,
        OrbitCamPresetLayer::BlenderLike => PresetLayerSet::BLENDER_LIKE,
        OrbitCamPresetLayer::Keyboard => PresetLayerSet::KEYBOARD,
        OrbitCamPresetLayer::Gamepad => PresetLayerSet::GAMEPAD,
    }
}

/// Builder for composing named preset layers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct OrbitCamPresetLayers {
    layers: PresetLayerSet,
}

impl OrbitCamPresetLayers {
    /// Creates an empty layer builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            layers: PresetLayerSet::empty(),
        }
    }

    /// Adds the `SimpleMouse` layer.
    #[must_use]
    pub const fn with_simple_mouse(mut self) -> Self {
        self.layers = self.layers.with_layer(OrbitCamPresetLayer::SimpleMouse);
        self
    }

    /// Adds the `BlenderLike` layer.
    #[must_use]
    pub const fn with_blender_like(mut self) -> Self {
        self.layers = self.layers.with_layer(OrbitCamPresetLayer::BlenderLike);
        self
    }

    /// Adds the keyboard camera-control layer.
    #[must_use]
    pub const fn with_keyboard(mut self) -> Self {
        self.layers = self.layers.with_layer(OrbitCamPresetLayer::Keyboard);
        self
    }

    /// Adds the gamepad camera-control layer.
    #[must_use]
    pub const fn with_gamepad(mut self) -> Self {
        self.layers = self.layers.with_layer(OrbitCamPresetLayer::Gamepad);
        self
    }

    /// Builds validated bindings from the selected layers.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when any generated descriptor fails
    /// validation.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_with_profile(OrbitCamBindingsProfile::LayeredPreset {
            layers: self.layers,
        })
    }

    pub(super) fn build_with_profile(
        self,
        profile: OrbitCamBindingsProfile,
    ) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        let mut builder = OrbitCamBindings::builder();
        if self.layers.contains(OrbitCamPresetLayer::SimpleMouse) {
            builder = OrbitCamSimpleMousePreset.build_into(builder);
        }
        if self.layers.contains(OrbitCamPresetLayer::BlenderLike) {
            builder = OrbitCamBlenderLikePreset::default().build_into(builder)?;
        }
        if self.layers.contains(OrbitCamPresetLayer::Keyboard) {
            builder = OrbitCamKeyboardPreset.build_into(builder);
        }
        if self.layers.contains(OrbitCamPresetLayer::Gamepad) {
            builder = OrbitCamGamepadPreset::default().build_into(builder)?;
        }
        builder.profile(profile).build()
    }
}
