//! Slug analytic Bézier glyph renderer.
//!
//! Renders glyphs from their quadratic Bézier contours with per-pixel
//! analytic coverage. This module owns text-specific shaping, glyph lookup,
//! and outline extraction; `render` owns the shared renderer, material, shader,
//! packing, and batch store.

mod glyph;
mod render;
mod runtime;
#[cfg(test)]
mod support;

use bevy::prelude::*;
pub(crate) use render::glyph_quad_extents;
pub(crate) use runtime::GlyphCache;
pub(crate) use runtime::PositionedGlyph;
pub(crate) use runtime::PreparedTextRun;
pub(crate) use runtime::RunStorageKey;

pub(super) struct SlugPlugin;

impl Plugin for SlugPlugin {
    fn build(&self, app: &mut App) { app.init_resource::<GlyphCache>(); }
}
