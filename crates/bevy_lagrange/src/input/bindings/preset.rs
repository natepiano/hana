//! Built-in orbit-camera input presets that compile down to validated
//! [`super::OrbitCamBindings`].
//!
//! Types:
//! - [`OrbitCamPreset`] — selects a built-in keymap for [`crate::input::OrbitCamInputMode`].
//! - [`OrbitCamPresetLayers`] / [`PresetLayerSet`] — layer builder used by composed presets.
//! - [`OrbitCamGamepadPreset`] — typed gamepad preset tuner that preserves profile metadata.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::ModKeys;

use super::OrbitCamBindings;
use super::builder::CameraInputGamepadSelectionPolicy;
use super::builder::OrbitCamBindingsBuilder;
use super::builder::OrbitCamMouseDrag;
use super::builder::OrbitCamMouseWheelZoom;
use super::builder::OrbitCamPinchZoom;
use super::builder::OrbitCamTrackpadScroll;
use super::error::OrbitCamBindingsError;
use super::held_binding::OrbitCamHeldBinding;
use super::held_binding::OrbitCamInputBinding;
use crate::input::ControlSpeed;
use crate::input::InputDeadZone;

const GAMEPAD_ORBIT_SCALE: f32 = 1200.0;
const GAMEPAD_SLOW_ORBIT_SCALE: f32 = 120.0;
const GAMEPAD_PAN_SCALE: f32 = 800.0;
const GAMEPAD_SLOW_PAN_SCALE: f32 = 80.0;
const GAMEPAD_ZOOM_SCALE: f32 = 7.0;
const GAMEPAD_SLOW_ZOOM_SCALE: f32 = 0.6;
const GAMEPAD_STICK_DEAD_ZONE_LOWER: f32 = 0.18;
const GAMEPAD_STICK_DEAD_ZONE_UPPER: f32 = 1.0;

const PRESET_LAYER_SIMPLE_MOUSE: u8 = 1 << 0;
const PRESET_LAYER_BLENDER_LIKE: u8 = 1 << 1;
const PRESET_LAYER_KEYBOARD: u8 = 1 << 2;
const PRESET_LAYER_GAMEPAD: u8 = 1 << 3;

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
    /// Returns [`OrbitCamBindingsError`] if the preset construction violates the
    /// shared binding validator.
    pub fn to_bindings(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        match self {
            Self::SimpleMouse => OrbitCamPresetLayers::new()
                .with_simple_mouse()
                .build_with_profile(OrbitCamBindingsProfile::LayeredPreset {
                    layers: PresetLayerSet::simple_mouse(),
                }),
            Self::BlenderLike => OrbitCamPresetLayers::new()
                .with_blender_like()
                .build_with_profile(OrbitCamBindingsProfile::LayeredPreset {
                    layers: PresetLayerSet::blender_like(),
                }),
            Self::Keyboard => OrbitCamPresetLayers::new()
                .with_keyboard()
                .build_with_profile(OrbitCamBindingsProfile::KeyboardPreset { customized: false }),
            Self::SimpleMouseKeyboard => OrbitCamPresetLayers::new()
                .with_simple_mouse()
                .with_keyboard()
                .build(),
            Self::BlenderLikeKeyboard => OrbitCamPresetLayers::new()
                .with_blender_like()
                .with_keyboard()
                .build(),
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
    /// Returns an empty layer set.
    #[must_use]
    pub const fn empty() -> Self { Self { bits: 0 } }

    /// Returns a set containing the `SimpleMouse` layer.
    #[must_use]
    pub const fn simple_mouse() -> Self {
        Self {
            bits: PRESET_LAYER_SIMPLE_MOUSE,
        }
    }

    /// Returns a set containing the `BlenderLike` layer.
    #[must_use]
    pub const fn blender_like() -> Self {
        Self {
            bits: PRESET_LAYER_BLENDER_LIKE,
        }
    }

    /// Returns `true` when this set includes `layer`.
    #[must_use]
    pub const fn contains(self, layer: OrbitCamPresetLayer) -> bool {
        self.bits & layer_bit(layer) == layer_bit(layer)
    }

    const fn with_layer(self, layer: OrbitCamPresetLayer) -> Self {
        Self {
            bits: self.bits | layer_bit(layer),
        }
    }
}

const fn layer_bit(layer: OrbitCamPresetLayer) -> u8 {
    match layer {
        OrbitCamPresetLayer::SimpleMouse => PRESET_LAYER_SIMPLE_MOUSE,
        OrbitCamPresetLayer::BlenderLike => PRESET_LAYER_BLENDER_LIKE,
        OrbitCamPresetLayer::Keyboard => PRESET_LAYER_KEYBOARD,
        OrbitCamPresetLayer::Gamepad => PRESET_LAYER_GAMEPAD,
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
    /// Returns [`OrbitCamBindingsError`] when any generated descriptor fails validation.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_with_profile(OrbitCamBindingsProfile::LayeredPreset {
            layers: self.layers,
        })
    }

    pub(super) fn build_with_profile(
        self,
        profile: OrbitCamBindingsProfile,
    ) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        let mut builder = OrbitCamBindings::builder().profile(profile);
        if self.layers.contains(OrbitCamPresetLayer::SimpleMouse) {
            builder = add_simple_mouse_layer(builder);
        }
        if self.layers.contains(OrbitCamPresetLayer::BlenderLike) {
            builder = add_blender_like_layer(builder);
        }
        if self.layers.contains(OrbitCamPresetLayer::Keyboard) {
            builder = add_keyboard_layer(builder);
        }
        if self.layers.contains(OrbitCamPresetLayer::Gamepad) {
            builder = add_gamepad_layer(builder, OrbitCamGamepadPreset::default(), false);
        }
        builder.build()
    }
}

