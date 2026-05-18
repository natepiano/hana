//! Shared bitmap-size computation for the CPU and GPU rasterizer paths.
//!
//! Both `fdsm`-based CPU rasterization (`msdf_rasterizer`) and GPU
//! compute dispatch (`gpu_rasterizer::edges`) must agree on the bitmap
//! dimensions allocated for each glyph: the GPU shader writes
//! `bitmap.width * bitmap.height` texels into the atlas, and the
//! atlas's shelf allocator (CPU side) reserves that exact region. Any
//! drift produces either out-of-bounds writes on the GPU or wasted
//! pixels in the atlas.
//!
//! The formula matches `rasterize_msdf_bitmap` and `SdfRasterizer`
//! exactly: each side gets `2 * (padding + sdf_range)` of room around
//! the glyph's em-space bounding box, scaled to px.

use bevy_kana::ToU32;
use ttf_parser::Face;
use ttf_parser::GlyphId;

/// Per-glyph bitmap dimensions in texels and per-side padding in px.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BitmapDims {
    /// Bitmap width in texels.
    pub width:  u32,
    /// Bitmap height in texels.
    pub height: u32,
}

/// Computes the bitmap dimensions for a single glyph.
///
/// Non-const: the formula uses `f64::ceil`, which is not stable in
/// `const` context. Returns `None` if the glyph has no bounding box
/// (e.g., space) or if the computed dimensions are zero.
#[must_use]
pub(crate) fn compute_bitmap_size(
    face: &Face<'_>,
    glyph_id: GlyphId,
    px_size: u32,
    sdf_range: f64,
    padding: u32,
) -> Option<BitmapDims> {
    let bbox = face.glyph_bounding_box(glyph_id)?;
    let units_per_em = f64::from(face.units_per_em());
    let scale = f64::from(px_size) / units_per_em;
    let total_pad = f64::from(padding) + sdf_range;
    let glyph_width = f64::from(bbox.x_max - bbox.x_min) * scale;
    let glyph_height = f64::from(bbox.y_max - bbox.y_min) * scale;
    let width = total_pad.mul_add(2.0, glyph_width).ceil().to_u32();
    let height = total_pad.mul_add(2.0, glyph_height).ceil().to_u32();
    if width == 0 || height == 0 {
        return None;
    }
    Some(BitmapDims { width, height })
}
