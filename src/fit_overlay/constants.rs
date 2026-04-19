use bevy::prelude::Color;

// Label constants
/// Color used for the "screen space bounds" label.
pub(super) const BOUNDS_LABEL_COLOR: Color = Color::srgb(1.0, 1.0, 0.0);
/// Font size used for all debug labels.
pub(super) const LABEL_FONT_SIZE: f32 = 11.0;
/// Pixel offset used to keep labels off line endpoints and screen edges.
pub(super) const LABEL_PIXEL_OFFSET: f32 = 8.0;
