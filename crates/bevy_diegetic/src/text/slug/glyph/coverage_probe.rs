//! Coverage-model tests for the slug text anti-aliasing path.
//!
//! These tests do NOT run the real shader — WGSL executes on the GPU and cannot
//! be called from a Rust test. They exercise three things instead:
//!
//! - the real glyph **packer** (`build_packed_glyph`, the band/curve records, `DEFAULT_BAND_COUNT`)
//!   — production code; if it changes, these numbers move,
//! - a Rust **copy of the `slug_text.wgsl` `aa_band` math** (`Probe`) — kept in sync with the
//!   shader by hand,
//! - an independent brute-force **ground truth** (`GroundTruth`) that counts inside/outside over
//!   the footprint with no bands at all.
//!
//! Each test compares the band model against the brute-force ground truth and
//! locks in a fact the shader design rests on:
//! 1. a single coverage sample over-covers a sharp convex corner under a grazing footprint (the
//!    "wing"), and the over-coverage is inherent to the single-sample model — it persists even with
//!    exact distance and band,
//! 2. the anisotropic stride (`aniso_shader_fix`, `per_band = d_minor + d_major/N`) brings corner
//!    coverage back to ground truth, and beats a fixed 4-tap supersample — which is why the fix
//!    strides along the foreshortened axis rather than sampling a fixed grid,
//! 3. the stride does not alias a straight edge.
//!
//! IMPORTANT: `Probe` mirrors the `slug_text.wgsl` coverage math by hand. If you
//! change that shader, update `Probe` to match and re-run these tests. The
//! [`shader_mirror_matches_wgsl`] test hashes the shader file and fails when it
//! changes, as a reminder to re-check the mirror — it detects *that* the shader
//! changed, not whether the change was correct.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    clippy::imprecise_flops,
    reason = "line-for-line CPU mirror of the slug_text.wgsl coverage math: the int/f32/u32 casts \
              reproduce the shader's f32()/u32() conversions, and mul_add/cbrt are deliberately \
              avoided so the Rust results match the shader's operation order exactly"
)]

use bevy::math::Vec2;
use bevy::math::Vec4;

use super::CurveRecord;
use super::GlyphRecord;
use super::build_packed_glyph;
use super::outline::Bounds;
use super::outline::Contour;
use super::outline::Glyph;
use super::outline::QuadraticSegment;
use super::packing::BandRecord;
use super::packing::DEFAULT_BAND_COUNT;

const ROOT_EPSILON: f32 = 0.000_01;
const EDGE_FILTER_WIDTH: f32 = 1.2;
const SQRT_3_OVER_2: f32 = 0.866_025_4;

// Test configuration: a ~40px-tall glyph viewed at a fixed extreme anisotropy.
const PX: f32 = 700.0 / 40.0;
const ANISO: f32 = 12.0;
const GT_SAMPLES: u32 = 24;
const MAX_ANISO_SAMPLES: u32 = 16;
/// Footprint orientations whose worst over-coverage point lands on the apex
/// (the acute convex corner). 90° is excluded — there the grazing footprint
/// hits a base corner, a different case.
const GRAZING_ANGLES: [f32; 5] = [0.0, 30.0, 60.0, 120.0, 150.0];

// Golden thresholds (the slack, not the answer — the answer is measured each run
// by the brute-force ground truth). Chosen with margin over the observed values.
const WING_MIN_OVERCOVERAGE: f32 = 0.40;
const MODEL_CAUSE_MIN_OVERCOVERAGE: f32 = 0.35;
const FIX_MAX_ERROR: f32 = 0.08;
const SUPER4_MIN_OVERCOVERAGE: f32 = 0.20;
const FIX_IMPROVEMENT_OVER_SUPER4: f32 = 0.15;
const EDGE_FIX_MAX_ERROR: f32 = 0.06;

// ---- shader-math replica (mirrors slug_text.wgsl exactly) ------------------

