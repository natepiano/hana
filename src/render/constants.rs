//! Rendering constants for diegetic text.

/// Physical conversion factor: meters per typographic point.
///
/// One point = 1/72 inch. One inch = 0.0254 meters.
/// 72pt text produces a 1-inch (0.0254m) em-square.
/// 12pt text produces a 4.23mm em-square.
///
/// Assumes Bevy convention: 1 world unit = 1 meter.
pub const METERS_PER_POINT: f32 = 0.0254 / 72.0;
