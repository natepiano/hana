use bevy::math::Mat4;
use bevy::math::UVec4;
use bevy::math::Vec2;
use bevy::math::Vec4;
use bevy::render::render_resource::ShaderSize;
use bevy::render::render_resource::ShaderType;
use bevy_kana::ToF32;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use super::Bounds;
use super::PathContour;
use super::PathOutline;
use super::QuadraticSegment;

/// Default number of horizontal bands packed per glyph.
pub(crate) const DEFAULT_BAND_COUNT: usize = 96;

const BAND_OVERLAP_EM_UNITS: f32 = 1.0;
/// Fewest curves a band must hold. Band count is capped at
/// `ceil(curve_count / this)` so a sparse path collapses toward one band and
/// its distance scan sees every curve at any grazing angle, where the
/// on-screen footprint exceeds a thin band's overlap.
const MIN_CURVES_PER_BAND: usize = 256;
const CURVE_DEGENERATE_EPS: f32 = 0.000_000_01;
/// Bow-to-chord ratio below which a segment packs as exactly linear.
///
/// Coordinate scaling rounds start/control/end independently, so a
/// midpoint-control line segment can carry a few-ulp second difference.
/// The shader's winding root `(-b ± sqrt(b² - 4ac)) / 2a` then cancels
/// catastrophically (for a long edge, `b²`'s ulp exceeds `4ac`), losing the
/// crossing entirely. A bow this small is far below one design unit of
/// deviation, so snapping it to zero routes such segments through the exact
/// linear solve with no visible change.
const CURVE_LINEAR_SNAP_RATIO: f32 = 0.000_1;

/// GPU curve record for a quadratic Bezier segment.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(crate) struct CurveRecord {
    /// Segment start point in `.xy`, control-minus-start in `.zw`.
    pub start_delta:   Vec4,
    /// Quadratic second-difference in `.xy`, segment end point in `.zw`.
    pub curve_end:     Vec4,
    /// Conservative control-point bounds minimum in `.xy`, maximum in `.zw`.
    pub bounds:        Vec4,
    /// Distance-solver coefficients in `.xyz`; `.w` carries the owning
    /// contour's narrowest stroke in design units (per-curve hairline
    /// dilation), 0.0 for undilated contours (text glyphs).
    pub solver:        Vec4,
    /// Owning contour's resolved hairline fade exponent
    /// (`PathContour::fade_exponent`). Each coverage evaluation fades by the
    /// winning (nearest) curve's exponent, so one merged path can mix fading
    /// and non-fading contours. `0.0` disables fade for this curve.
    pub fade_exponent: f32,
    /// Outward unit normal of this segment's edge line, oriented by the owning
    /// contour's winding. The line branch's convex-corner clip reads it as the
    /// edge half-plane direction; the radial `normalize(point - closest)`
    /// degenerates to the vertex direction past a corner, so it cannot stand in
    /// here. `Vec2::ZERO` (text glyphs, degenerate edges) routes the shader back
    /// to the radial normal.
    pub edge_normal:   Vec2,
}

impl From<&QuadraticSegment> for CurveRecord {
    fn from(segment: &QuadraticSegment) -> Self {
        let control_delta = segment.control - segment.start;
        let mut curve_delta = segment.end - 2.0 * segment.control + segment.start;
        let chord_sq = (segment.end - segment.start).length_squared();
        if curve_delta.length_squared()
            < chord_sq * (CURVE_LINEAR_SNAP_RATIO * CURVE_LINEAR_SNAP_RATIO)
        {
            curve_delta = Vec2::ZERO;
        }
        let curve_norm_sq = curve_delta.length_squared();
        let inverse_curve_norm_sq = if curve_norm_sq >= CURVE_DEGENERATE_EPS {
            curve_norm_sq.recip()
        } else {
            0.0
        };
        Self {
            start_delta:   Vec4::new(
                segment.start.x,
                segment.start.y,
                control_delta.x,
                control_delta.y,
            ),
            curve_end:     Vec4::new(curve_delta.x, curve_delta.y, segment.end.x, segment.end.y),
            bounds:        Vec4::new(
                segment.start.x.min(segment.control.x).min(segment.end.x),
                segment.start.y.min(segment.control.y).min(segment.end.y),
                segment.start.x.max(segment.control.x).max(segment.end.x),
                segment.start.y.max(segment.control.y).max(segment.end.y),
            ),
            solver:        Vec4::new(
                3.0 * control_delta.dot(curve_delta) * inverse_curve_norm_sq,
                2.0 * control_delta.length_squared() * inverse_curve_norm_sq,
                inverse_curve_norm_sq,
                0.0,
            ),
            fade_exponent: 0.0,
            // Winding is a per-contour property; the packer overwrites this from
            // the owning contour's orientation. A lone segment has no winding, so
            // it keeps the radial-fallback sentinel.
            edge_normal:   Vec2::ZERO,
        }
    }
}