struct Probe {
    record: GlyphRecord,
    curves: Vec<CurveRecord>,
    bands:  Vec<BandRecord>,
}

fn xy(v: Vec4) -> Vec2 { Vec2::new(v.x, v.y) }
fn zw(v: Vec4) -> Vec2 { Vec2::new(v.z, v.w) }

impl Probe {
    fn bounds_min(&self) -> Vec2 { xy(self.record.bounds_min_size) }
    fn bounds_size(&self) -> Vec2 { zw(self.record.bounds_min_size) }

    fn horizontal_band_index(&self, point: Vec2) -> u32 {
        let bmin = self.bounds_min();
        let bsize = self.bounds_size();
        let band_count = self.record.band_range.y;
        let normalized_y = ((point.y - bmin.y) / bsize.y.max(ROOT_EPSILON)).clamp(0.0, 0.999_999);
        ((normalized_y * band_count as f32) as u32).min(band_count - 1)
    }

    fn vertical_band_index(&self, point: Vec2) -> u32 {
        let bmin = self.bounds_min();
        let bsize = self.bounds_size();
        let band_count = self.record.band_range.w;
        let normalized_x = ((point.x - bmin.x) / bsize.x.max(ROOT_EPSILON)).clamp(0.0, 0.999_999);
        ((normalized_x * band_count as f32) as u32).min(band_count - 1)
    }

    fn outside_bounds(&self, point: Vec2) -> bool {
        let bmin = self.bounds_min();
        let bmax = bmin + self.bounds_size();
        point.x < bmin.x || point.x > bmax.x || point.y < bmin.y || point.y > bmax.y
    }

    fn winding_at(&self, point: Vec2) -> i32 {
        if self.outside_bounds(point) {
            return 0;
        }
        let band =
            &self.bands[(self.record.band_range.x + self.horizontal_band_index(point)) as usize];
        let mut winding = 0;
        for offset in 0..band.count {
            winding += curve_winding(self.curves[(band.start + offset) as usize], point);
        }
        winding
    }

    fn any_outside_neighbor(&self, point: Vec2, edge_width: f32) -> bool {
        self.winding_at(point + Vec2::new(edge_width, 0.0)) == 0
            || self.winding_at(point - Vec2::new(edge_width, 0.0)) == 0
            || self.winding_at(point + Vec2::new(0.0, edge_width)) == 0
            || self.winding_at(point - Vec2::new(0.0, edge_width)) == 0
    }

    /// `horizontal_coverage_terms` + `nearest_vertical_curve_distance_sq` fused.
    fn winding_and_distance_sq(&self, point: Vec2, edge_width_sq: f32) -> (i32, f32) {
        let include_winding = !self.outside_bounds(point);
        let mut winding = 0;
        let mut distance_sq = 1.0e12_f32;

        let hband =
            &self.bands[(self.record.band_range.x + self.horizontal_band_index(point)) as usize];
        for offset in 0..hband.count {
            let curve = self.curves[(hband.start + offset) as usize];
            if include_winding {
                winding += curve_winding(curve, point);
            }
            if curve_bounds_distance_sq(point, curve) <= edge_width_sq {
                distance_sq = distance_sq.min(curve_distance_sq(point, curve));
            }
        }

        let vband =
            &self.bands[(self.record.band_range.z + self.vertical_band_index(point)) as usize];
        for offset in 0..vband.count {
            let curve = self.curves[(vband.start + offset) as usize];
            if curve_bounds_distance_sq(point, curve) <= edge_width_sq {
                distance_sq = distance_sq.min(curve_distance_sq(point, curve));
            }
        }
        (winding, distance_sq)
    }

    /// Verbatim port of `signed_distance` (the `aa_band` feeder).
    fn signed_distance(&self, point: Vec2, edge_width_sq: f32) -> f32 {
        let edge_width = edge_width_sq.sqrt();
        let (winding, distance_sq) = self.winding_and_distance_sq(point, edge_width_sq);
        let inside = winding != 0;
        if distance_sq > edge_width_sq {
            return if inside { -edge_width } else { edge_width };
        }
        if inside && !self.any_outside_neighbor(point, edge_width) {
            return -edge_width;
        }
        let distance = distance_sq.sqrt();
        if inside { -distance } else { distance }
    }

