//! Isolated Slug feasibility helpers.
//!
//! This module is intentionally feature-gated and hidden from normal
//! documentation. It exists to prove the Slug data path before any
//! production text renderer refactor.

mod constants;
mod fixtures;
mod geometry;
mod material;
mod mesh;
mod packing;
mod run;

pub use fixtures::FIXTURE_TEXT;
pub use fixtures::load_fixture_glyphs;
pub use geometry::QuadraticSegment;
pub use geometry::SlugBounds;
pub use geometry::SlugContour;
pub use geometry::SlugGlyph;
pub use geometry::SlugOutlineError;
pub use geometry::load_glyph;
pub use geometry::load_glyph_by_id;
pub use material::SlugTextMaterial;
pub use material::SlugTextMaterialInput;
pub use material::SlugTextSpikePlugin;
pub use material::slug_text_material;
pub use mesh::build_outline_mesh;
pub use packing::DEFAULT_BAND_COUNT;
pub use packing::SlugBandRecord;
pub use packing::SlugCurveRecord;
pub use packing::SlugPackedGlyph;
pub use packing::build_packed_glyph;
pub use run::SlugFontKey;
pub use run::SlugGlyphCache;
pub use run::SlugGlyphInstance;
pub use run::SlugGlyphKey;
pub use run::SlugTextRun;