/// GPU band record pointing at a contiguous curve range.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(crate) struct BandRecord {
    /// First curve record for this band.
    pub start: u32,
    /// Number of curve records for this band.
    pub count: u32,
    /// Lower band edge in font design-space units on the banded axis.
    pub y_min: f32,
    /// Upper band edge in font design-space units on the banded axis.
    pub y_max: f32,
}

/// GPU glyph record for one unique glyph in a packed text run.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(crate) struct GlyphRecord {
    /// Bounds minimum in `.xy`, bounds size in `.zw`, in font design-space units.
    pub bounds_min_size: Vec4,
    /// Horizontal band start/count in `.xy`, vertical band start/count in `.zw`.
    pub band_range:      UVec4,
    /// Narrowest stroke of the path in design-space units; the shader dilates
    /// the silhouette so that stroke covers at least one screen pixel
    /// (hairline rendering). `0.0` disables — text glyphs stay undilated.
    pub min_feature:     f32,
}

impl GlyphRecord {
    /// Creates a glyph record that points into the combined run band buffer.
    #[must_use]
    pub fn new(
        bounds: Bounds,
        horizontal_start: u32,
        horizontal_count: u32,
        vertical_start: u32,
        vertical_count: u32,
        min_feature: f32,
    ) -> Self {
        Self {
            bounds_min_size: Vec4::new(bounds.min.x, bounds.min.y, bounds.width(), bounds.height()),
            band_range: UVec4::new(
                horizontal_start,
                horizontal_count,
                vertical_start,
                vertical_count,
            ),
            min_feature,
        }
    }
}

/// GPU record for one batched glyph quad. The vertex-pulling shader expands
/// each record into four corners: positions from `rect_min`/`rect_size` in run
/// layout space, padded glyph UVs from `uv_min`/`uv_size`, the shared-atlas
/// slot through `atlas_index`, and the owning run through `run_index`.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(crate) struct GlyphInstanceRecord {
    /// Quad minimum corner (left, bottom) in run layout space, clipped.
    pub rect_min:    Vec2,
    /// Quad size (width, height) in run layout space.
    pub rect_size:   Vec2,
    /// Padded glyph UV at the quad's (left, top) corner.
    pub uv_min:      Vec2,
    /// Padded glyph UV extent from `uv_min` toward (right, bottom).
    pub uv_size:     Vec2,
    /// Shared-atlas [`GlyphRecord`] index.
    pub atlas_index: u32,
    /// [`RunRecord`] index within the same batch.
    pub run_index:   u32,
}

/// GPU record for one text run inside a batch: world placement plus the
/// per-run values that batching moves out of the material uniform.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(crate) struct RunRecord {
    /// Label world matrix (run layout space → world).
    pub transform:        Mat4,
    /// Linear fill color.
    pub fill_color:       Vec4,
    /// Visible render mode (`RenderMode` as `u32`).
    pub render_mode:      u32,
    /// Clip-space depth nudge in layer units for non-OIT views.
    pub depth_nudge:      f32,
    /// Per-run OIT position-z offset for coplanar ordering.
    pub oit_depth_offset: f32,
    /// Resolved anti-alias mode bits (`AntiAlias::aa_flags`:
    /// `AA_FLAG_SUPERSAMPLE` | `AA_FLAG_BAND`). Per-record so an element-level
    /// AA override never splits a batch or material.
    pub aa_flags:         u32,
}

// GPU-layout assertions against the std430 sizes the shaders index by — the
// WGSL mirror structs in `analytic_path_vertex_pull.wgsl` assume these strides.
// `ShaderSize` measures the encase layout, not the Rust layout.
const _: () = assert!(CurveRecord::SHADER_SIZE.get() == 80);
const _: () = assert!(GlyphRecord::SHADER_SIZE.get() == 48);
const _: () = assert!(GlyphInstanceRecord::SHADER_SIZE.get() == 40);
const _: () = assert!(RunRecord::SHADER_SIZE.get() == 96);

