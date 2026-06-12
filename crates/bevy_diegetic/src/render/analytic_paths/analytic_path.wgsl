// Analytic path fragment shader: exact coverage for quadratic-Bezier
// outlines, shared by slug text runs and merged panel-line paths. A
// "glyph" here is one packed path — a font glyph or a whole merge group of
// panel lines (ticks + spine in one record).
//
// Inputs (packed by render/analytic_paths/packing.rs, structs mirrored
// below — coverage_probe.rs hash-pins this file against its CPU mirror):
//   curves  — CurveRecord quadratic segments with precomputed
//             distance-solver coefficients, per-contour hairline stroke
//             width (solver.w), and per-contour fade exponent.
//   bands   — BandRecord (start, count) windows into a band-ordered curve
//             table. Each glyph has horizontal bands (y-slabs: every curve
//             a +x ray from a point in the slab can cross) and vertical
//             bands (x-slabs), so winding and nearest-distance scans touch
//             a handful of curves, not the whole path.
//   glyphs  — GlyphRecord per packed path: design-space bounds, band table
//             ranges, narrowest dilating stroke.
//   uv_a    — fractional position inside the glyph bounds quad.
//   uv_b    — (glyph index, run index) as floats, constant per quad.
//   per-run — fill color / render mode / OIT offset / AA flags from the
//             RunRecord table under GLYPH_VERTEX_PULL, else from the
//             material's TextUniform.
//
// Evaluation: non-zero winding (inside/outside) + distance to the nearest
// curve (the AA ramp), both banded. Strokes thinner than
// TextUniform.hairline_min_px on screen are dilated per curve to that
// floor and faded back by each contour's fade exponent; fading and exempt
// contours coexist in one path via the two-lane CoverageTerms evaluation.
// Coverage then feeds PBR lighting and (when enabled) OIT.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::VertexOutput
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

#ifdef OIT_ENABLED
#import bevy_core_pipeline::oit::oit_draw
#import bevy_pbr::pbr_types::{
    STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS,
    STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE,
}

// Floor for the offset fragment depth handed to oit_draw.
// pack_24bit_depth_8bit_alpha saturates depth to [0, 1] and bevy's OIT
// resolve compares packed (depth << 8 | alpha) values against the cleared
// background (depth 0, alpha 1): a fragment whose offset z reaches 0 packs
// below the background whenever its alpha < 1.0 and is silently dropped.
// ~3 quanta of the 24-bit depth packing keeps the fragment storable; an
// out-of-calibration offset then degrades to wrong ordering, not
// invisibility. Offset magnitudes are calibrated against the focus depth
// in OIT_DEPTH_STEP (render/constants.rs).
const OIT_MIN_DEPTH: f32 = 2e-7;
#endif

const ROOT_EPSILON: f32 = 0.00001;
const DEGENERATE_EPS: f32 = 0.00000001;
const SQRT_3_OVER_2: f32 = 0.8660254037844386;
const DISCARD_ALPHA: f32 = 0.02;
const EDGE_FILTER_WIDTH: f32 = 1.2;
const MAX_ANISO_SAMPLES: f32 = 16.0;
const RENDER_MODE_TEXT: u32 = 1u;
const RENDER_MODE_PUNCH_OUT: u32 = 2u;
// Mirrors AA_FLAG_SUPERSAMPLE / AA_FLAG_BAND in render/mod.rs — the
// RunRecord.aa_flags bit encoding written by AntiAlias::aa_flags().
const AA_FLAG_SUPERSAMPLE: u32 = 1u;
const AA_FLAG_BAND: u32 = 2u;

struct TextUniform {
    fill_color: vec4<f32>,
    render_mode: u32,
    oit_depth_offset: f32,
    supersample: u32,
    aa_band: u32,
    // Minimum on-screen stroke width in device pixels for hairline-dilated
    // paths. Synced from the HairlineWidth resource (logical px × window
    // scale factor, floored): a stroke dilated to under ~1.5px renders as
    // either one solid column or two half-bright columns depending on pixel
    // phase, so a near-vertical line stairsteps a full column per crossover.
    hairline_min_px: f32,
}

// One quadratic Bezier segment, mirroring `CurveRecord` in packing.rs. The
// segment is evaluated as B(t) = start + 2t·control_delta + t²·curve_delta,
// so winding and distance never need the raw control point.
struct CurveRecord {
    // Segment start point in .xy; control-minus-start in .zw.
    start_delta: vec4<f32>,
    // Quadratic second difference (end − 2·control + start) in .xy — zero
    // for segments the packer snapped to exactly linear; segment end point
    // in .zw.
    curve_end: vec4<f32>,
    // Control-point AABB, min in .xy / max in .zw — conservative cull for
    // the distance scan.
    bounds: vec4<f32>,
    // .xyz: precomputed closest-point cubic coefficients — .x and .y are the
    // point-independent terms of the normalized cubic
    // exact_quadratic_distance_sq solves, .z is 1/|curve_delta|² (0 routes
    // the segment through the exact linear solve). .w: the owning contour's
    // narrowest stroke in design units (per-curve hairline dilation), 0.0
    // for undilated contours (text glyphs).
    solver: vec4<f32>,
    // Owning contour's resolved hairline fade exponent; each coverage
    // evaluation fades by the winning (nearest) curve's exponent, so one
    // merged path can mix fading and non-fading contours. 0.0 disables fade
    // for this curve.
    fade_exponent: f32,
}

