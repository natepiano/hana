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
use fdsm::shape::ColoredSegment; // allow-banned: upstream fdsm API name
use fdsm::transform::Transform;
use nalgebra::Affine2;
use nalgebra::Matrix3;
use ttf_parser::Face;
use ttf_parser::GlyphId;

use super::coloring;
use crate::text::bitmap_dims;
use crate::text::constants::EDGE_COLORING_ANGLE;
use crate::text::constants::EDGE_COLORING_SEED;
use crate::text::msdf_rasterizer::DistanceField;

/// Discriminant bits in `EdgeSegment::kind` for the segment order.
///
/// Bits 0–1 of `kind` encode the segment order. Bits 2–4 encode the
/// MSDF channel mask (R / G / B = bits 0 / 1 / 2 of the mask).
/// Linear segment discriminant — matches `EDGE_KIND_LINEAR` in
/// `shaders/sdf_gen.wgsl` and `shaders/msdf_gen.wgsl`.
pub(super) const EDGE_KIND_LINEAR: u32 = 0;
/// Quadratic bezier discriminant. Shared across SDF and MSDF kernels;
/// the constants stay together so the WGSL ports validate against the
/// same numeric mapping.
pub(super) const EDGE_KIND_QUADRATIC: u32 = 1;
/// Cubic bezier discriminant.
pub(super) const EDGE_KIND_CUBIC: u32 = 2;
/// Shift to extract the MSDF channel mask from `EdgeSegment::kind`.
/// Used by both `msdf_gen.wgsl` and the parity port.
pub(super) const EDGE_CHANNEL_MASK_SHIFT: u32 = 2;
/// Width of the channel-mask field (bits 2–4 = three channels).
pub(super) const EDGE_CHANNEL_MASK_BITS: u32 = 0b111;

/// Per-edge record sent to the GPU storage buffer.
///
/// 9 × 4 = 36 bytes per record. Holds up to four control points (P0–P3,
/// stored interleaved as `[x0, y0, x1, y1, x2, y2, x3, y3]`) plus a
/// `kind` field. Bits 0–1 of `kind` are the segment order; bits 2–4
/// are the MSDF channel mask (set by `build_edge_buffer` on the MSDF
/// path, left at 0 on the SDF path).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct EdgeSegment {
    pub points: [f32; 8],
    pub kind:   u32,
}

/// Per-corner record consumed by the MSDF error-correction pass.
///
/// Stores the pixel-space position of a contour corner — a corner
/// being the start vertex of a segment where the bitwise AND of the
/// previous and current segment's channel mask is not bright (i.e.
/// has fewer than two bits set). The correction kernel protects the
/// four texels straddling each corner from being flattened to the
/// median.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct CornerPoint {
    pub x: f32,
    pub y: f32,
}

/// Output of [`build_edge_buffer`].
#[derive(Clone, Debug)]
pub struct GpuGlyphRequestBody {
    /// All edges from every contour of the glyph, concatenated.
    pub edges:       Vec<EdgeSegment>,
    /// Contour corner points in pixel space; empty on SDF requests.
    pub corners:     Vec<CornerPoint>,
    /// Bitmap dimensions in texels (the GPU shader writes
    /// `bitmap_size.x * bitmap_size.y` pixels).
    pub bitmap_size: UVec2,
    /// Font-defined horizontal bearing in em units (atlas-invariant —
    /// equals `bbox.x_min / units_per_em`).
    pub bearing_x:   f32,
    /// Font-defined vertical bearing in em units (atlas-invariant —
    /// equals `bbox.y_max / units_per_em`).
    pub bearing_y:   f32,
    /// Atlas-specific horizontal bitmap inset in em units. Quad
    /// builders subtract this from `bearing_x` to position the padded
    /// quad while keeping the ink at the same em-coordinate across
    /// canonical sizes.
    pub pad_x_em:    f32,
    /// Atlas-specific vertical bitmap inset in em units.
    pub pad_y_em:    f32,
}

