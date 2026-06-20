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
//! - **`input`** (default) — macros and utilities for wiring keyboard actions to commands through
//!   `bevy_enhanced_input`.
//!
//! Disable defaults to pick only what you need:
//!
//! ```toml
//! bevy_kana = { version = "0.0.1", default-features = false, features = ["math"] }
//! ```

#[cfg(feature = "input")]
mod input;
#[cfg(feature = "math")]
mod math;
/// Convenience re-exports for glob imports.
pub mod prelude;

#[cfg(feature = "input")]
pub use input::Keybindings;
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
