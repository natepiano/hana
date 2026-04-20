//! Constants shared across text loading, atlas packing, and MSDF rasterization.

// Atlas packing
/// Texel gutter around each glyph in the atlas.
///
/// Prevents linear filtering from sampling into adjacent glyph regions,
/// which causes the MSDF median-of-three decode to produce faint vertical
/// line artifacts at glyph boundaries. Border texels are replicated into
/// the gutter so the distance field is continuous at the edge, and UV
/// coordinates are inset by half a texel so the sampler hits texel centers.
pub(super) const ATLAS_GUTTER: u32 = 1;
/// Number of bytes per pixel (RGBA).
pub(super) const BYTES_PER_PIXEL: u32 = 4;
/// Default atlas page texture size in pixels.
pub(super) const DEFAULT_ATLAS_SIZE: u32 = 1024;
/// Default number of worker threads used by the atlas when no override is provided.
pub(super) const DEFAULT_GLYPH_WORKER_THREADS: usize = 6;

// Font defaults
/// Default font family name.
pub(super) const DEFAULT_FAMILY: &str = "JetBrains Mono";
/// Embedded `JetBrains Mono` Regular font binary (SIL Open Font License).
pub const EMBEDDED_FONT: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");

// MSDF rasterization
/// Default canonical pixel size for MSDF generation.
///
/// MSDF is resolution-independent, so all glyphs are generated at this
/// single size. The shader handles scaling.
pub(super) const DEFAULT_CANONICAL_SIZE: u32 = 64;
/// Default padding around each glyph in pixels.
pub(super) const DEFAULT_GLYPH_PADDING: u32 = 2;
/// Default SDF range in pixels.
///
/// Higher values = smoother edges at extreme zoom but less precision.
pub(super) const DEFAULT_SDF_RANGE: f64 = 4.0;
/// Angle threshold for edge coloring (3 degrees, as recommended by Chlumsky).
pub(super) const EDGE_COLORING_ANGLE: f64 = 3.0;
/// Seed for deterministic edge coloring.
pub(super) const EDGE_COLORING_SEED: u64 = 0;
