//! JSONC keymap foundations for Bevy applications.
//!
//! `hana_rubric` exposes validated [`CommandId`] values and durable
//! [`KeymapLoadFailures`] data for callers that load and inspect keymaps.

mod command;
mod condition;
mod diagnostic;
mod disk;
mod keymap;
/// Convenience re-exports for common keymap callers.
pub mod prelude;

#[doc(hidden)]
pub use bevy_kana::action;
#[doc(hidden)]
pub use bevy_kana::event;
pub use command::Capability;
pub use command::CommandId;
pub use command::CommandIdParseError;
pub use command::KeymapCommand;
pub use command::ReflectKeymapCommand;
pub use diagnostic::Diagnostic;
pub use diagnostic::DiagnosticKind;
pub use diagnostic::KeymapLoadFailures;
