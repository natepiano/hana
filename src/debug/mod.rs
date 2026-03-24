//! Debug overlays for typography visualization.
//!
//! This module is only compiled when the `typography_overlay` feature is enabled.

mod constants;
mod typography_overlay;

pub use typography_overlay::TypographyOverlay;
pub use typography_overlay::build_typography_overlay;
