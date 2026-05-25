//! Slug analytic Bézier glyph renderer.
//!
//! Renders glyphs from their quadratic Bézier contours with per-pixel
//! analytic coverage, building curve bands synchronously. Owns the slug
//! material, shader, glyph packing, and run/render data; the parent
//! [`text`](crate::text) module supplies the font infrastructure it consumes.

mod backend;
mod constants;
mod geometry;
mod material;
mod packing;
mod run;
mod run_render;
#[cfg(test)]
mod test_support;

pub(crate) use backend::SlugBackend;
pub(crate) use backend::SlugPreparedTextRun;
pub(crate) use backend::SlugRunStorage;
pub(crate) use backend::SlugRunStorageKey;
pub(crate) use material::SlugRenderMode;
pub(crate) use material::SlugTextMaterial;
pub(crate) use material::SlugTextMaterialInput;
pub(crate) use material::slug_text_material;
pub(crate) use packing::DEFAULT_BAND_COUNT;