/// Shared name for a renderer path atlas record.
#[allow(
    dead_code,
    reason = "Phase A names shared path record types before Phase B consumes them"
)]
pub(crate) type PathRecord = GlyphRecord;

/// Shared name for a batched path instance record.
#[allow(
    dead_code,
    reason = "Phase A names shared path record types before Phase B consumes them"
)]
pub(crate) type PathInstanceRecord = GlyphInstanceRecord;

/// Shared name for a batched path run record.
#[allow(
    dead_code,
    reason = "Phase A names shared path record types before Phase B consumes them"
)]
pub(crate) type PathRunRecord = RunRecord;

/// One analytic path's packed curve and band data for the shader.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PackedPath {
    bounds:           Bounds,
    curves:           Vec<CurveRecord>,
    bands:            Vec<BandRecord>,
    horizontal_count: u32,
    vertical_count:   u32,
}

/// Compatibility alias for text glyph caches while text is bridged onto the
/// shared path renderer.
pub(crate) type GlyphOutline = PackedPath;

impl PackedPath {
    /// Path bounds in local design-space units.
    #[must_use]
    pub const fn bounds(&self) -> Bounds { self.bounds }

    /// Band-packed curve records.
    #[must_use]
    pub fn curves(&self) -> &[CurveRecord] { &self.curves }

    /// Band records.
    #[must_use]
    pub fn bands(&self) -> &[BandRecord] { &self.bands }

    /// Number of horizontal bands (first in the band list).
    #[must_use]
    pub const fn horizontal_count(&self) -> u32 { self.horizontal_count }

    /// Number of vertical bands (after the horizontal bands).
    #[must_use]
    pub const fn vertical_count(&self) -> u32 { self.vertical_count }
}

/// Per-axis band counts and inclusion overlap for packing one path.
#[derive(Clone, Copy, Debug)]
pub(super) struct BandLayout {
    /// Horizontal band count (splits the y extent).
    pub horizontal_count: usize,
    /// Vertical band count (splits the x extent).
    pub vertical_count:   usize,
    /// Design-unit margin around each band; curves within it are included so
    /// the distance scan near a band edge still sees them.
    pub overlap:          f32,
}

impl BandLayout {
    /// Equal band counts on both axes with the text overlap margin.
    #[must_use]
    pub(super) const fn uniform(band_count: usize) -> Self {
        Self {
            horizontal_count: band_count,
            vertical_count:   band_count,
            overlap:          BAND_OVERLAP_EM_UNITS,
        }
    }

    /// Per-axis counts sized so each band spans about `target_extent` design
    /// units, with a half-band overlap. Small paths keep one exact band (the
    /// distance scan sees every curve at any zoom); large merged paths split
    /// so the per-fragment curve loop stays short.
    #[must_use]
    pub fn for_extents(bounds: Bounds, target_extent: f32, curve_count: usize) -> Self {
        let curve_cap = band_cap_for_curves(curve_count);
        Self {
            horizontal_count: band_count_for_extent(bounds.height(), target_extent).min(curve_cap),
            vertical_count:   band_count_for_extent(bounds.width(), target_extent).min(curve_cap),
            overlap:          target_extent * 0.5,
        }
    }
}

fn band_count_for_extent(extent: f32, target_extent: f32) -> usize {
    if target_extent <= 0.0 || !(extent / target_extent).is_finite() {
        return 1;
    }
    (extent / target_extent)
        .ceil()
        .to_usize()
        .clamp(1, DEFAULT_BAND_COUNT)
}

/// Largest band count a path of `curve_count` segments justifies, holding at
/// least [`MIN_CURVES_PER_BAND`] curves per band.
fn band_cap_for_curves(curve_count: usize) -> usize {
    curve_count
        .div_ceil(MIN_CURVES_PER_BAND)
        .clamp(1, DEFAULT_BAND_COUNT)
}

/// Builds horizontal and vertical band data for one quadratic path outline
/// with equal per-axis band counts (the text glyph path).
#[must_use]
pub(crate) fn build_packed_path(path: PathOutline, band_count: usize) -> PackedPath {
    build_packed_path_with_layout(path, BandLayout::uniform(band_count))
}

