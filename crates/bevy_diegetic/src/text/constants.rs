//! Constants shared across text loading, atlas packing, and MSDF rasterization.

// Atlas configuration tuning
/// Average glyph coverage ratio — most glyphs use roughly this fraction of
/// the canonical size.
pub(super) const AVERAGE_GLYPH_COVERAGE: f32 = 0.75;
/// Default auto-selected glyph raster worker count on sufficiently parallel
/// machines. Intentionally distinct from `DEFAULT_GLYPH_WORKER_THREADS`
/// (the unconditional override default); same value today, but they answer
/// different questions and may diverge.
pub(super) const DEFAULT_AUTO_GLYPH_WORKER_THREADS: usize = 6;
/// Default glyphs per atlas page.
pub(super) const DEFAULT_GLYPHS_PER_PAGE: u16 = 100;
/// Maximum canonical rasterization size in pixels.
pub(super) const MAX_CUSTOM_RASTER_SIZE: u32 = 256;
/// Maximum glyphs per atlas page.
pub(super) const MAX_GLYPHS_PER_PAGE: u16 = 2000;
/// Minimum canonical rasterization size in pixels.
pub(super) const MIN_CUSTOM_RASTER_SIZE: u32 = 8;
/// Minimum glyphs per atlas page.
pub(super) const MIN_GLYPHS_PER_PAGE: u16 = 10;
/// SDF distance range used during MSDF rasterization.
pub(super) const SDF_RANGE: u32 = 4;
/// Estimated packing efficiency for a shelf-based atlas allocator.
pub(super) const SHELF_PACKING_EFFICIENCY: f32 = 0.80;

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
/// Default number of worker threads used by the atlas when no override is
/// provided. Intentionally distinct from `DEFAULT_AUTO_GLYPH_WORKER_THREADS`
/// (the cap used by `Auto` worker selection); same value today, but they
/// answer different questions and may diverge.
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