    /// Full `aa_band` `render_coverage` for a chosen point + footprint (dx, dy).
    /// Returns (`single_sample`, supersampled).
    fn aa_band_coverage(&self, point: Vec2, dx: Vec2, dy: Vec2) -> (f32, f32) {
        let pixel = Vec2::new(
            (dx.x.abs() + dy.x.abs()).max(ROOT_EPSILON),
            (dx.y.abs() + dy.y.abs()).max(ROOT_EPSILON),
        );
        let edge_width = (pixel.x.max(pixel.y) * EDGE_FILTER_WIDTH).max(ROOT_EPSILON);
        let edge_width_sq = edge_width * edge_width;

        // band = fwidth(signed_distance) modeled as forward differences across
        // the 2x2 quad.
        let sd_center = self.signed_distance(point, edge_width_sq);
        let band = ((self.signed_distance(point + dx, edge_width_sq) - sd_center).abs()
            + (self.signed_distance(point + dy, edge_width_sq) - sd_center).abs())
        .max(ROOT_EPSILON);

        let single = band_coverage(sd_center, band);

        let mut sum = 0.0;
        for (a, b) in [
            (0.375, 0.125),
            (-0.125, 0.375),
            (-0.375, -0.125),
            (0.125, -0.375),
        ] {
            sum += band_coverage(
                self.signed_distance(point + a * dx + b * dy, edge_width_sq),
                band,
            );
        }
        (single, sum * 0.25)
    }

    /// Mirrors the shader's anisotropic stride: stride N samples along the longer
    /// footprint axis, `per_band = d_minor + d_major/N`. The Lipschitz clamp on
    /// the finite differences removes the spike when a sample crosses into a band
    /// where a horizontal/vertical edge isn't visible (returns a far curve).
    fn aniso_shader_fix(&self, point: Vec2, dx: Vec2, dy: Vec2, max_n: u32) -> f32 {
        let pixel = Vec2::new(
            (dx.x.abs() + dy.x.abs()).max(ROOT_EPSILON),
            (dx.y.abs() + dy.y.abs()).max(ROOT_EPSILON),
        );
        let edge_width = (pixel.x.max(pixel.y) * EDGE_FILTER_WIDTH).max(ROOT_EPSILON);
        let edge_width_sq = edge_width * edge_width;
        let sd_c = self.signed_distance(point, edge_width_sq);

        let (major, minor, major_len, minor_len) = if dx.length() >= dy.length() {
            (dx, dy, dx.length(), dy.length())
        } else {
            (dy, dx, dy.length(), dx.length())
        };
        let n = ((major_len / minor_len.max(ROOT_EPSILON)).ceil() as u32).clamp(1, max_n);
        // sd is 1-Lipschitz, so |Δsd| over a step can't exceed the step length.
        let d_major = (self.signed_distance(point + major, edge_width_sq) - sd_c)
            .abs()
            .min(major_len);
        let d_minor = (self.signed_distance(point + minor, edge_width_sq) - sd_c)
            .abs()
            .min(minor_len);
        let per_band = (d_minor + d_major / n as f32).max(ROOT_EPSILON);

        let mut sum = 0.0;
        for i in 0..n {
            let s = (i as f32 + 0.5) / n as f32 - 0.5;
            sum += band_coverage(
                self.signed_distance(point + s * major, edge_width_sq),
                per_band,
            );
        }
        sum / n as f32
    }
}

fn band_coverage(sd: f32, band: f32) -> f32 { (0.5 - sd / band).clamp(0.0, 1.0) }

fn curve_distance_sq(point: Vec2, curve: CurveRecord) -> f32 {
    exact_quadratic_distance_sq(
        curve,
        point,
        xy(curve.start_delta),
        zw(curve.start_delta),
        xy(curve.curve_end),
        zw(curve.curve_end),
    )
}