// One band's window into the band-ordered curve table: a horizontal band
// holds every curve whose y-extent overlaps that y-slab of the glyph (the
// complete crossing set for a +x winding ray from inside the slab), a
// vertical band the same by x. Band lookup derives the index from the
// point's normalized coordinate; y_min/y_max are the band edges in design
// units.
struct BandRecord {
    start: u32,
    count: u32,
    y_min: f32,
    y_max: f32,
}

// One packed path (font glyph or merged panel-line group).
struct GlyphRecord {
    // Design-space bounds: min in .xy, size in .zw. uv_a maps onto this
    // rectangle (see design_position).
    bounds_min_size: vec4<f32>,
    // Band-table ranges: horizontal band start/count in .xy, vertical band
    // start/count in .zw.
    band_range: vec4<u32>,
    // Narrowest dilating stroke across the path's contours, in design units;
    // > 0 enables hairline dilation and sizes the distance scan for the
    // largest dilation in the path. 0 for text. Per-contour widths arrive in
    // each curve's solver.w.
    min_feature: f32,
}

struct LaneTerms {
    winding: i32,
    // Min over this lane's scanned curves of (distance - that curve's
    // dilation): the distance to the lane's nearest per-contour-dilated
    // silhouette.
    adjusted: f32,
    // Dilation of the curve that won `adjusted`.
    dilation: f32,
}

// Two-lane coverage accumulator. Curves split by fade policy: the exempt
// lane holds fade_exponent == 0 curves (never-fading contours, and every
// text glyph), the faded lane holds fade_exponent > 0 curves. Contours are
// wholly one lane, so each lane's winding is a valid winding number of its
// own sub-geometry, and the lanes' windings sum to the whole path's. Final
// alpha takes mix(exempt_coverage, union_coverage, fade_factor): at fade
// factor 1 the path renders exactly as an unfaded single-winding union (an
// exempt/faded abutment is union-interior, so no junction line), at factor 0
// only the exempt sub-geometry remains, and a never-fading ruler spine keeps
// full alpha (mix(1, 1, f) = 1) even where a thinner (more-dilated) fading
// tick's curves are nearer in adjusted-distance terms.
struct CoverageTerms {
    exempt: LaneTerms,
    faded: LaneTerms,
    // Fade exponent of the faded lane's winning curve.
    fade_exponent: f32,
}

fn empty_coverage_terms() -> CoverageTerms {
    return CoverageTerms(
        LaneTerms(0, 1000000.0, 0.0),
        LaneTerms(0, 1000000.0, 0.0),
        0.0,
    );
}

