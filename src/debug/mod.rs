//! Debug overlays for typography visualization.
//!
//! This module is only compiled when the `typography_overlay` feature is enabled.

mod typography_overlay;

pub use typography_overlay::TypographyOverlay;
pub use typography_overlay::TypographyOverlayGizmoGroup;
pub use typography_overlay::render_typography_overlay;
pub use typography_overlay::update_typography_gizmo_config;
