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
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
pub(crate) use glyph::DEFAULT_BAND_COUNT;
pub(crate) use render::RenderMode;
pub(crate) use render::RunRenderError;
pub(crate) use render::TextMaterial;
pub(crate) use render::TextMaterialInput;
pub(crate) use render::text_material;
pub(crate) use runtime::GlyphAtlasHandles;
pub(crate) use runtime::GlyphCache;
pub(crate) use runtime::PositionedGlyph;
pub(crate) use runtime::PreparedTextRun;
pub(crate) use runtime::RunStorageKey;

pub(super) struct SlugPlugin;

impl Plugin for SlugPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/slug_text.wgsl");
        app.init_resource::<GlyphCache>();
        app.add_plugins(MaterialPlugin::<TextMaterial>::default());
    }
}
