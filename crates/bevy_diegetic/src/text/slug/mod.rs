//! Slug analytic Bézier glyph renderer.
//!
//! Renders glyphs from their quadratic Bézier contours with per-pixel
//! analytic coverage, building curve bands synchronously. Owns the slug
//! material, shader, glyph packing, and run/render data; the parent
//! [`text`](crate::text) module supplies the font infrastructure it consumes.

mod glyph;
mod render;
mod runtime;
#[cfg(test)]
mod support;

use bevy::asset::embedded_asset;
use bevy::asset::load_internal_asset;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
pub(crate) use glyph::DEFAULT_BAND_COUNT;
pub(crate) use glyph::GlyphInstanceRecord;
pub(crate) use glyph::RunRecord;
pub(crate) use render::BatchTextMaterialInput;
pub(crate) use render::RenderMode;
use render::SLUG_TEXT_VERTEX_PULL_SHADER_HANDLE;
pub(crate) use render::TextExtension;
pub(crate) use render::TextExtensionKey;
pub(crate) use render::TextMaterial;
pub(crate) use render::batch_text_material;
pub(crate) use render::glyph_quad_extents;
pub(crate) use render::set_batch_text_material_buffers;
pub(crate) use render::set_text_material_anti_alias;
#[cfg(feature = "batch_proof")]
pub(crate) use render::toggle_text_material_debug_glyph_index;
pub(crate) use runtime::BatchGpu;
pub(crate) use runtime::BatchKey;
pub(crate) use runtime::BatchRenderLayers;
pub(crate) use runtime::GlyphAtlasHandles;
pub(crate) use runtime::GlyphCache;
pub(crate) use runtime::PositionedGlyph;
pub(crate) use runtime::PreparedTextRun;
pub(crate) use runtime::RunStorageKey;

pub(super) struct SlugPlugin;

impl Plugin for SlugPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/slug_text.wgsl");
        load_internal_asset!(
            app,
            SLUG_TEXT_VERTEX_PULL_SHADER_HANDLE,
            "shaders/slug_text_vertex_pull.wgsl",
            Shader::from_wgsl
        );
        app.init_resource::<GlyphCache>();
        app.add_plugins(MaterialPlugin::<TextMaterial>::default());
    }
}