/// Synchronous variant: builds the edge buffer + bitmap dims + bearings
/// for a single glyph. Returns `None` if the glyph has no outline
/// (space) or the computed bitmap is zero-sized.
///
/// On the MSDF path, edges are first colored via the ink-trap algorithm
/// in [`super::coloring`] (a port of msdfgen's `edgeColoringInkTrap`); the
/// resulting per-segment channel mask is packed into bits 2–4 of each
/// `EdgeSegment::kind`.
///
/// Used by the parity test and by the spawned worker task in
/// `enqueue_gpu_glyph`. The helper is `pub(super)` so it can be spawned
/// inside a worker `async move`.
#[must_use]
pub(super) fn build_edge_buffer(
    font_data: &[u8],
    glyph_index: u16,
    canonical_size: u32,
    sdf_range: f64,
    padding: u32,
    distance_field: DistanceField,
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

    let (edges, corners) = match distance_field {
        DistanceField::Sdf => {
            let mut outline = outline;
            outline.transform(&affine);
            let mut out = Vec::new();
            for contour in &outline.contours {
                for segment in &contour.segments {
                    out.push(segment_to_edge(segment, 0));
                }
            }
            (out, Vec::new())
        },
        DistanceField::Msdf => {
            let sin_alpha = EDGE_COLORING_ANGLE.to_radians().sin();
            let mut colored =
                coloring::edge_coloring_ink_trap(outline, sin_alpha, EDGE_COLORING_SEED);
            colored.transform(&affine);
            let mut edge_out = Vec::new();
            let mut corner_out = Vec::new();
            for contour in &colored.contours {
                collect_contour_corners(&contour.segments, &mut corner_out);
                for colored_segment in &contour.segments {
                    edge_out.push(colored_segment_to_edge(colored_segment));
                }
            }
            (edge_out, corner_out)
        },
    };

    let bearing_x = (f64::from(bbox.x_min) / units_per_em).to_f32();
    let bearing_y = (f64::from(bbox.y_max) / units_per_em).to_f32();
    let horizontal_padding_em = (actual_pad_x / f64::from(canonical_size)).to_f32();
    let vertical_padding_em = (actual_pad_y / f64::from(canonical_size)).to_f32();

    Some(GpuGlyphRequestBody {
        edges,
        corners,
        bitmap_size: UVec2::new(image_width, image_height),
        bearing_x,
        bearing_y,
        pad_x_em: horizontal_padding_em,
        pad_y_em: vertical_padding_em,
    })
}

