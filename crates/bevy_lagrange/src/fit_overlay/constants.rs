use bevy::prelude::Color;

// Label constants
/// Color used for the "screen space bounds" label.
pub(super) const BOUNDS_LABEL_COLOR: Color = Color::srgb(1.0, 1.0, 0.0);
/// Font size used for all debug labels.
pub(super) const LABEL_FONT_SIZE: f32 = 11.0;
/// Pixel offset used to keep labels off line endpoints and screen edges.
pub(super) const LABEL_PIXEL_OFFSET: f32 = 8.0;

// Overlay constants
/// Default line width for the fit-target overlay gizmo.
pub(super) const DEFAULT_OVERLAY_LINE_WIDTH: f32 = 2.0;
/// Depth bias applied to the fit-target gizmo so it draws on top of scene geometry.
pub(super) const OVERLAY_GIZMO_DEPTH_BIAS: f32 = -1.0;
/// Multiplier converting a fraction (0.0–1.0) into a percentage (0–100).
pub(super) const PERCENT_MULTIPLIER: f32 = 100.0;
