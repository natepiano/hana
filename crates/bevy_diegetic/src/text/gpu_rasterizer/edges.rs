//! CPU-side edge buffer construction for the GPU rasterizer.
//!
//! Walks the fdsm contour list for a glyph and emits a flat
//! `Vec<EdgeSegment>` that the WGSL kernel reads from a storage
//! buffer. Linear, quadratic, and cubic bezier segments are all
//! handled — each one occupies one `EdgeSegment` slot with up to four
//! control points packed alongside a `kind` discriminant.

use bevy::math::UVec2;
use bevy_kana::ToF32;
use bytemuck::Pod;
use bytemuck::Zeroable;
use fdsm::bezier::Order;
use fdsm::bezier::Segment as FdsmSegment;
use fdsm::transform::Transform;
use nalgebra::Affine2;
use nalgebra::Matrix3;
use ttf_parser::Face;
use ttf_parser::GlyphId;

use crate::text::bitmap_dims;

/// Discriminant bits in `EdgeSegment::kind` for the segment order.
///
/// Bits 0–1 of `kind` encode the segment order, leaving the upper bits
/// reserved for the MSDF channel mask (Phase 2).
/// Linear segment discriminant — matches `EDGE_KIND_LINEAR` in
/// `shaders/sdf_gen.wgsl`. Bits 0–1 of `EdgeSegment::kind`.
pub(super) const EDGE_KIND_LINEAR: u32 = 0;
/// Quadratic bezier discriminant. Phase 1 SDF kernel handles all three
/// orders; the constants stay together so future MSDF code can validate
/// against the same numeric mapping.
pub(super) const EDGE_KIND_QUADRATIC: u32 = 1;
/// Cubic bezier discriminant.
pub(super) const EDGE_KIND_CUBIC: u32 = 2;

/// Per-edge record sent to the GPU storage buffer.
///
/// 9 × 4 = 36 bytes per record. Holds up to four control points (P0–P3,
/// stored interleaved as `[x0, y0, x1, y1, x2, y2, x3, y3]`) plus a
/// `kind` field. Bits 0–1 of `kind` are the segment order; bits 2–4
/// reserve room for the Phase-2 MSDF channel mask.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct EdgeSegment {
    pub points: [f32; 8],
    pub kind:   u32,
}

/// Output of [`build_edge_buffer`].
#[derive(Clone, Debug)]
pub struct GpuGlyphRequestBody {
    /// All edges from every contour of the glyph, concatenated.
    pub edges:       Vec<EdgeSegment>,
    /// Bitmap dimensions in texels (the GPU shader writes
    /// `bitmap_size.x * bitmap_size.y` pixels).
    pub bitmap_size: UVec2,
    /// Horizontal bearing in em units (matches CPU rasterizer output).
    pub bearing_x:   f32,
    /// Vertical bearing in em units (matches CPU rasterizer output).
    pub bearing_y:   f32,
}

/// Synchronous variant: builds the edge buffer + bitmap dims + bearings
/// for a single glyph. Returns `None` if the glyph has no outline
/// (space) or the computed bitmap is zero-sized.
///
/// Used by the parity test and by the spawned worker task in
/// `enqueue_gpu_glyph`. Cost is dominated by `load_shape_from_face`
/// (~hundreds of µs); the helper is `pub(super)` so it can be spawned
/// inside a worker `async move`.
#[must_use]
pub(super) fn build_edge_buffer(
    font_data: &[u8],
    glyph_index: u16,
    canonical_size: u32,
    sdf_range: f64,
    padding: u32,
) -> Option<GpuGlyphRequestBody> {
    let face = Face::parse(font_data, 0).ok()?;
    let glyph_id = GlyphId(glyph_index);
    let outline = fdsm_ttf_parser::load_shape_from_face(&face, glyph_id)?; // allow-banned: upstream fdsm API name

    let dims =
        bitmap_dims::compute_bitmap_size(&face, glyph_id, canonical_size, sdf_range, padding)?;
    let image_width = dims.width;
    let image_height = dims.height;

    let bbox = face.glyph_bounding_box(glyph_id)?;
    let units_per_em = f64::from(face.units_per_em());
    let scale = f64::from(canonical_size) / units_per_em;
    let glyph_width = f64::from(bbox.x_max - bbox.x_min) * scale;
    let glyph_height = f64::from(bbox.y_max - bbox.y_min) * scale;

    let actual_pad_x = (f64::from(image_width) - glyph_width) / 2.0;
    let actual_pad_y = (f64::from(image_height) - glyph_height) / 2.0;

    // Same affine transform the CPU path applies before rasterization:
    // em-space → pixel-space, y-flipped so the bitmap origin is top-left.
    let tx = actual_pad_x - f64::from(bbox.x_min) * scale;
    let ty = actual_pad_y + f64::from(bbox.y_max) * scale;
    let affine = Affine2::from_matrix_unchecked(Matrix3::new(
        scale, 0.0, tx, 0.0, -scale, ty, 0.0, 0.0, 1.0,
    ));
    let mut outline = outline;
    outline.transform(&affine);

    let mut edges = Vec::new();
    for contour in &outline.contours {
        for segment in &contour.segments {
            edges.push(segment_to_edge(segment));
        }
    }

    let bearing_x =
        (f64::from(bbox.x_min) / units_per_em - actual_pad_x / f64::from(canonical_size)).to_f32();
    let bearing_y =
        (f64::from(bbox.y_max) / units_per_em + actual_pad_y / f64::from(canonical_size)).to_f32();

    Some(GpuGlyphRequestBody {
        edges,
        bitmap_size: UVec2::new(image_width, image_height),
        bearing_x,
        bearing_y,
    })
}

/// Computes only the bitmap dimensions for a glyph, without building
/// the full edge buffer. Used by `enqueue_gpu_glyph` to allocate the
/// page region synchronously before spawning the expensive
/// `build_edge_buffer` work on a worker thread.
#[must_use]
pub(super) fn glyph_bitmap_size(
    font_data: &[u8],
    glyph_index: u16,
    canonical_size: u32,
    sdf_range: f64,
    padding: u32,
) -> Option<UVec2> {
    let face = Face::parse(font_data, 0).ok()?;
    let dims = bitmap_dims::compute_bitmap_size(
        &face,
        GlyphId(glyph_index),
        canonical_size,
        sdf_range,
        padding,
    )?;
    Some(UVec2::new(dims.width, dims.height))
}

fn segment_to_edge(segment: &FdsmSegment) -> EdgeSegment {
    let (kind, point_count) = match segment.order() {
        Order::Linear => (EDGE_KIND_LINEAR, 2_usize),
        Order::Quadratic => (EDGE_KIND_QUADRATIC, 3_usize),
        Order::Cubic => (EDGE_KIND_CUBIC, 4_usize),
    };
    let mut points = [0.0_f32; 8];
    for i in 0..point_count {
        let p = segment.control_point(i);
        points[i * 2] = p.x.to_f32();
        points[i * 2 + 1] = p.y.to_f32();
    }
    EdgeSegment { points, kind }
}