// The whole path's terms, rebuilt from the two lanes: windings add (the
// lanes partition the path's contours), and the nearest-silhouette race is
// the cross-lane min with the winner's dilation.
fn union_lane(terms: CoverageTerms) -> LaneTerms {
    let faded_wins = terms.faded.adjusted < terms.exempt.adjusted;
    return LaneTerms(
        terms.exempt.winding + terms.faded.winding,
        min(terms.exempt.adjusted, terms.faded.adjusted),
        select(terms.exempt.dilation, terms.faded.dilation, faded_wins),
    );
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> uniforms: TextUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<storage, read> curves: array<CurveRecord>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<storage, read> bands: array<BandRecord>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var<storage, read> glyphs: array<GlyphRecord>;

#ifdef GLYPH_VERTEX_PULL
// Mirrors `RunRecord` in `glyph/packing.rs` (std430, 96 B stride). Under the
// vertex-pulling route a batch holds many runs, so the per-run values move
// out of the material uniform into this table, indexed by the run index the
// vertex stage forwards in `uv_b.y`.
struct RunRecord {
    transform: mat4x4<f32>,
    fill_color: vec4<f32>,
    render_mode: u32,
    depth_nudge: f32,
    oit_depth_offset: f32,
    aa_flags: u32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(105) var<storage, read> run_records: array<RunRecord>;

// The index arrives in an interpolated varying that is constant across the
// quad, but perspective-corrected interpolation can land a hair below the
// integer (478.9999 for 479.0) on long sliver quads, so recovery must round —
// a floor() here reads the previous record and the quad renders another
// path's coverage.
fn run_index(glyph_uv: vec2<f32>) -> u32 {
    return u32(glyph_uv.y + 0.5);
}
#endif

// uv_b.x carries the glyph table index; same rounding recovery as
// run_index above.
fn glyph_index(glyph_uv: vec2<f32>) -> u32 {
    return u32(glyph_uv.x + 0.5);
}

// Per-run fill color: the run table under vertex pulling, the material
// uniform on the per-run path.
fn run_fill_color(glyph_uv: vec2<f32>) -> vec4<f32> {
#ifdef GLYPH_VERTEX_PULL
    return run_records[run_index(glyph_uv)].fill_color;
#else
    return uniforms.fill_color;
#endif
}

// Per-run render mode (Text / PunchOut), sourced like `run_fill_color`.
fn run_render_mode(glyph_uv: vec2<f32>) -> u32 {
#ifdef GLYPH_VERTEX_PULL
    return run_records[run_index(glyph_uv)].render_mode;
#else
    return uniforms.render_mode;
#endif
}

// Per-run OIT z offset: the run table under vertex pulling, the material
// uniform on the per-run path.
fn run_oit_depth_offset(glyph_uv: vec2<f32>) -> f32 {
#ifdef GLYPH_VERTEX_PULL
    return run_records[run_index(glyph_uv)].oit_depth_offset;
#else
    return uniforms.oit_depth_offset;
#endif
}

// Per-run anti-alias mode bits (AA_FLAG_SUPERSAMPLE | AA_FLAG_BAND), sourced
// like `run_fill_color`.
fn run_aa_flags(glyph_uv: vec2<f32>) -> u32 {
#ifdef GLYPH_VERTEX_PULL
    return run_records[run_index(glyph_uv)].aa_flags;
#else
    var flags = 0u;
    if uniforms.supersample != 0u {
        flags |= AA_FLAG_SUPERSAMPLE;
    }
    if uniforms.aa_band != 0u {
        flags |= AA_FLAG_BAND;
    }
    return flags;
#endif
}

fn glyph_bounds_min(glyph: GlyphRecord) -> vec2<f32> {
    return glyph.bounds_min_size.xy;
}

fn glyph_bounds_size(glyph: GlyphRecord) -> vec2<f32> {
    return glyph.bounds_min_size.zw;
}

// uv_a → design-space point inside the glyph bounds. v flips: uv origin is
// top-left, design space is y-up.
fn design_position(uv: vec2<f32>, glyph: GlyphRecord) -> vec2<f32> {
    let bounds_min = glyph_bounds_min(glyph);
    let bounds_size = glyph_bounds_size(glyph);
    return bounds_min + vec2<f32>(
        uv.x * bounds_size.x,
        (1.0 - uv.y) * bounds_size.y,
    );
}

fn horizontal_band_index(point: vec2<f32>, glyph: GlyphRecord) -> u32 {
    let bounds_min = glyph_bounds_min(glyph);
    let bounds_size = glyph_bounds_size(glyph);
    let band_count = glyph.band_range.y;
    let normalized_y = clamp(
        (point.y - bounds_min.y) / max(bounds_size.y, ROOT_EPSILON),
        0.0,
        0.999999,
    );
    return min(u32(normalized_y * f32(band_count)), band_count - 1u);
}

fn vertical_band_index(point: vec2<f32>, glyph: GlyphRecord) -> u32 {
    let bounds_min = glyph_bounds_min(glyph);
    let bounds_size = glyph_bounds_size(glyph);
    let band_count = glyph.band_range.w;
    let normalized_x = clamp(
        (point.x - bounds_min.x) / max(bounds_size.x, ROOT_EPSILON),
        0.0,
        0.999999,
    );
    return min(u32(normalized_x * f32(band_count)), band_count - 1u);
}

// Sign-preserving cube root for the depressed-cubic solver below.
fn cbrt_signed(x: f32) -> f32 {
    if x < 0.0 {
        return -pow(-x, 1.0 / 3.0);
    }
    return pow(x, 1.0 / 3.0);
}

// Real roots of t³ + a·t² + b·t + c (trigonometric branch for three roots,
// Cardano for one). Writes them to `roots`, returns the count.
fn solve_cubic_normed(a: f32, b: f32, c: f32, roots: ptr<function, array<f32, 3>>) -> u32 {
    let a2 = a * a;
    let q = (1.0 / 9.0) * (a2 - 3.0 * b);
    let r = (1.0 / 54.0) * (a * (2.0 * a2 - 9.0 * b) + 27.0 * c);
    let r2 = r * r;
    let q3 = q * q * q;
    let a_third = a * (1.0 / 3.0);
    if r2 < q3 {
        let t_norm = clamp(r / sqrt(q3), -1.0, 1.0);
        let theta = acos(t_norm);
        let q_pre = -2.0 * sqrt(q);
        let cos_t3 = cos(theta / 3.0);
        let sin_t3 = sin(theta / 3.0);
        (*roots)[0] = q_pre * cos_t3 - a_third;
        (*roots)[1] = q_pre * (-0.5 * cos_t3 - SQRT_3_OVER_2 * sin_t3) - a_third;
        (*roots)[2] = q_pre * (-0.5 * cos_t3 + SQRT_3_OVER_2 * sin_t3) - a_third;
        return 3u;
    }
    let sgn = select(-1.0, 1.0, r < 0.0);
    let u = sgn * cbrt_signed(abs(r) + sqrt(r2 - q3));
    let v = select(q / u, 0.0, u == 0.0);
    (*roots)[0] = (u + v) - a_third;
    return 1u;
}

// Squared distance from `point` to the segment. The on-curve closest-point
// parameters are the roots of the cubic (B(t) − point)·B'(t) = 0; its two
// point-independent coefficients and the 1/|curve_delta|² normalizer arrive
// precomputed in curve.solver.xyz, the point-dependent terms are filled in
// here. Endpoints compete with the in-range (t ∈ [0, 1]) roots; degenerate
// curvature (solver.z == 0) falls back to the exact point–segment distance.
fn exact_quadratic_distance_sq(
    curve: CurveRecord,
    point: vec2<f32>,
    start: vec2<f32>,
    control_delta: vec2<f32>,
    curve_delta: vec2<f32>,
    end: vec2<f32>,
) -> f32 {
    let pv = point - start;
    var best_sq = dot(pv, pv);

    let end_diff = end - point;
    best_sq = min(best_sq, dot(end_diff, end_diff));

    let inverse_curve_norm_sq = curve.solver.z;
    if inverse_curve_norm_sq > 0.0 {
        var roots: array<f32, 3>;
        let root_count = solve_cubic_normed(
            curve.solver.x,
            curve.solver.y - dot(curve_delta, pv) * inverse_curve_norm_sq,
            -dot(control_delta, pv) * inverse_curve_norm_sq,
            &roots,
        );
        for (var index = 0u; index < root_count; index += 1u) {
            let t = roots[index];
            if t >= 0.0 && t <= 1.0 {
                let closest = start + control_delta * (2.0 * t) + curve_delta * (t * t);
                let diff = closest - point;
                best_sq = min(best_sq, dot(diff, diff));
            }
        }
    } else {
        return min(best_sq, point_line_distance_sq(point, start, end));
    }

    return best_sq;
}

fn winding_for_t(curve: CurveRecord, point: vec2<f32>, t: f32) -> i32 {
    let dy = 2.0 * (curve.start_delta.w + curve.curve_end.y * t);
    if abs(dy) < ROOT_EPSILON {
        return 0;
    }
    // Half-open in y, not t: an upward crossing counts on t ∈ [0, 1), a
    // downward crossing on t ∈ (0, 1], so each segment's y-interval includes
    // its lower endpoint and excludes its upper one. A ray exactly through a
    // join (e.g. a rectangle cap's corner, where the horizontal edge
    // contributes nothing) then sees both adjoining segments agree — counting
    // half-open in t lets one cap count its t=0 corner while the other
    // excludes its t=1 corner, flipping a whole row of fragments inside.
    let upward = dy > 0.0;
    if upward && (t < 0.0 || t >= 1.0) {
        return 0;
    }
    if !upward && (t <= 0.0 || t > 1.0) {
        return 0;
    }

    let curve_x = curve.start_delta.x +
        2.0 * curve.start_delta.z * t +
        curve.curve_end.x * t * t;
    if curve_x <= point.x {
        return 0;
    }
    return select(-1, 1, upward);
}

// Signed crossing count this segment contributes to a +x ray from `point`:
// solve the quadratic y(t) = point.y, score each in-range root by crossing
// direction (winding_for_t). Sums to the non-zero winding number over a
// whole contour.
fn curve_winding(curve: CurveRecord, point: vec2<f32>) -> i32 {
    let a = curve.curve_end.y;
    let b = 2.0 * curve.start_delta.w;
    let c = curve.start_delta.y - point.y;

    if abs(a) < ROOT_EPSILON {
        if abs(b) < ROOT_EPSILON {
            return 0;
        }
        return winding_for_t(curve, point, -c / b);
    }

    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return 0;
    }

    let root = sqrt(discriminant);
    return winding_for_t(curve, point, (-b - root) / (2.0 * a)) +
        winding_for_t(curve, point, (-b + root) / (2.0 * a));
}

fn outside_glyph_bounds(point: vec2<f32>, glyph: GlyphRecord) -> bool {
    let bounds_min = glyph_bounds_min(glyph);
    let bounds_max = bounds_min + glyph_bounds_size(glyph);
    return point.x < bounds_min.x ||
        point.x > bounds_max.x ||
        point.y < bounds_min.y ||
        point.y > bounds_max.y;
}

// Hairline fade factor for one coverage evaluation: the alpha scale that
// makes a dilated sub-floor stroke fade toward its natural coverage instead
// of rendering at full alpha. Both inputs come from the evaluation's winning
// curve — its dilation (natural = hairline_target − 2 × dilation) and its
// contour's fade_exponent — so contours with different fade policies coexist
// in one merged path. dilation 0 (at-floor strokes, and text glyphs whose
// curves carry solver.w = 0) gives factor 1; fade_exponent 0 disables (Full
// policy).
fn hairline_fade_factor(dilation: f32, hairline_target: f32, fade_exponent: f32) -> f32 {
    if fade_exponent <= 0.0 || dilation <= 0.0 || hairline_target <= 0.0 {
        return 1.0;
    }
    let natural = max(hairline_target - 2.0 * dilation, 0.0);
    return pow(natural / hairline_target, fade_exponent);
}

// Per-curve hairline dilation. hairline_target is the minimum on-screen
// stroke width converted to design units (pixel_design_units ×
// uniforms.hairline_min_px); a curve whose contour stroke (solver.w) falls
// below it dilates by half the deficit. 0 for undilated contours.
fn curve_dilation(curve: CurveRecord, hairline_target: f32) -> f32 {
    if curve.solver.w <= 0.0 {
        return 0.0;
    }
    return max(0.0, (hairline_target - curve.solver.w) * 0.5);
}

// Run one curve in the nearest-silhouette race for its lane: adjusted =
// distance − that curve's dilation, so a more-dilated (thinner-stroke)
// curve's silhouette wins at the same raw distance. The faded lane also
// records the winner's fade exponent.
fn accumulate_nearest(
    terms: ptr<function, CoverageTerms>,
    point: vec2<f32>,
    curve: CurveRecord,
    hairline_target: f32,
) {
    let dilation = curve_dilation(curve, hairline_target);
    let adjusted = sqrt(curve_distance_sq(point, curve)) - dilation;
    if curve.fade_exponent > 0.0 {
        if adjusted < (*terms).faded.adjusted {
            (*terms).faded.adjusted = adjusted;
            (*terms).faded.dilation = dilation;
            (*terms).fade_exponent = curve.fade_exponent;
        }
    } else if adjusted < (*terms).exempt.adjusted {
        (*terms).exempt.adjusted = adjusted;
        (*terms).exempt.dilation = dilation;
    }
}

// One pass over the point's horizontal band: per-lane winding (the y-slab
// band holds every curve a +x ray can cross, so the winding is complete
// after this pass) plus nearest-distance candidates for curves whose AABB
// is within scan_width.
fn horizontal_coverage_terms(
    point: vec2<f32>,
    scan_width_sq: f32,
    hairline_target: f32,
    glyph: GlyphRecord,
) -> CoverageTerms {
    let include_winding = !outside_glyph_bounds(point, glyph);
    let horizontal_band = bands[glyph.band_range.x + horizontal_band_index(point, glyph)];
    var terms = empty_coverage_terms();
    for (var offset = 0u; offset < horizontal_band.count; offset += 1u) {
        let curve = curves[horizontal_band.start + offset];
        if include_winding {
            let winding = curve_winding(curve, point);
            if curve.fade_exponent > 0.0 {
                terms.faded.winding += winding;
            } else {
                terms.exempt.winding += winding;
            }
        }
        if curve_bounds_distance_sq(point, curve) <= scan_width_sq {
            accumulate_nearest(&terms, point, curve, hairline_target);
        }
    }
    return terms;
}

fn point_line_distance_sq(point: vec2<f32>, start: vec2<f32>, end: vec2<f32>) -> f32 {
    let edge = end - start;
    let edge_length_squared = max(dot(edge, edge), ROOT_EPSILON);
    let t = clamp(dot(point - start, edge) / edge_length_squared, 0.0, 1.0);
    let diff = point - (start + edge * t);
    return dot(diff, diff);
}

fn curve_distance_sq(point: vec2<f32>, curve: CurveRecord) -> f32 {
    return exact_quadratic_distance_sq(
        curve,
        point,
        curve.start_delta.xy,
        curve.start_delta.zw,
        curve.curve_end.xy,
        curve.curve_end.zw,
    );
}

fn curve_bounds_distance_sq(point: vec2<f32>, curve: CurveRecord) -> f32 {
    let nearest = clamp(point, curve.bounds.xy, curve.bounds.zw);
    let diff = point - nearest;
    return dot(diff, diff);
}

// Distance-only second pass over the point's vertical (x-slab) band: a
// curve above or below the point can sit outside its horizontal band yet
// inside the AA scan radius. Winding is already complete from the
// horizontal pass.
fn nearest_vertical_curve(
    point: vec2<f32>,
    scan_width_sq: f32,
    hairline_target: f32,
    glyph: GlyphRecord,
    initial: CoverageTerms,
) -> CoverageTerms {
    let vertical_band = bands[glyph.band_range.z + vertical_band_index(point, glyph)];
    var terms = initial;
    for (var offset = 0u; offset < vertical_band.count; offset += 1u) {
        let curve = curves[vertical_band.start + offset];
        if curve_bounds_distance_sq(point, curve) <= scan_width_sq {
            accumulate_nearest(&terms, point, curve, hairline_target);
        }
    }
    return terms;
}

// Non-zero winding number at `point` over ALL curves (both lanes), using the
// point's horizontal band. Returns 0 outside the glyph bounds. The prepass
// silhouette test wants the union fill, not a per-lane one.
fn winding_at(point: vec2<f32>, glyph: GlyphRecord) -> i32 {
    if outside_glyph_bounds(point, glyph) {
        return 0;
    }
    let band = bands[glyph.band_range.x + horizontal_band_index(point, glyph)];
    var winding = 0;
    for (var offset = 0u; offset < band.count; offset += 1u) {
        winding += curve_winding(curves[band.start + offset], point);
    }
    return winding;
}

// Per-lane winding number at `point` (x = exempt lane, y = faded lane);
// zero outside the glyph bounds.
fn lane_winding_at(point: vec2<f32>, glyph: GlyphRecord) -> vec2<i32> {
    if outside_glyph_bounds(point, glyph) {
        return vec2<i32>(0, 0);
    }
    let band = bands[glyph.band_range.x + horizontal_band_index(point, glyph)];
    var winding = vec2<i32>(0, 0);
    for (var offset = 0u; offset < band.count; offset += 1u) {
        let curve = curves[band.start + offset];
        let curve_winding_value = curve_winding(curve, point);
        if curve.fade_exponent > 0.0 {
            winding.y += curve_winding_value;
        } else {
            winding.x += curve_winding_value;
        }
    }
    return winding;
}

// Whether any neighbor one filter-width away is outside the fill: x for the
// exempt lane, y for the whole-path union (a lane's winding components sum
// to the union's). A true outer silhouette has at least one outside
// neighbor; an interior edge of a self-intersecting / overlapping glyph
// (e.g. the EB Garamond `g` neck) is filled on both sides, so all neighbors
// stay inside.
fn lanes_any_outside_neighbor(point: vec2<f32>, edge_width: f32, glyph: GlyphRecord) -> vec2<bool> {
    let right = lane_winding_at(point + vec2<f32>(edge_width, 0.0), glyph);
    let left = lane_winding_at(point - vec2<f32>(edge_width, 0.0), glyph);
    let up = lane_winding_at(point + vec2<f32>(0.0, edge_width), glyph);
    let down = lane_winding_at(point - vec2<f32>(0.0, edge_width), glyph);
    return vec2<bool>(
        right.x == 0 || left.x == 0 || up.x == 0 || down.x == 0,
        right.x + right.y == 0 || left.x + left.y == 0
            || up.x + up.y == 0 || down.x + down.y == 0,
    );
}

// Whether a lane's inside fragment sits within `edge_width` of one of its
// curves — the only case where the interior-edge suppression (the neighbor
// walk) matters.
fn lane_needs_neighbor_test(lane: LaneTerms, edge_width: f32) -> bool {
    return lane.winding != 0 && lane.adjusted <= edge_width;
}

// One lane's inside-POSITIVE smoothstep coverage. `no_outside` is the lane's
// interior-edge suppression: an inside fragment within edge_width of a curve
// sits either near the true outer silhouette (apply the AA ramp) or near an
// interior edge where two filled regions overlap (keep solid). The overlap
// case has no outside neighbor, so the down-ramp toward the submerged edge
// must be suppressed.
fn lane_coverage(lane: LaneTerms, edge_width: f32, no_outside: bool) -> f32 {
    let inside = lane.winding != 0;
    if lane.adjusted > edge_width {
        return select(0.0, 1.0, inside);
    }
    if inside && no_outside {
        return 1.0;
    }
    // This smoothstep ramp is inside-POSITIVE (unlike lane_signed_distance,
    // whose band_coverage consumer is inside-negative). Outside, -adjusted
    // = dilation - distance goes positive within the dilated halo; inside,
    // the dilated edge is farther out than the raw edge:
    // distance + dilation = adjusted + 2 * dilation.
    let signed_distance = select(-lane.adjusted, lane.adjusted + 2.0 * lane.dilation, inside);
    return smoothstep(-edge_width, edge_width, signed_distance);
}

// Interior-edge suppression flags (x = exempt lane, y = whole-path union),
// walking the neighbor windings once for both and only when some evaluation
// needs the test.
fn lanes_no_outside_neighbor(
    terms: CoverageTerms,
    union_terms: LaneTerms,
    point: vec2<f32>,
    edge_width: f32,
    glyph: GlyphRecord,
) -> vec2<bool> {
    if lane_needs_neighbor_test(terms.exempt, edge_width)
        || lane_needs_neighbor_test(union_terms, edge_width) {
        let any_outside = lanes_any_outside_neighbor(point, edge_width, glyph);
        return vec2<bool>(!any_outside.x, !any_outside.y);
    }
    return vec2<bool>(false, false);
}

fn distance_coverage(
    point: vec2<f32>,
    pixel: vec2<f32>,
    dilation_max: f32,
    hairline_target: f32,
    glyph: GlyphRecord,
) -> f32 {
    let edge_width = max(max(pixel.x, pixel.y) * EDGE_FILTER_WIDTH, ROOT_EPSILON);
    // The distance scan must reach the most-dilated silhouette plus the AA ramp.
    let scan_width = edge_width + dilation_max;
    let scan_width_sq = scan_width * scan_width;
    var terms = horizontal_coverage_terms(point, scan_width_sq, hairline_target, glyph);
    terms = nearest_vertical_curve(point, scan_width_sq, hairline_target, glyph, terms);
    let union_terms = union_lane(terms);
    let no_outside = lanes_no_outside_neighbor(terms, union_terms, point, edge_width, glyph);

    // The fade factor interpolates between only-exempt-visible (factor 0)
    // and the whole unfaded path (factor 1, the pre-fade single-winding
    // evaluation). The junction where an exempt contour abuts a fading one
    // is union-interior, so no per-lane AA ramps meet there at half alpha.
    let exempt = lane_coverage(terms.exempt, edge_width, no_outside.x);
    let union_coverage = lane_coverage(union_terms, edge_width, no_outside.y);
    let fade = hairline_fade_factor(terms.faded.dilation, hairline_target, terms.fade_exponent);
    return mix(exempt, union_coverage, fade);
}

// One signed-distance evaluation plus the faded lane's winning dilation and
// fade exponent, so the aa_band consumers can apply the per-evaluation fade
// factor. sd.x is the exempt lane, sd.y the whole-path union.
struct SdSample {
    sd: vec2<f32>,
    dilation: f32,
    fade_exponent: f32,
}

// One lane's signed design-space distance to its per-curve-dilated
// silhouette: negative inside, positive outside, saturated to ±scan_width
// beyond the scan range. Interior overlaps are forced solidly negative (same
// case lane_coverage suppresses).
fn lane_signed_distance(lane: LaneTerms, scan_width: f32, no_outside: bool) -> f32 {
    let inside = lane.winding != 0;
    if lane.adjusted > scan_width {
        return select(scan_width, -scan_width, inside);
    }
    if inside && no_outside {
        return -scan_width;
    }
    return select(lane.adjusted, -(lane.adjusted + 2.0 * lane.dilation), inside);
}

// Exempt-lane and whole-path-union signed distances feeding the screen-space
// AA band used by aa_band mode.
fn signed_distance_sample(
    point: vec2<f32>,
    scan_width_sq: f32,
    hairline_target: f32,
    glyph: GlyphRecord,
) -> SdSample {
    let scan_width = sqrt(scan_width_sq);
    var terms = horizontal_coverage_terms(point, scan_width_sq, hairline_target, glyph);
    terms = nearest_vertical_curve(point, scan_width_sq, hairline_target, glyph, terms);
    let union_terms = union_lane(terms);
    let no_outside = lanes_no_outside_neighbor(terms, union_terms, point, scan_width, glyph);
    return SdSample(
        vec2<f32>(
            lane_signed_distance(terms.exempt, scan_width, no_outside.x),
            lane_signed_distance(union_terms, scan_width, no_outside.y),
        ),
        terms.faded.dilation,
        terms.fade_exponent,
    );
}

// Exempt/union signed distances for callers that need only the field values
// (ramp width finite differences).
fn signed_distance(
    point: vec2<f32>,
    scan_width_sq: f32,
    hairline_target: f32,
    glyph: GlyphRecord,
) -> vec2<f32> {
    return signed_distance_sample(point, scan_width_sq, hairline_target, glyph).sd;
}

// Coverage from a signed distance and a screen-space band width: a 1px box ramp
// centered on the silhouette (sd 0). Negative sd (inside) → 1, positive → 0.
fn band_coverage(sd: f32, band: f32) -> f32 {
    return clamp(0.5 - sd / band, 0.0, 1.0);
}

// Anisotropic supersample of the band coverage. A single band sample models the
// silhouette as a straight edge through the nearest point; at a convex corner the
// fill is a thin wedge, so under a foreshortened footprint the straight-edge model
// over-covers (the grazing-angle ghost wing). Stride N ~= the footprint anisotropy
// samples along the longer footprint axis to integrate across the corner; the
// well-resolved short axis stays a single sample.
//
// The per-sample ramp width is rebuilt from directional differences of the signed
// distance along the two footprint axes: the major contribution shrinks with N
// (the sub-sample spacing), the minor contribution stays full so edges whose
// normal is the well-resolved axis keep a ~1px ramp. signed_distance is
// 1-Lipschitz, so each difference is clamped to its step length; that rejects the
// spike when a finite-difference sample lands in a band where an axis-parallel
// edge isn't visible. Head-on the footprint is isotropic, N collapses to 1.
fn aniso_band_coverage(
    point: vec2<f32>,
    dx: vec2<f32>,
    dy: vec2<f32>,
    scan_width_sq: f32,
    hairline_target: f32,
    glyph: GlyphRecord,
) -> f32 {
    let sd_center = signed_distance(point, scan_width_sq, hairline_target, glyph);
    let len_dx = length(dx);
    let len_dy = length(dy);
    let major = select(dy, dx, len_dx >= len_dy);
    let minor = select(dx, dy, len_dx >= len_dy);
    let major_len = max(len_dx, len_dy);
    let minor_len = max(min(len_dx, len_dy), ROOT_EPSILON);
    let sample_count = clamp(ceil(major_len / minor_len), 1.0, MAX_ANISO_SAMPLES);
    let inv_count = 1.0 / sample_count;

    // Per-evaluation (exempt / union) band widths from signed-distance
    // differences.
    let d_major = min(abs(signed_distance(point + major, scan_width_sq, hairline_target, glyph) - sd_center), vec2<f32>(major_len));
    let d_minor = min(abs(signed_distance(point + minor, scan_width_sq, hairline_target, glyph) - sd_center), vec2<f32>(minor_len));
    let per_band = max(d_minor + d_major * inv_count, vec2<f32>(ROOT_EPSILON));

    // Fade applies per stride sample from that sample's faded-lane winning
    // curve, so adjacent samples that select different winning curves each
    // fade by their own curve's deficit and exponent.
    let count = u32(sample_count);
    var sum = 0.0;
    for (var index = 0u; index < count; index += 1u) {
        let stride = (f32(index) + 0.5) * inv_count - 0.5;
        let sample = signed_distance_sample(point + stride * major, scan_width_sq, hairline_target, glyph);
        let exempt = band_coverage(sample.sd.x, per_band.x);
        let union_coverage = band_coverage(sample.sd.y, per_band.y);
        let fade = hairline_fade_factor(sample.dilation, hairline_target, sample.fade_exponent);
        sum += mix(exempt, union_coverage, fade);
    }
    return sum * inv_count;
}

fn render_coverage(
    uv: vec2<f32>,
    glyph: GlyphRecord,
    render_mode: u32,
    aa_flags: u32,
) -> f32 {
    // Derivatives stay at the top, BEFORE any branch: aa_flags is per-run data
    // recovered from an interpolated varying, so the branches below are
    // non-uniform control flow where fwidth/dpdx/dpdy are undefined in WGSL.
    // Every derivative this function needs is computed here; in-branch ramp
    // widths are rebuilt from finite differences along dx/dy instead.
    let point = design_position(uv, glyph);
    let pixel = max(abs(fwidth(point)), vec2<f32>(ROOT_EPSILON));
    let dx = dpdx(point);
    let dy = dpdy(point);

    // Hairline dilation: a contour whose stroke falls below
    // uniforms.hairline_min_px on screen is rendered dilated to that width, so
    // a ruler tick stays a uniform thin line instead of a sub-pixel sliver
    // whose brightness depends on where it lands in the pixel grid. Each curve
    // dilates by its own contour's deficit (solver.w), so a merged path mixing
    // stroke widths dilates every member exactly to the floor. hairline_target
    // is the floor in design units, sized from the well-resolved footprint
    // axis so grazing-angle foreshortening cannot balloon the dilation;
    // dilation_max (from the narrowest contour) sizes the distance scan.
    // glyph.min_feature == 0 (text glyphs) disables.
    var dilation_max = 0.0;
    var hairline_target = 0.0;
    if glyph.min_feature > 0.0 {
        let pixel_design_units = min(pixel.x, pixel.y);
        hairline_target = pixel_design_units * uniforms.hairline_min_px;
        dilation_max = max(0.0, (hairline_target - glyph.min_feature) * 0.5);
    }

    // The two AA axes are independent. aa_band picks the edge-ramp width (scalar
    // design-space `edge_width` vs. a screen-space band that holds ~1px per axis,
    // so the convex-corner apron can't balloon at grazing). supersample picks 1
    // sample vs. an anisotropic stride along the foreshortened axis, which
    // integrates the along-footprint coverage a single sample can't capture (the
    // grazing-angle corner wing). All four combinations are valid; the combined
    // mode fixes both artifacts at once.
    var coverage: f32;
    if (aa_flags & AA_FLAG_BAND) != 0u {
        let edge_width = max(max(pixel.x, pixel.y) * EDGE_FILTER_WIDTH, ROOT_EPSILON);
        let scan_width = edge_width + dilation_max;
        let scan_width_sq = scan_width * scan_width;
        if (aa_flags & AA_FLAG_SUPERSAMPLE) != 0u {
            coverage = aniso_band_coverage(point, dx, dy, scan_width_sq, hairline_target, glyph);
        } else {
            // Single sample: one full-footprint exempt/union band from the
            // center sample's screen-space distance change. Forward
            // differences along dx/dy stand in for fwidth(sd), which is
            // unavailable here — this branch is non-uniform flow (aa_flags is
            // per-run).
            let center = signed_distance_sample(point, scan_width_sq, hairline_target, glyph);
            let band = max(
                abs(signed_distance(point + dx, scan_width_sq, hairline_target, glyph) - center.sd)
                    + abs(signed_distance(point + dy, scan_width_sq, hairline_target, glyph) - center.sd),
                vec2<f32>(ROOT_EPSILON),
            );
            let exempt = band_coverage(center.sd.x, band.x);
            let union_coverage = band_coverage(center.sd.y, band.y);
            let fade = hairline_fade_factor(center.dilation, hairline_target, center.fade_exponent);
            coverage = mix(exempt, union_coverage, fade);
        }
    } else if (aa_flags & AA_FLAG_SUPERSAMPLE) != 0u {
        // Scalar band, four rotated-grid footprint samples (the original path). At
        // grazing angles dx/dy stretch along the foreshortened axis, so the
        // samples integrate the coverage strip a single sample cannot capture.
        var sum = 0.0;
        sum += distance_coverage(point + 0.375 * dx + 0.125 * dy, pixel, dilation_max, hairline_target, glyph);
        sum += distance_coverage(point - 0.125 * dx + 0.375 * dy, pixel, dilation_max, hairline_target, glyph);
        sum += distance_coverage(point - 0.375 * dx - 0.125 * dy, pixel, dilation_max, hairline_target, glyph);
        sum += distance_coverage(point + 0.125 * dx - 0.375 * dy, pixel, dilation_max, hairline_target, glyph);
        coverage = sum * 0.25;
    } else {
        coverage = distance_coverage(point, pixel, dilation_max, hairline_target, glyph);
    }

    if render_mode == RENDER_MODE_PUNCH_OUT {
        return 1.0 - coverage;
    }
    return coverage;
}

#ifdef PREPASS_PIPELINE
@fragment
fn fragment(in: VertexOutput) {
#ifdef VERTEX_UVS_A
#ifdef VERTEX_UVS_B
    // The shadow map stores a binary silhouette, so one winding test answers
    // it: keep fragments inside the outline (punch-out runs invert the test).
    let glyph = glyphs[glyph_index(in.uv_b)];
    let point = design_position(in.uv, glyph);
    let inside = winding_at(point, glyph) != 0;
    if inside == (run_render_mode(in.uv_b) == RENDER_MODE_PUNCH_OUT) {
        discard;
    }
#else
    discard;
#endif
#else
    discard;
#endif
}
#else
@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
#ifndef VERTEX_UVS_A
    discard;
#endif
#ifndef VERTEX_UVS_B
    discard;
#endif

    let glyph = glyphs[glyph_index(in.uv_b)];
    let fill_color = run_fill_color(in.uv_b);
    let coverage = render_coverage(
        in.uv,
        glyph,
        run_render_mode(in.uv_b),
        run_aa_flags(in.uv_b),
    );
    let final_alpha = coverage * fill_color.a;
    // This discard precedes oit_draw below, so faded near-zero fragments never
    // occupy OIT fragment-pool slots.
    if final_alpha < DISCARD_ALPHA {
        discard;
    }

    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = vec4<f32>(
        fill_color.rgb,
        final_alpha,
    );
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material,
        pbr_input.material.base_color,
    );

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

#ifdef OIT_ENABLED
    let alpha_mode = pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if alpha_mode != STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE {
        // Offset position.z so coplanar layers get distinct depths in the OIT
        // linked list; pipeline depth_bias does not affect in.position.z.
        var oit_pos = in.position;
        oit_pos.z = max(oit_pos.z + run_oit_depth_offset(in.uv_b), OIT_MIN_DEPTH);
        oit_draw(oit_pos, out.color);
        discard;
    }
#endif

    return out;
}
#endif
