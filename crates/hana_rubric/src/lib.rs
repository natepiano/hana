//! JSONC keymap foundations for Bevy applications.
//!
//! `hana_rubric` exposes validated [`CommandId`] values and durable
//! [`KeymapLoadFailures`] data for callers that load and inspect keymaps.

#[cfg(test)]
mod allocation_test_support;
mod command;
mod condition;
mod derived_context;
mod diagnostic;
mod disk;
mod keymap;
mod keymap_plugin;
mod keystroke;
mod platform_shortcut_mode;
/// Convenience re-exports for common keymap callers.
pub mod prelude;

/// Cancels every partially matched multi-keystroke sequence in the active keymap.
///
/// Applications call this after a recovery action or focus change should discard the next
/// keystroke instead of completing an earlier sequence.
pub fn cancel_pending_sequences(world: &mut bevy::prelude::World) {
    keymap::runtime::cancel_pending_sequences(world);
}

#[cfg(test)]
pub(crate) use allocation_test_support::TEST_ALLOCATOR;
pub use command::Capability;
pub use command::CommandId;
pub use command::CommandIdParseError;
pub use command::CommandInfo;
pub use command::CommandRegistry;
pub use command::HoldPhase;
pub use command::Keybindings;
pub use command::KeymapCommand;
pub use command::ReflectKeymapCommand;
pub use condition::ActiveCondition;
pub use condition::ConditionName;
pub use condition::KeymapContext;
pub use derived_context::DerivedContext;
pub use diagnostic::Diagnostic;
pub use diagnostic::DiagnosticKind;
pub use diagnostic::DiagnosticSeverity;
pub use diagnostic::KeymapLoadFailures;
pub use disk::KeymapPaths;
pub use keymap_plugin::KeymapPlugin;
pub use keymap_plugin::KeymapSystems;
pub use keystroke::EmptyKeystrokeSequenceError;
pub use keystroke::Keystroke;
pub use keystroke::KeystrokeParseError;
pub use keystroke::KeystrokeSequence;
pub use keystroke::KeystrokeSequenceParseError;
pub use keystroke::MatchOutcome;
pub use keystroke::Modifiers;
pub use keystroke::SequenceMatcher;