/// Builds horizontal and vertical band data for one quadratic path outline.
#[must_use]
pub(super) fn build_packed_path_with_layout(path: PathOutline, layout: BandLayout) -> PackedPath {
    let horizontal_count = layout.horizontal_count.max(1);
    let vertical_count = layout.vertical_count.max(1);
    let mut curves = Vec::new();
    let mut bands = Vec::with_capacity(horizontal_count + vertical_count);
    let bounds = path.bounds;

    let oriented_segments: Vec<BandedSegment> = path
        .contours
        .iter()
        .flat_map(|contour| {
            let chirality = contour_signed_area(contour).signum();
            contour.segments.iter().map(move |segment| BandedSegment {
                segment:       *segment,
                orientation:   segment_orientation(segment),
                min_feature:   contour.min_feature,
                fade_exponent: contour.fade_exponent,
                edge_normal:   segment_outward_normal(segment, chirality),
            })
        })
        .collect();

    append_bands(
        &oriented_segments,
        bounds.min.y,
        bounds.height(),
        horizontal_count,
        layout.overlap,
        Axis::Horizontal,
        &mut curves,
        &mut bands,
    );
    append_bands(
        &oriented_segments,
        bounds.min.x,
        bounds.width(),
        vertical_count,
        layout.overlap,
        Axis::Vertical,
        &mut curves,
        &mut bands,
    );

    PackedPath {
        bounds,
        curves,
        bands,
        horizontal_count: horizontal_count.to_u32(),
        vertical_count: vertical_count.to_u32(),
    }
}

/// Whether every contour of `path` is a straight-edge polygon (each segment
/// snaps to exactly linear under the same bow-to-chord test packing applies).
/// Straight-edge convex marks (Segment rectangles, Triangle/Square/Diamond
/// caps) render through the half-plane product path; a curved contour (a Circle
/// cap's ellipse arcs) keeps the curve-distance path.
#[must_use]
pub(super) fn path_is_straight_polygon(path: &PathOutline) -> bool {
    path.contours
        .iter()
        .flat_map(|contour| &contour.segments)
        .all(segment_is_linear)
}

fn segment_is_linear(segment: &QuadraticSegment) -> bool {
    let curve_delta = segment.end - 2.0 * segment.control + segment.start;
    let chord_sq = (segment.end - segment.start).length_squared();
    curve_delta.length_squared() < chord_sq * (CURVE_LINEAR_SNAP_RATIO * CURVE_LINEAR_SNAP_RATIO)
}

/// Packs each contour as one convex polygon for the half-plane product path:
/// one band per contour holding its edges in contour order (no axis split, no
/// sort, no overlap margin), each edge carrying its outward half-plane
/// [`CurveRecord::edge_normal`], start point, contour stroke (`solver.w`), and
/// fade exponent. `vertical_count` is `0` — the shader reads that as the
/// polygon-mode sentinel and evaluates `analytic_polygon_coverage` instead of
/// the curve-distance scan.
#[must_use]
pub(super) fn build_packed_polygons(path: PathOutline) -> PackedPath {
    let bounds = path.bounds;
    let mut curves = Vec::new();
    let mut bands = Vec::with_capacity(path.contours.len());
    for contour in &path.contours {
        let chirality = contour_signed_area(contour).signum();
        let start = curves.len().to_u32();
        for segment in &contour.segments {
            let normal = segment_outward_normal(segment, chirality);
            let mut record = CurveRecord::from(segment);
            // Per-edge hairline-floor dimension = the contour's perpendicular
            // extent in this edge's outward normal. A stroke's long edge gets
            // the (thin) width, so the floor pads it to one screen pixel; its
            // cap gets the (long) length, so the floor never inflates the line
            // past its ends (a foreshortened cap floor would paint a stray
            // ~1px dot off the receding end).
            record.solver.w = edge_slab_width(contour, segment, normal);
            record.fade_exponent = contour.fade_exponent;
            record.edge_normal = normal;
            curves.push(record);
        }
        let count = curves.len().to_u32() - start;
        if count == 0 {
            continue;
        }
        bands.push(BandRecord {
            start,
            count,
            y_min: 0.0,
            y_max: 0.0,
        });
    }
    PackedPath {
        bounds,
        horizontal_count: bands.len().to_u32(),
        vertical_count: 0,
        curves,
        bands,
    }
}

