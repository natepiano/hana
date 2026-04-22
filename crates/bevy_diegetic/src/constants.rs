//! Crate-wide constants shared across multiple modules.

/// Conversion factor from seconds to milliseconds for timing diagnostics.
pub(crate) const MILLISECONDS_PER_SECOND: f32 = 1000.0;

/// Estimated character width as a fraction of font size for monospace approximation.
pub(crate) const MONOSPACE_WIDTH_RATIO: f32 = 0.6;
