//! Keystroke parsing plus input action utilities for `bevy_enhanced_input`.
//!
//! Provides macros to reduce boilerplate when wiring keyboard actions to
//! commands through intermediate events, a [`Keybindings`] builder that handles platform-specific
//! modifier keys (`Cmd` vs `Ctrl`) and automatic `BlockBy` application, and canonical
//! [`Keystroke`] values that do not require the `input` feature.

#[cfg(feature = "input")]
mod action;
#[cfg(feature = "input")]
mod bind_action_system;
#[cfg(feature = "input")]
mod event;
#[cfg(feature = "input")]
mod keybindings;
mod keystroke;
mod platform_shortcut_mode;

#[cfg(feature = "input")]
pub use keybindings::Keybindings;
pub use keystroke::EmptyKeystrokeSequenceError;
pub use keystroke::Keystroke;
pub use keystroke::KeystrokeParseError;
pub use keystroke::KeystrokeSequence;
pub use keystroke::KeystrokeSequenceParseError;
pub use keystroke::MatchOutcome;
pub use keystroke::Modifiers;
pub use keystroke::SequenceMatcher;
