//! # `bevy_kana`
//!
//! Ergonomic, opinionated utilities for Bevy — type-safe math, input wiring, and more.
//!
//! `bevy_kana` is a growing collection of ergonomic utilities for Bevy projects.
//! Enable features to pull in what you need.
//!
//! ## Features
//!
//! - **`math`** (default) — zero-cost newtype wrappers around Bevy math primitives that prevent
//!   accidental mixing at compile time.
//! - **`input`** — macros and utilities for wiring keyboard actions to commands through
//!   `bevy_enhanced_input`. Canonical keystroke parsing is always available.
//! - [`Cascade`] — explicit inherited/overridden authoring values plus optional relationship-backed
//!   ECS propagation through [`CascadePlugin`].
//!
//! Disable defaults to pick only what you need:
//!
//! ```toml
//! bevy_kana = { version = "0.0.1", default-features = false, features = ["math"] }
//! ```

#[cfg(test)]
mod allocation_test_support;
mod cascade;
mod input;
#[cfg(feature = "math")]
mod math;
/// Convenience re-exports for glob imports.
pub mod prelude;

#[cfg(test)]
pub(crate) use allocation_test_support::TEST_ALLOCATOR;
pub use cascade::CASCADE_DEPTH_LIMIT;
pub use cascade::Cascade;
pub use cascade::CascadeAttribute;
pub use cascade::CascadeChildren;
pub use cascade::CascadeDefault;
pub use cascade::CascadeEntityCommandsExt;
pub use cascade::CascadeFrom;
pub use cascade::CascadePlugin;
pub use cascade::CascadeRootResource;
pub use cascade::CascadeSet;
pub use cascade::Resolved;
pub use cascade::resolve_cascade;
pub use cascade::resolve_cascade_ref;
pub use cascade::resolve_entity_cascade;
pub use cascade::resolved_cascade;
pub use input::EmptyKeystrokeSequenceError;
#[cfg(feature = "input")]
pub use input::Keybindings;
pub use input::Keystroke;
pub use input::KeystrokeParseError;
pub use input::KeystrokeSequence;
pub use input::KeystrokeSequenceParseError;
pub use input::MatchOutcome;
pub use input::Modifiers;
pub use input::SequenceMatcher;
#[cfg(feature = "math")]
pub use math::Displacement;
#[cfg(feature = "math")]
pub use math::Orientation;
#[cfg(feature = "math")]
pub use math::Position;
#[cfg(feature = "math")]
pub use math::ScreenPosition;
#[cfg(feature = "math")]
pub use math::ToF32;
#[cfg(feature = "math")]
pub use math::ToF64;
#[cfg(feature = "math")]
pub use math::ToI32;
#[cfg(feature = "math")]
pub use math::ToU8;
#[cfg(feature = "math")]
pub use math::ToU16;
#[cfg(feature = "math")]
pub use math::ToU32;
#[cfg(feature = "math")]
pub use math::ToUsize;
#[cfg(feature = "math")]
pub use math::Velocity;
