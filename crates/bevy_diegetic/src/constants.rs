//! Crate-wide constants shared across multiple modules.

// shader assets
pub(crate) const EMBEDDED_GLYPH_TEXT_SHADER_PATH: &str =
    "embedded://bevy_diegetic/shaders/glyph_text.wgsl";
pub(crate) const EMBEDDED_SDF_PANEL_SHADER_PATH: &str =
    "embedded://bevy_diegetic/shaders/sdf_panel.wgsl";

// text measurement
/// Estimated character width as a fraction of font size for monospace approximation.
pub(crate) const MONOSPACE_WIDTH_RATIO: f32 = 0.6;

// timing
/// Conversion factor from seconds to milliseconds for timing diagnostics.
pub(crate) const MILLISECONDS_PER_SECOND: f32 = 1000.0;
