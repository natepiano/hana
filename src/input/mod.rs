//! Input action macros and keybinding utilities for `bevy_enhanced_input`.
//!
//! Provides macros to reduce boilerplate when wiring keyboard actions to
//! commands through intermediate events, and a [`Keybindings`] builder
//! that handles platform-specific modifier keys (`Cmd` vs `Ctrl`) and
//! automatic `BlockBy` application.

mod action_macro;
mod bind_action_system_macro;
mod event_macro;
mod keybindings;

pub use keybindings::Keybindings;