/// Tunable gamepad preset descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct OrbitCamGamepadPreset {
    orbit_scale:      f32,
    slow_orbit_scale: f32,
    pan_scale:        f32,
    slow_pan_scale:   f32,
    zoom_scale:       f32,
    slow_zoom_scale:  f32,
    stick_dead_zone:  InputDeadZone,
}

impl Default for OrbitCamGamepadPreset {
    fn default() -> Self {
        Self {
            orbit_scale:      GAMEPAD_ORBIT_SCALE,
            slow_orbit_scale: GAMEPAD_SLOW_ORBIT_SCALE,
            pan_scale:        GAMEPAD_PAN_SCALE,
            slow_pan_scale:   GAMEPAD_SLOW_PAN_SCALE,
            zoom_scale:       GAMEPAD_ZOOM_SCALE,
            slow_zoom_scale:  GAMEPAD_SLOW_ZOOM_SCALE,
            stick_dead_zone:  InputDeadZone::new(
                GAMEPAD_STICK_DEAD_ZONE_LOWER,
                GAMEPAD_STICK_DEAD_ZONE_UPPER,
            ),
        }
    }
}

impl OrbitCamGamepadPreset {
    /// Starts a tuning builder from this preset.
    #[must_use]
    pub const fn customize(self) -> OrbitCamGamepadPresetBuilder {
        OrbitCamGamepadPresetBuilder {
            preset:     self,
            customized: false,
        }
    }

    /// Builds the zero-config gamepad preset.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when generated descriptors fail validation.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        add_gamepad_layer(
            OrbitCamBindings::builder()
                .profile(OrbitCamBindingsProfile::GamepadPreset { customized: false }),
            self,
            false,
        )
        .build()
    }
}

/// Fluent tuning builder for [`OrbitCamGamepadPreset`].
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct OrbitCamGamepadPresetBuilder {
    preset:     OrbitCamGamepadPreset,
    customized: bool,
}

impl OrbitCamGamepadPresetBuilder {
    /// Sets the fast orbit scale.
    #[must_use]
    pub const fn orbit_scale(mut self, orbit_scale: f32) -> Self {
        self.preset.orbit_scale = orbit_scale;
        self.customized = true;
        self
    }

    /// Sets the slow orbit scale.
    #[must_use]
    pub const fn slow_orbit_scale(mut self, slow_orbit_scale: f32) -> Self {
        self.preset.slow_orbit_scale = slow_orbit_scale;
        self.customized = true;
        self
    }

    /// Sets the fast pan scale.
    #[must_use]
    pub const fn pan_scale(mut self, pan_scale: f32) -> Self {
        self.preset.pan_scale = pan_scale;
        self.customized = true;
        self
    }

    /// Sets the slow pan scale.
    #[must_use]
    pub const fn slow_pan_scale(mut self, slow_pan_scale: f32) -> Self {
        self.preset.slow_pan_scale = slow_pan_scale;
        self.customized = true;
        self
    }

    /// Sets the fast zoom scale.
    #[must_use]
    pub const fn zoom_scale(mut self, zoom_scale: f32) -> Self {
        self.preset.zoom_scale = zoom_scale;
        self.customized = true;
        self
    }

    /// Sets the slow zoom scale.
    #[must_use]
    pub const fn slow_zoom_scale(mut self, slow_zoom_scale: f32) -> Self {
        self.preset.slow_zoom_scale = slow_zoom_scale;
        self.customized = true;
        self
    }

    /// Sets the axial stick dead-zone thresholds.
    #[must_use]
    pub const fn stick_dead_zone(mut self, stick_dead_zone: InputDeadZone) -> Self {
        self.preset.stick_dead_zone = stick_dead_zone;
        self.customized = true;
        self
    }

