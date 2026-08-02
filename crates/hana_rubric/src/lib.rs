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
pub fn cancel_pending_sequences(world: &mut World) { keymap::cancel_pending_sequences(world); }

/// Resets physical keymap input after a focus or input-suppression transition.
///
/// This cancels partially matched sequences, releases physical held sources while preserving
/// semantic-event sources, publishes resulting held-value changes, and inhibits every key that
/// remains down until its release. Calling it on the next transition refreshes that inhibition
/// from the keys still down.
pub fn reset_physical_input(world: &mut World) { keymap::reset_physical_input(world); }

#[cfg(test)]
pub(crate) use allocation_test_support::TEST_ALLOCATOR;
use bevy::prelude::World;
pub use command::Capability;
pub use command::CommandId;
pub use command::CommandIdParseError;
pub use command::CommandInfo;
pub use command::CommandRegistry;
pub use command::HeldCommandLookupOutcome;
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
pub use keystroke::InvalidOrdinaryKeyCode;
pub use keystroke::Keystroke;
pub use keystroke::KeystrokeParseError;
pub use keystroke::KeystrokeSequence;
pub use keystroke::KeystrokeSequenceParseError;
pub use keystroke::MatchOutcome;
pub use keystroke::ModifierFamily;
pub use keystroke::Modifiers;
pub use keystroke::OrdinaryKey;
pub use keystroke::PrimaryTrigger;
pub use keystroke::SequenceMatcher;