#[derive(Clone, Copy)]
enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CurveOrientation {
    Horizontal,
    Vertical,
}

/// One outline segment with the per-band packing inputs it carries.
#[derive(Clone, Copy)]
struct BandedSegment {
    segment:       QuadraticSegment,
    orientation:   CurveOrientation,
    min_feature:   f32,
    fade_exponent: f32,
    edge_normal:   Vec2,
}

fn append_bands(
    oriented_segments: &[BandedSegment],
    start_position: f32,
    extent: f32,
    band_count: usize,
    overlap: f32,
    axis: Axis,
    curves: &mut Vec<CurveRecord>,
    bands: &mut Vec<BandRecord>,
) {
    let band_size = extent.max(1.0) / band_count.to_f32();

    for band_index in 0..band_count {
        let band_min = start_position + band_size * band_index.to_f32();
        let band_max = if band_index + 1 == band_count {
            start_position + extent
        } else {
            band_min + band_size
        };
        let start = curves.len().to_u32();
        append_band_curves(
            oriented_segments,
            band_min - overlap,
            band_max + overlap,
            axis,
            curves,
        );
        bands.push(BandRecord {
            start,
            count: curves.len().to_u32() - start,
            y_min: band_min,
            y_max: band_max,
        });
    }
}

fn append_band_curves(
    oriented_segments: &[BandedSegment],
    band_min: f32,
    band_max: f32,
    axis: Axis,
    curves: &mut Vec<CurveRecord>,
) {
    let mut filtered: Vec<BandedSegment> = oriented_segments
        .iter()
        .copied()
        .filter(|banded| overlaps_band(&banded.segment, band_min, band_max, axis))
        .filter(|banded| match axis {
            Axis::Horizontal => true,
            Axis::Vertical => banded.orientation == CurveOrientation::Vertical,
        })
        .collect();

    filtered.sort_by(|left, right| {
        descending_band_sort_value(&right.segment, axis)
            .total_cmp(&descending_band_sort_value(&left.segment, axis))
    });
    curves.extend(filtered.iter().map(|banded| {
        let mut record = CurveRecord::from(&banded.segment);
        record.solver.w = banded.min_feature;
        record.fade_exponent = banded.fade_exponent;
        record.edge_normal = banded.edge_normal;
        record
    }));
}

/// Signed area of the contour's start-point polygon. The sign gives the
/// winding orientation (positive = counter-clockwise) that orients each
/// segment's outward edge normal.
fn contour_signed_area(contour: &PathContour) -> f32 {
    let segments = &contour.segments;
    let mut area = 0.0;
    for index in 0..segments.len() {
        let current = segments[index].start;
        let next = segments[(index + 1) % segments.len()].start;
        area += current.x.mul_add(next.y, -(next.x * current.y));
    }
    area * 0.5
}

/// Outward unit normal of a segment's edge line given the contour's winding
/// chirality (`+1` counter-clockwise, `-1` clockwise). A counter-clockwise
/// contour's outward direction is the chord's right perpendicular. Returns
/// `Vec2::ZERO` for a degenerate (zero-length) chord so the shader keeps the
/// radial fallback.
fn segment_outward_normal(segment: &QuadraticSegment, chirality: f32) -> Vec2 {
    let chord = segment.end - segment.start;
    let length = chord.length();
    if length <= CURVE_DEGENERATE_EPS {
        return Vec2::ZERO;
    }
    Vec2::new(chord.y, -chord.x) / length * chirality
}

/// Perpendicular extent of `contour` in `normal`'s direction: the dimension the
/// hairline floor pads for the edge with that outward normal. For a convex
/// contour the edge's start sits on the supporting face, so this is the full
/// slab thickness between that edge's line and the opposite vertex. Returns the
/// contour stroke for a degenerate (zero-normal) edge.
fn edge_slab_width(contour: &PathContour, segment: &QuadraticSegment, normal: Vec2) -> f32 {
    if normal == Vec2::ZERO {
        return contour.min_feature;
    }
    let edge_support = normal.dot(segment.start);
    let min_support = contour
        .segments
        .iter()
        .map(|other| normal.dot(other.start))
        .fold(f32::INFINITY, f32::min);
    (edge_support - min_support).max(0.0)
}