    /// Builds tuned gamepad bindings.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when generated descriptors fail validation.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        add_gamepad_layer(
            OrbitCamBindings::builder().profile(OrbitCamBindingsProfile::GamepadPreset {
                customized: self.customized,
            }),
            self.preset,
            self.customized,
        )
        .build()
    }
}

fn add_simple_mouse_layer(builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
    builder
        .orbit(OrbitCamMouseDrag::new(MouseButton::Left))
        .pan(OrbitCamMouseDrag::new(MouseButton::Right))
        .zoom(OrbitCamMouseWheelZoom)
        .zoom(OrbitCamTrackpadScroll::default())
        .zoom(OrbitCamPinchZoom)
}

fn add_blender_like_layer(builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
    builder
        .orbit(OrbitCamMouseDrag::new(MouseButton::Middle))
        .orbit(OrbitCamTrackpadScroll::default())
        .pan(OrbitCamMouseDrag::new(MouseButton::Middle).with_mod_keys(ModKeys::SHIFT))
        .pan(OrbitCamTrackpadScroll::default().with_mod_keys(ModKeys::SHIFT))
        .zoom(OrbitCamMouseWheelZoom)
        .zoom(OrbitCamTrackpadScroll::default().with_mod_keys(ModKeys::CONTROL))
        .zoom(OrbitCamPinchZoom)
}

fn add_keyboard_layer(builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
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

fn add_gamepad_layer(
    builder: OrbitCamBindingsBuilder,
    preset: OrbitCamGamepadPreset,
    _customized: bool,
) -> OrbitCamBindingsBuilder {
    let fast_orbit = gamepad_stick(
        GamepadAxis::RightStickX,
        GamepadAxis::RightStickY,
        preset.orbit_scale,
        preset.stick_dead_zone,
    );
    let slow_orbit = gamepad_stick(
        GamepadAxis::RightStickX,
        GamepadAxis::RightStickY,
        preset.slow_orbit_scale,
        preset.stick_dead_zone,
    );
    let fast_pan = gamepad_stick(
        GamepadAxis::LeftStickX,
        GamepadAxis::LeftStickY,
        preset.pan_scale,
        preset.stick_dead_zone,
    );
    let slow_pan = gamepad_stick(
        GamepadAxis::LeftStickX,
        GamepadAxis::LeftStickY,
        preset.slow_pan_scale,
        preset.stick_dead_zone,
    );

    builder
        .orbit(OrbitCamHeldBinding::same(fast_orbit).with_blocked_gate(GamepadButton::RightTrigger))
        .orbit(
            OrbitCamHeldBinding::same(slow_orbit)
                .with_required_gate(GamepadButton::RightTrigger)
                .speed(ControlSpeed::Slow),
        )
        .pan(OrbitCamHeldBinding::same(fast_pan).with_blocked_gate(GamepadButton::LeftTrigger))
        .pan(
            OrbitCamHeldBinding::same(slow_pan)
                .with_required_gate(GamepadButton::LeftTrigger)
                .speed(ControlSpeed::Slow),
        )
        .zoom(
            OrbitCamHeldBinding::same(gamepad_trigger(
                GamepadButton::RightTrigger2,
                preset.zoom_scale,
            ))
            .with_blocked_gate(GamepadButton::RightTrigger),
        )
        .zoom(
            OrbitCamHeldBinding::same(gamepad_trigger(
                GamepadButton::LeftTrigger2,
                -preset.zoom_scale,
            ))
            .with_blocked_gate(GamepadButton::LeftTrigger),
        )
        .zoom(
            OrbitCamHeldBinding::same(gamepad_trigger(
                GamepadButton::RightTrigger2,
                preset.slow_zoom_scale,
            ))
            .with_required_gate(GamepadButton::RightTrigger)
            .speed(ControlSpeed::Slow),
        )
        .zoom(
            OrbitCamHeldBinding::same(gamepad_trigger(
                GamepadButton::LeftTrigger2,
                -preset.slow_zoom_scale,
            ))
            .with_required_gate(GamepadButton::LeftTrigger)
            .speed(ControlSpeed::Slow),
        )
        .gamepad(CameraInputGamepadSelectionPolicy::Active)
}

fn gamepad_stick(
    x_axis: GamepadAxis,
    y_axis: GamepadAxis,
    scale: f32,
    dead_zone: InputDeadZone,
) -> OrbitCamInputBinding {
    OrbitCamInputBinding::gamepad_axes_2d(x_axis, y_axis)
        .with_dead_zone(dead_zone)
        .with_scale(Vec2::splat(scale))
        .with_delta_scale()
}

fn gamepad_trigger(button: GamepadButton, scale: f32) -> OrbitCamInputBinding {
    OrbitCamInputBinding::gamepad_button_axis(button, scale).with_delta_scale()
}
