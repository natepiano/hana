//! Isolated Slug feasibility helpers.
//!
//! This module is intentionally feature-gated and hidden from normal
//! documentation. It exists to prove the Slug data path before any
//! production text renderer refactor.

mod backend;
mod constants;
mod fixtures;
mod geometry;
mod material;
mod packing;
mod run;
mod run_render;

pub use backend::SlugBackend;
pub use backend::SlugBackendCompleted;
pub use backend::SlugPreparedTextRun;
pub use backend::SlugRunStorage;
pub use backend::SlugRunStorageKey;
pub use backend::SlugTextRequest;
pub use fixtures::FIXTURE_TEXT;
pub use geometry::SlugBounds;
pub use geometry::SlugGlyph;
pub use geometry::SlugOutlineError;
pub use geometry::load_glyph;
pub use geometry::load_glyph_by_id_from_face;
pub use material::SlugRenderMode;
pub use material::SlugTextMaterial;
pub use material::SlugTextMaterialInput;
pub use material::SlugTextSpikePlugin;
pub use material::slug_text_material;
pub use packing::DEFAULT_BAND_COUNT;
pub use packing::SlugBandRecord;
pub use packing::SlugCurveRecord;
pub use packing::SlugGlyphRecord;
pub use packing::SlugPackedGlyph;
pub use packing::build_packed_glyph;
pub use run::SlugBuiltTextRun;
pub use run::SlugFontKey;
pub use run::SlugGlyphCache;
pub use run::SlugGlyphInstance;
pub use run::SlugGlyphKey;
pub use run::SlugTextRun;
pub use run::build_slug_text_run;
pub use run_render::SlugRunRenderData;
pub use run_render::SlugRunRenderError;
pub use run_render::SlugRunStorageProfile;
pub use run_render::build_slug_run_render_data;