const fn segment_orientation(segment: &QuadraticSegment) -> CurveOrientation {
    let x_extent =
        segment_axis_max(segment, Axis::Vertical) - segment_axis_min(segment, Axis::Vertical);
    let y_extent =
        segment_axis_max(segment, Axis::Horizontal) - segment_axis_min(segment, Axis::Horizontal);
    if y_extent > x_extent {
        CurveOrientation::Vertical
    } else {
        CurveOrientation::Horizontal
    }
}

fn overlaps_band(segment: &QuadraticSegment, band_min: f32, band_max: f32, axis: Axis) -> bool {
    // Axis-parallel edges (a horizontal line in a horizontal band, etc.) are kept:
    // they add 0 to winding (`curve_winding` returns 0 when the scanline is parallel)
    // but DO carry distance, which the signed-distance field needs. Dropping them
    // left the field blind to those edges, so it saturated to ±edge_width near them
    // and the screen-space AA band ballooned at grazing angles.
    let segment_min = segment_axis_min(segment, axis);
    let segment_max = segment_axis_max(segment, axis);
    segment_min <= band_max && segment_max >= band_min
}

const fn segment_axis_min(segment: &QuadraticSegment, axis: Axis) -> f32 {
    match axis {
        Axis::Horizontal => segment.start.y.min(segment.control.y).min(segment.end.y),
        Axis::Vertical => segment.start.x.min(segment.control.x).min(segment.end.x),
    }
}

const fn segment_axis_max(segment: &QuadraticSegment, axis: Axis) -> f32 {
    match axis {
        Axis::Horizontal => segment.start.y.max(segment.control.y).max(segment.end.y),
        Axis::Vertical => segment.start.x.max(segment.control.x).max(segment.end.x),
    }
}

const fn descending_band_sort_value(segment: &QuadraticSegment, axis: Axis) -> f32 {
    match axis {
        Axis::Horizontal => segment.start.x.max(segment.control.x).max(segment.end.x),
        Axis::Vertical => segment.start.y.max(segment.control.y).max(segment.end.y),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should fail loudly when encase encoding breaks"
)]
mod tests {
    use bevy::render::render_resource::encase::StorageBuffer;

    use super::*;

    fn glyph_record(seed: f32, atlas_index: u32, run_index: u32) -> GlyphInstanceRecord {
        GlyphInstanceRecord {
            rect_min: Vec2::new(seed, seed + 0.5),
            rect_size: Vec2::new(seed + 1.0, seed + 1.5),
            uv_min: Vec2::new(-0.0625, -0.0625),
            uv_size: Vec2::new(1.125, 1.125),
            atlas_index,
            run_index,
        }
    }

    fn run_record(seed: f32) -> RunRecord {
        RunRecord {
            transform:        Mat4::from_translation(bevy::math::Vec3::new(
                seed,
                -seed,
                seed * 2.0,
            )),
            fill_color:       Vec4::new(0.25, 0.5, 0.75, 1.0),
            render_mode:      1,
            depth_nudge:      seed,
            oit_depth_offset: -seed,
            aa_flags:         3,
        }
    }

    #[test]
    fn glyph_instance_records_round_trip_through_encase_at_40_byte_stride() {
        let records = vec![glyph_record(1.0, 7, 0), glyph_record(-2.0, 11, 1)];
        let mut encoded = StorageBuffer::new(Vec::<u8>::new());
        encoded.write(&records).expect("records should encode");
        assert_eq!(
            encoded.as_ref().len(),
            80,
            "two records at a 40-byte stride"
        );

        let mut decoded: Vec<GlyphInstanceRecord> = Vec::new();
        StorageBuffer::new(encoded.as_ref().clone())
            .read(&mut decoded)
            .expect("records should decode");
        assert_eq!(decoded, records);
    }

    #[test]
    fn run_records_round_trip_through_encase_at_96_byte_stride() {
        let records = vec![run_record(0.0), run_record(3.5)];
        let mut encoded = StorageBuffer::new(Vec::<u8>::new());
        encoded.write(&records).expect("records should encode");
        assert_eq!(
            encoded.as_ref().len(),
            192,
            "two records at a 96-byte stride"
        );

        let mut decoded: Vec<RunRecord> = Vec::new();
        StorageBuffer::new(encoded.as_ref().clone())
            .read(&mut decoded)
            .expect("records should decode");
        assert_eq!(decoded, records);
    }
}