fn curve_bounds_distance_sq(point: Vec2, curve: CurveRecord) -> f32 {
    let nearest = point.clamp(xy(curve.bounds), zw(curve.bounds));
    (point - nearest).length_squared()
}

fn point_line_distance_sq(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let edge = end - start;
    let len_sq = edge.length_squared().max(ROOT_EPSILON);
    let t = ((point - start).dot(edge) / len_sq).clamp(0.0, 1.0);
    (point - (start + edge * t)).length_squared()
}

fn exact_quadratic_distance_sq(
    curve: CurveRecord,
    point: Vec2,
    start: Vec2,
    control_delta: Vec2,
    curve_delta: Vec2,
    end: Vec2,
) -> f32 {
    let pv = point - start;
    let mut best_sq = pv.length_squared();
    best_sq = best_sq.min((end - point).length_squared());

    let inv = curve.solver.z;
    if inv > 0.0 {
        let roots = solve_cubic_normed(
            curve.solver.x,
            curve.solver.y - curve_delta.dot(pv) * inv,
            -control_delta.dot(pv) * inv,
        );
        for &t in &roots {
            if (0.0..=1.0).contains(&t) {
                let closest = start + control_delta * (2.0 * t) + curve_delta * (t * t);
                best_sq = best_sq.min((closest - point).length_squared());
            }
        }
        best_sq
    } else {
        best_sq.min(point_line_distance_sq(point, start, end))
    }
}

fn cbrt_signed(x: f32) -> f32 {
    if x < 0.0 {
        -(-x).powf(1.0 / 3.0)
    } else {
        x.powf(1.0 / 3.0)
    }
}

/// Returns 1 or 3 valid roots; unused slots are NaN (filtered by the [0,1] gate).
/// Solves the monic cubic `t³ + quadratic·t² + linear·t + constant = 0`.
fn solve_cubic_normed(quadratic: f32, linear: f32, constant: f32) -> [f32; 3] {
    let quadratic_sq = quadratic * quadratic;
    let quad_reduced = (1.0 / 9.0) * (quadratic_sq - 3.0 * linear);
    let cubic_reduced =
        (1.0 / 54.0) * (quadratic * (2.0 * quadratic_sq - 9.0 * linear) + 27.0 * constant);
    let cubic_reduced_sq = cubic_reduced * cubic_reduced;
    let quad_reduced_cubed = quad_reduced * quad_reduced * quad_reduced;
    let root_shift = quadratic * (1.0 / 3.0);
    if cubic_reduced_sq < quad_reduced_cubed {
        let cos_argument = (cubic_reduced / quad_reduced_cubed.sqrt()).clamp(-1.0, 1.0);
        let theta = cos_argument.acos();
        let trig_scale = -2.0 * quad_reduced.sqrt();
        let cos_t3 = (theta / 3.0).cos();
        let sin_t3 = (theta / 3.0).sin();
        [
            trig_scale * cos_t3 - root_shift,
            trig_scale * (-0.5 * cos_t3 - SQRT_3_OVER_2 * sin_t3) - root_shift,
            trig_scale * (-0.5 * cos_t3 + SQRT_3_OVER_2 * sin_t3) - root_shift,
        ]
    } else {
        let sign = if cubic_reduced < 0.0 { -1.0 } else { 1.0 };
        let cube_root = sign
            * cbrt_signed(cubic_reduced.abs() + (cubic_reduced_sq - quad_reduced_cubed).sqrt());
        let paired_root = if cube_root == 0.0 {
            0.0
        } else {
            quad_reduced / cube_root
        };
        [(cube_root + paired_root) - root_shift, f32::NAN, f32::NAN]
    }
}