/// Detects contour corners by the same rule fdsm's
/// `protect_corners` uses: a corner is the start of a segment whose
/// channel-mask AND with the previous segment's mask is not bright
/// (i.e. has fewer than two bits set). `segments` is one contour,
/// already transformed to pixel space.
fn collect_contour_corners(segments: &[ColoredSegment], out: &mut Vec<CornerPoint>) {
    let len = segments.len();
    if len == 0 {
        return;
    }
    for i in 0..len {
        let curr = &segments[i];
        let prev_idx = if i == 0 { len - 1 } else { i - 1 };
        let prev = &segments[prev_idx];
        let common = curr.color.value() & prev.color.value();
        // Color::is_bright: `(c & (c - 1)) != 0` — true iff ≥ 2 bits set.
        let is_bright = common != 0 && (common & common.wrapping_sub(1)) != 0;
        if !is_bright {
            let start = curr.segment.start();
            out.push(CornerPoint {
                x: start.x.to_f32(),
                y: start.y.to_f32(),
            });
        }
    }
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

fn segment_to_edge(segment: &FdsmSegment, channel_mask: u32) -> EdgeSegment {
    let (order_bits, point_count) = match segment.order() {
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
    let kind = order_bits | ((channel_mask & EDGE_CHANNEL_MASK_BITS) << EDGE_CHANNEL_MASK_SHIFT);
    EdgeSegment { points, kind }
}

fn colored_segment_to_edge(colored: &ColoredSegment) -> EdgeSegment {
    segment_to_edge(&colored.segment, u32::from(colored.color.value()))
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::panic,
        clippy::unwrap_used,
        reason = "tests use panic/unwrap for clearer failure messages"
    )]

    use ttf_parser::GlyphId;

    use super::*;
    use crate::text::constants::DEFAULT_GLYPH_PADDING;
    use crate::text::constants::DEFAULT_SDF_RANGE;

    const FONT_DATA: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");
    const EB_GARAMOND: &[u8] = include_bytes!("../../../assets/fonts/EBGaramond-Regular.ttf");
    const CANONICAL_SIZE: u32 = 32;

    fn glyph_index(font_data: &[u8], ch: char) -> u16 {
        let face = ttf_parser::Face::parse(font_data, 0).unwrap_or_else(|e| panic!("parse: {e}"));
        face.glyph_index(ch)
            .unwrap_or_else(|| panic!("no glyph for '{ch}'"))
            .0
    }

    /// Walks the same ink-trap coloring path the GPU edge builder uses and
    /// collects channel masks per segment, then asserts the GPU path's
    /// `EdgeSegment::kind` bits 2–4 match exactly for the same glyph.
    fn channel_masks_from_cpu(font_data: &[u8], ch: char) -> Vec<u32> {
        let face = ttf_parser::Face::parse(font_data, 0).unwrap();
        let glyph_id = GlyphId(glyph_index(font_data, ch));
        let outline = fdsm_ttf_parser::load_shape_from_face(&face, glyph_id).unwrap(); // allow-banned: upstream fdsm API name
        let sin_alpha = EDGE_COLORING_ANGLE.to_radians().sin();
        let colored = coloring::edge_coloring_ink_trap(outline, sin_alpha, EDGE_COLORING_SEED);
        let mut masks = Vec::new();
        for contour in &colored.contours {
            for seg in &contour.segments {
                masks.push(u32::from(seg.color.value()));
            }
        }
        masks
    }

    fn channel_masks_from_gpu(font_data: &[u8], ch: char) -> Vec<u32> {
        let body = build_edge_buffer(
            font_data,
            glyph_index(font_data, ch),
            CANONICAL_SIZE,
            DEFAULT_SDF_RANGE,
            DEFAULT_GLYPH_PADDING,
            DistanceField::Msdf,
        )
        .unwrap();
        body.edges
            .iter()
            .map(|e| (e.kind >> EDGE_CHANNEL_MASK_SHIFT) & EDGE_CHANNEL_MASK_BITS)
            .collect()
    }

    #[test]
    fn edge_coloring_matches_cpu() {
        for (font, label, glyphs) in [
            (FONT_DATA, "JetBrains Mono", ['A', 'O', 'W', 'g'].as_slice()),
            (EB_GARAMOND, "EB Garamond", ['V', 'A', 'g'].as_slice()),
        ] {
            for &ch in glyphs {
                let cpu = channel_masks_from_cpu(font, ch);
                let gpu = channel_masks_from_gpu(font, ch);
                assert_eq!(
                    cpu, gpu,
                    "{label} '{ch}': channel masks disagree between CPU coloring and GPU edge \
                     buffer"
                );
            }
        }
    }

    #[test]
    fn sdf_path_emits_zero_channel_mask() {
        let body = build_edge_buffer(
            FONT_DATA,
            glyph_index(FONT_DATA, 'A'),
            CANONICAL_SIZE,
            DEFAULT_SDF_RANGE,
            DEFAULT_GLYPH_PADDING,
            DistanceField::Sdf,
        )
        .unwrap();
        for edge in &body.edges {
            let mask = (edge.kind >> EDGE_CHANNEL_MASK_SHIFT) & EDGE_CHANNEL_MASK_BITS;
            assert_eq!(mask, 0, "SDF path should leave channel mask bits zero");
        }
    }
}
