use bevy::math::Mat4;
use bevy::math::UVec4;
use bevy::math::Vec2;
use bevy::math::Vec4;
use bevy::render::render_resource::ShaderSize;
use bevy::render::render_resource::ShaderType;
use bevy_kana::ToF32;
use bevy_kana::ToU32;

use super::Bounds;
use super::PathOutline;
use super::QuadraticSegment;

/// Default number of horizontal bands packed per glyph.
pub(crate) const DEFAULT_BAND_COUNT: usize = 96;

const BAND_OVERLAP_EM_UNITS: f32 = 1.0;
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
    pub start_delta: Vec4,
    /// Quadratic second-difference in `.xy`, segment end point in `.zw`.
    pub curve_end:   Vec4,
    /// Conservative control-point bounds minimum in `.xy`, maximum in `.zw`.
    pub bounds:      Vec4,
    /// Distance-solver coefficients in `.xyz`; `.w` is 1.0 when the curve is
    /// assigned to the vertical band for distance (skipped by the horizontal
    /// band's distance loop to avoid duplicate solves), 0.0 otherwise.
    pub solver:      Vec4,
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
            start_delta: Vec4::new(
                segment.start.x,
                segment.start.y,
                control_delta.x,
                control_delta.y,
            ),
            curve_end:   Vec4::new(curve_delta.x, curve_delta.y, segment.end.x, segment.end.y),
            bounds:      Vec4::new(
                segment.start.x.min(segment.control.x).min(segment.end.x),
                segment.start.y.min(segment.control.y).min(segment.end.y),
                segment.start.x.max(segment.control.x).max(segment.end.x),
                segment.start.y.max(segment.control.y).max(segment.end.y),
            ),
            solver:      Vec4::new(
                3.0 * control_delta.dot(curve_delta) * inverse_curve_norm_sq,
                2.0 * control_delta.length_squared() * inverse_curve_norm_sq,
                inverse_curve_norm_sq,
                0.0,
            ),
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
    ) -> Self {
        Self {
            bounds_min_size: Vec4::new(bounds.min.x, bounds.min.y, bounds.width(), bounds.height()),
            band_range:      UVec4::new(
                horizontal_start,
                horizontal_count,
                vertical_start,
                vertical_count,
            ),
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
    /// Clip-space depth nudge in layer units (`command_index × LAYER_DEPTH_BIAS`)
    /// for non-OIT views.
    pub depth_nudge:      f32,
    /// Per-run OIT position-z offset for coplanar ordering.
    pub oit_depth_offset: f32,
}

// GPU-layout assertions against the std430 sizes the shaders index by — the
// WGSL mirror structs in `analytic_path_vertex_pull.wgsl` assume these strides.
// `ShaderSize` measures the encase layout, not the Rust layout.
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
    bounds: Bounds,
    curves: Vec<CurveRecord>,
    bands:  Vec<BandRecord>,
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
}

/// Builds horizontal and vertical band data for one quadratic path outline.
#[must_use]
pub(crate) fn build_packed_path(path: PathOutline, band_count: usize) -> PackedPath {
    let band_count = band_count.max(1);
    let mut curves = Vec::new();
    let mut bands = Vec::with_capacity(band_count * 2);
    let bounds = path.bounds;

    let oriented_segments: Vec<(QuadraticSegment, CurveOrientation)> = path
        .contours
        .iter()
        .flat_map(|contour| contour.segments.iter().copied())
        .map(|segment| {
            let orientation = segment_orientation(&segment);
            (segment, orientation)
        })
        .collect();

    append_bands(
        &oriented_segments,
        bounds.min.y,
        bounds.height(),
        band_count,
        Axis::Horizontal,
        &mut curves,
        &mut bands,
    );
    append_bands(
        &oriented_segments,
        bounds.min.x,
        bounds.width(),
        band_count,
        Axis::Vertical,
        &mut curves,
        &mut bands,
    );

    PackedPath {
        bounds,
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

fn append_bands(
    oriented_segments: &[(QuadraticSegment, CurveOrientation)],
    start_position: f32,
    extent: f32,
    band_count: usize,
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
            band_min - BAND_OVERLAP_EM_UNITS,
            band_max + BAND_OVERLAP_EM_UNITS,
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
    oriented_segments: &[(QuadraticSegment, CurveOrientation)],
    band_min: f32,
    band_max: f32,
    axis: Axis,
    curves: &mut Vec<CurveRecord>,
) {
    let mut filtered: Vec<(QuadraticSegment, CurveOrientation)> = oriented_segments
        .iter()
        .copied()
        .filter(|(segment, _)| overlaps_band(segment, band_min, band_max, axis))
        .filter(|(_, orientation)| match axis {
            Axis::Horizontal => true,
            Axis::Vertical => *orientation == CurveOrientation::Vertical,
        })
        .collect();

    filtered.sort_by(|left, right| {
        descending_band_sort_value(&right.0, axis)
            .total_cmp(&descending_band_sort_value(&left.0, axis))
    });
    curves.extend(filtered.iter().map(|(segment, orientation)| {
        let mut record = CurveRecord::from(segment);
        if *orientation == CurveOrientation::Vertical {
            record.solver.w = 1.0;
        }
        record
    }));
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
