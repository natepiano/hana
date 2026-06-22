//! Built-in orbit-camera input presets that compile down to validated
//! [`super::OrbitCamBindings`].

mod blender_like;
mod blender_like_keyboard;
mod config;
mod enum_preset;
mod gamepad;
mod keyboard;
mod simple_mouse;
mod simple_mouse_keyboard;
mod source_sensitivity;

pub use blender_like::OrbitCamBlenderLikePreset;
#[cfg(feature = "reflect-input-modes")]
pub use blender_like::OrbitCamBlenderLikePresetDraft;
pub use blender_like_keyboard::OrbitCamBlenderLikeKeyboardPreset;
#[cfg(feature = "reflect-input-modes")]
pub use blender_like_keyboard::OrbitCamBlenderLikeKeyboardPresetDraft;
pub use enum_preset::OrbitCamPreset;
#[cfg(feature = "reflect-input-modes")]
pub use enum_preset::OrbitCamPresetDraft;
pub use enum_preset::OrbitCamPresetKind;
#[cfg(feature = "reflect-input-modes")]
pub use enum_preset::OrbitCamSensitivityDraft;
pub use gamepad::OrbitCamGamepadPreset;
pub use gamepad::OrbitCamGamepadPresetBuilder;
#[cfg(feature = "reflect-input-modes")]
pub use gamepad::OrbitCamGamepadPresetDraft;
pub use keyboard::OrbitCamKeyboardPreset;
#[cfg(feature = "reflect-input-modes")]
pub use keyboard::OrbitCamKeyboardPresetDraft;
pub use simple_mouse::OrbitCamSimpleMousePreset;
#[cfg(feature = "reflect-input-modes")]
pub use simple_mouse::OrbitCamSimpleMousePresetDraft;
pub use simple_mouse_keyboard::OrbitCamSimpleMouseKeyboardPreset;
#[cfg(feature = "reflect-input-modes")]
pub use simple_mouse_keyboard::OrbitCamSimpleMouseKeyboardPresetDraft;
pub use source_sensitivity::GamepadSensitivity;
pub use source_sensitivity::MouseSensitivity;
pub use source_sensitivity::SmoothScrollSensitivity;