fn winding_for_t(curve: CurveRecord, point: Vec2, t: f32) -> i32 {
    if !(0.0..1.0).contains(&t) {
        return 0;
    }
    let curve_x = curve.start_delta.x + 2.0 * curve.start_delta.z * t + curve.curve_end.x * t * t;
    if curve_x <= point.x {
        return 0;
    }
    let dy = 2.0 * (curve.start_delta.w + curve.curve_end.y * t);
    if dy.abs() < ROOT_EPSILON {
        return 0;
    }
    if dy > 0.0 { 1 } else { -1 }
}

fn curve_winding(curve: CurveRecord, point: Vec2) -> i32 {
    let a = curve.curve_end.y;
    let b = 2.0 * curve.start_delta.w;
    let c = curve.start_delta.y - point.y;
    if a.abs() < ROOT_EPSILON {
        if b.abs() < ROOT_EPSILON {
            return 0;
        }
        return winding_for_t(curve, point, -c / b);
    }
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return 0;
    }
    let root = discriminant.sqrt();
    winding_for_t(curve, point, (-b - root) / (2.0 * a))
        + winding_for_t(curve, point, (-b + root) / (2.0 * a))
}

// ---- ground truth (independent of bands; full segment scan) ----------------

struct GroundTruth {
    segments: Vec<QuadraticSegment>,
}

impl GroundTruth {
    /// Global non-zero winding via the same crossing rule, over all segments.
    fn inside(&self, point: Vec2) -> bool {
        let mut winding = 0;
        for segment in &self.segments {
            winding += curve_winding(CurveRecord::from(segment), point);
        }
        winding != 0
    }

    /// True signed distance + outward unit normal at the nearest point.
    /// Synthetic glyph is all line segments, so nearest-point is exact here.
    fn signed_distance_and_normal(&self, point: Vec2) -> (f32, Vec2) {
        let mut best_sq = f32::MAX;
        let mut nearest = point;
        for segment in &self.segments {
            let edge = segment.end - segment.start;
            let len_sq = edge.length_squared().max(ROOT_EPSILON);
            let t = ((point - segment.start).dot(edge) / len_sq).clamp(0.0, 1.0);
            let foot = segment.start + edge * t;
            let d_sq = (point - foot).length_squared();
            if d_sq < best_sq {
                best_sq = d_sq;
                nearest = foot;
            }
        }
        let distance = best_sq.sqrt();
        let inside = self.inside(point);
        let outward = point - nearest;
        let normal = if outward.length() > ROOT_EPSILON {
            outward.normalize()
        } else {
            Vec2::new(1.0, 0.0)
        };
        (if inside { -distance } else { distance }, normal)
    }

    /// Reference coverage: fraction of the footprint parallelogram inside the
    /// fill, by stratified supersampling. `point + s*dx + t*dy`, s,t in [-.5,.5].
    fn coverage(&self, point: Vec2, dx: Vec2, dy: Vec2, n: u32) -> f32 {
        let mut inside = 0u32;
        for i in 0..n {
            for j in 0..n {
                let s = (i as f32 + 0.5) / n as f32 - 0.5;
                let t = (j as f32 + 0.5) / n as f32 - 0.5;
                if self.inside(point + s * dx + t * dy) {
                    inside += 1;
                }
            }
        }
        inside as f32 / (n * n) as f32
    }

    fn signed_distance(&self, point: Vec2) -> f32 { self.signed_distance_and_normal(point).0 }

    /// Footprint extent along the silhouette normal: the band a straight-edge
    /// model SHOULD use (L1 projection of the two footprint basis vectors).
    fn exact_band(&self, point: Vec2, dx: Vec2, dy: Vec2) -> f32 {
        let (_, normal) = self.signed_distance_and_normal(point);
        (normal.dot(dx)).abs() + (normal.dot(dy)).abs()
    }
}

// ---- synthetic glyph -------------------------------------------------------

fn line(start: Vec2, end: Vec2) -> QuadraticSegment {
    QuadraticSegment {
        start,
        control: start.midpoint(end),
        end,
    }
}

/// A sharp upward triangle: one acute convex corner at the apex.
fn sharp_corner_glyph() -> (Glyph, Vec<QuadraticSegment>) {
    let apex = Vec2::new(500.0, 700.0);
    let right = Vec2::new(580.0, 0.0);
    let left = Vec2::new(420.0, 0.0);
    let segments = vec![line(apex, right), line(right, left), line(left, apex)];
    let glyph = Glyph {
        character: '^',
        id:        0,
        bounds:    Bounds {
            min: Vec2::new(420.0, 0.0),
            max: Vec2::new(580.0, 700.0),
        },
        contours:  vec![Contour {
            segments: segments.clone(),
        }],
    };
    (glyph, segments)
}

/// Anisotropic grazing footprint: long axis at `angle_deg`, ratio `aniso`,
/// base scale `px` design-units per head-on screen pixel.
fn footprint(angle_deg: f32, aniso: f32, px: f32) -> (Vec2, Vec2) {
    let a = angle_deg.to_radians();
    let long = aniso * px * Vec2::new(a.cos(), a.sin());
    let short = px * Vec2::new(-a.sin(), a.cos());
    (long, short)
}

// ---- test harness ----------------------------------------------------------

fn build() -> (Probe, GroundTruth) {
    let (glyph, segments) = sharp_corner_glyph();
    let packed = build_packed_glyph(glyph, DEFAULT_BAND_COUNT);
    let band_count = (packed.bands().len() / 2) as u32;
    let probe = Probe {
        record: GlyphRecord::new(packed.bounds(), 0, band_count, band_count, band_count),
        curves: packed.curves().to_vec(),
        bands:  packed.bands().to_vec(),
    };
    let gt = GroundTruth { segments };
    (probe, gt)
}

struct CornerSample {
    gt:          f32,
    single:      f32,
    super4:      f32,
    fix_b:       f32,
    model_cause: f32,
}

/// Scans the glyph bounds (plus footprint margin) and returns the metrics at the
/// pixel where the single-sample band over-covers ground truth the most — the
/// corner where the wing forms.
fn worst_overcoverage(probe: &Probe, gt: &GroundTruth, dx: Vec2, dy: Vec2) -> CornerSample {
    let bmin = probe.bounds_min();
    let bmax = bmin + probe.bounds_size();
    let margin = 2.0 * (dx.length() + dy.length());
    let x0 = bmin.x - margin;
    let x1 = bmax.x + margin;
    let y0 = bmin.y - margin;
    let y1 = bmax.y + margin;
    let cols = 80usize;
    let rows = 40usize;

    let mut best = CornerSample {
        gt:          0.0,
        single:      0.0,
        super4:      0.0,
        fix_b:       0.0,
        model_cause: 0.0,
    };
    let mut max_over = f32::MIN;
    for row in 0..rows {
        let y = y1 + (y0 - y1) * (row as f32 + 0.5) / rows as f32;
        for col in 0..cols {
            let x = x0 + (x1 - x0) * (col as f32 + 0.5) / cols as f32;
            let point = Vec2::new(x, y);
            let reference = gt.coverage(point, dx, dy, GT_SAMPLES);
            let (single, super4) = probe.aa_band_coverage(point, dx, dy);
            if single - reference > max_over {
                max_over = single - reference;
                let sd_true = gt.signed_distance(point);
                let band_true = gt.exact_band(point, dx, dy);
                best = CornerSample {
                    gt: reference,
                    single,
                    super4,
                    fix_b: probe.aniso_shader_fix(point, dx, dy, MAX_ANISO_SAMPLES),
                    model_cause: band_coverage(sd_true, band_true),
                };
            }
        }
    }
    best
}

/// A single coverage sample over-covers the sharp convex corner under a grazing
/// footprint, and the over-coverage is inherent to the single-sample model: it
/// persists even when fed exact distance and exact band.
#[test]
fn single_sample_overcovers_convex_corner() {
    let (probe, gt) = build();
    for angle in GRAZING_ANGLES {
        let (dx, dy) = footprint(angle, ANISO, PX);
        let s = worst_overcoverage(&probe, &gt, dx, dy);
        assert!(
            s.single - s.gt >= WING_MIN_OVERCOVERAGE,
            "angle {angle}: single-sample over-coverage {:.3} (single {:.3}, truth {:.3}) below {WING_MIN_OVERCOVERAGE}",
            s.single - s.gt,
            s.single,
            s.gt,
        );
        assert!(
            s.model_cause - s.gt >= MODEL_CAUSE_MIN_OVERCOVERAGE,
            "angle {angle}: even exact distance+band over-covers by {:.3}; the single-sample model is the cause",
            s.model_cause - s.gt,
        );
    }
}

/// The anisotropic stride brings corner coverage back to ground truth, and beats
/// a fixed 4-tap supersample — the data behind striding along the foreshortened
/// axis rather than sampling a fixed sub-pixel grid.
#[test]
fn anisotropic_stride_beats_supersample_at_corner() {
    let (probe, gt) = build();
    for angle in GRAZING_ANGLES {
        let (dx, dy) = footprint(angle, ANISO, PX);
        let s = worst_overcoverage(&probe, &gt, dx, dy);
        let fix_error = (s.fix_b - s.gt).abs();
        assert!(
            fix_error <= FIX_MAX_ERROR,
            "angle {angle}: stride fix off ground truth by {fix_error:.3} (fix {:.3}, truth {:.3})",
            s.fix_b,
            s.gt,
        );
        let super4_over = s.super4 - s.gt;
        assert!(
            super4_over >= SUPER4_MIN_OVERCOVERAGE,
            "angle {angle}: fixed 4-tap supersample over-covers only {super4_over:.3}; expected it to stay well over ground truth at the corner",
        );
        assert!(
            super4_over - fix_error >= FIX_IMPROVEMENT_OVER_SUPER4,
            "angle {angle}: stride fix should beat 4-tap by >= {FIX_IMPROVEMENT_OVER_SUPER4} (4-tap over {super4_over:.3}, fix error {fix_error:.3})",
        );
    }
}

/// The stride tracks ground truth across a straight edge — it does not steepen
/// the ramp into aliasing the way a naive `full_band / N` stride would.
#[test]
fn stride_does_not_alias_straight_edge() {
    let (probe, gt) = build();
    // Horizontal grazing across the triangle's base edge; the monotonic ramp
    // region only (the band-boundary row has a known signed-distance reach spike).
    let (dx, dy) = footprint(0.0, 4.0, PX);
    let x = 500.0;
    for k in 0..=12 {
        let y = -7.5 + 12.5 * (k as f32) / 12.0;
        let point = Vec2::new(x, y);
        let reference = gt.coverage(point, dx, dy, GT_SAMPLES);
        let fix = probe.aniso_shader_fix(point, dx, dy, MAX_ANISO_SAMPLES);
        assert!(
            (fix - reference).abs() <= EDGE_FIX_MAX_ERROR,
            "y {y:.1}: stride fix {fix:.3} should track straight-edge ground truth {reference:.3} within {EDGE_FIX_MAX_ERROR}",
        );
    }
}

/// Tripwire: hashes `slug_text.wgsl` and fails when it changes. The [`Probe`]
/// above mirrors the shader's coverage math by hand; this flags that the shader
/// moved so the mirror gets re-checked. It cannot tell whether the change was
/// correct — the shader runs on the GPU. On failure, re-verify [`Probe`], then
/// set `EXPECTED_SHADER_FNV1A` to the printed value.
#[test]
fn shader_mirror_matches_wgsl() {
    const SHADER: &str = include_str!("../shaders/slug_text.wgsl");
    const EXPECTED_SHADER_FNV1A: u64 = 0x2def_34cc_71a4_572f;
    let actual = fnv1a_64(SHADER.as_bytes());
    assert_eq!(
        actual, EXPECTED_SHADER_FNV1A,
        "shader has changed, make sure to update this test to match the new logic (new hash {actual:#018x})",
    );
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
