// Slug analytic coverage text shader.
//
// This first pass evaluates non-zero winding coverage from quadratic curve
// records grouped into horizontal bands. It is deliberately separate from
// the production text renderer.

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

const ROOT_EPSILON: f32 = 0.00001;
const DEGENERATE_EPS: f32 = 0.00000001;
const SQRT_3_OVER_2: f32 = 0.8660254037844386;
const DISCARD_ALPHA: f32 = 0.02;
const EDGE_FILTER_WIDTH: f32 = 1.2;
const RENDER_MODE_TEXT: u32 = 1u;
const RENDER_MODE_PUNCH_OUT: u32 = 2u;
const RENDER_MODE_SOLID_QUAD: u32 = 3u;

struct SlugTextUniform {
    fill_color: vec4<f32>,
    render_mode: u32,
    is_shadow_proxy: u32,
}

struct SlugCurveRecord {
    start_delta: vec4<f32>,
    curve_end: vec4<f32>,
    bounds: vec4<f32>,
    solver: vec4<f32>,
}

struct SlugBandRecord {
    start: u32,
    count: u32,
    y_min: f32,
    y_max: f32,
}

struct SlugGlyphRecord {
    bounds_min_size: vec4<f32>,
    band_range: vec4<u32>,
}

struct CoverageTerms {
    winding: i32,
    distance_sq: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> uniforms: SlugTextUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<storage, read> curves: array<SlugCurveRecord>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<storage, read> bands: array<SlugBandRecord>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var<storage, read> glyphs: array<SlugGlyphRecord>;

fn glyph_index(glyph_uv: vec2<f32>) -> u32 {
    return u32(floor(glyph_uv.x));
}

fn glyph_bounds_min(glyph: SlugGlyphRecord) -> vec2<f32> {
    return glyph.bounds_min_size.xy;
}

fn glyph_bounds_size(glyph: SlugGlyphRecord) -> vec2<f32> {
    return glyph.bounds_min_size.zw;
}

fn design_position(uv: vec2<f32>, glyph: SlugGlyphRecord) -> vec2<f32> {
    let bounds_min = glyph_bounds_min(glyph);
    let bounds_size = glyph_bounds_size(glyph);
    return bounds_min + vec2<f32>(
        uv.x * bounds_size.x,
        (1.0 - uv.y) * bounds_size.y,
    );
}

fn horizontal_band_index(point: vec2<f32>, glyph: SlugGlyphRecord) -> u32 {
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

fn vertical_band_index(point: vec2<f32>, glyph: SlugGlyphRecord) -> u32 {
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

fn cbrt_signed(x: f32) -> f32 {
    if x < 0.0 {
        return -pow(-x, 1.0 / 3.0);
    }
    return pow(x, 1.0 / 3.0);
}

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

fn exact_quadratic_distance_sq(
    curve: SlugCurveRecord,
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

fn winding_for_t(curve: SlugCurveRecord, point: vec2<f32>, t: f32) -> i32 {
    if t < 0.0 || t >= 1.0 {
        return 0;
    }

    let curve_x = curve.start_delta.x +
        2.0 * curve.start_delta.z * t +
        curve.curve_end.x * t * t;
    if curve_x <= point.x {
        return 0;
    }

    let dy = 2.0 * (curve.start_delta.w + curve.curve_end.y * t);
    if abs(dy) < ROOT_EPSILON {
        return 0;
    }
    return select(-1, 1, dy > 0.0);
}

fn curve_winding(curve: SlugCurveRecord, point: vec2<f32>) -> i32 {
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

fn outside_glyph_bounds(point: vec2<f32>, glyph: SlugGlyphRecord) -> bool {
    let bounds_min = glyph_bounds_min(glyph);
    let bounds_max = bounds_min + glyph_bounds_size(glyph);
    return point.x < bounds_min.x ||
        point.x > bounds_max.x ||
        point.y < bounds_min.y ||
        point.y > bounds_max.y;
}

fn horizontal_coverage_terms(
    point: vec2<f32>,
    edge_width_sq: f32,
    glyph: SlugGlyphRecord,
) -> CoverageTerms {
    let include_winding = !outside_glyph_bounds(point, glyph);
    let horizontal_band = bands[glyph.band_range.x + horizontal_band_index(point, glyph)];
    var terms = CoverageTerms(0, 1000000000000.0);
    for (var offset = 0u; offset < horizontal_band.count; offset += 1u) {
        let curve = curves[horizontal_band.start + offset];
        if include_winding {
            terms.winding += curve_winding(curve, point);
        }
        // solver.w == 1.0 means the curve also lives in the vertical band;
        // skip the distance solve here to avoid computing it twice.
        if curve.solver.w < 0.5 && curve_bounds_distance_sq(point, curve) <= edge_width_sq {
            terms.distance_sq = min(terms.distance_sq, curve_distance_sq(point, curve));
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

fn curve_distance_sq(point: vec2<f32>, curve: SlugCurveRecord) -> f32 {
    return exact_quadratic_distance_sq(
        curve,
        point,
        curve.start_delta.xy,
        curve.start_delta.zw,
        curve.curve_end.xy,
        curve.curve_end.zw,
    );
}

fn curve_bounds_distance_sq(point: vec2<f32>, curve: SlugCurveRecord) -> f32 {
    let nearest = clamp(point, curve.bounds.xy, curve.bounds.zw);
    let diff = point - nearest;
    return dot(diff, diff);
}

fn nearest_vertical_curve_distance_sq(
    point: vec2<f32>,
    edge_width_sq: f32,
    glyph: SlugGlyphRecord,
    initial_distance_sq: f32,
) -> f32 {
    let vertical_band = bands[glyph.band_range.z + vertical_band_index(point, glyph)];
    var distance_sq = initial_distance_sq;
    for (var offset = 0u; offset < vertical_band.count; offset += 1u) {
        let curve = curves[vertical_band.start + offset];
        if curve_bounds_distance_sq(point, curve) <= edge_width_sq {
            distance_sq = min(distance_sq, curve_distance_sq(point, curve));
        }
    }
    return distance_sq;
}

fn distance_coverage(point: vec2<f32>, pixel: vec2<f32>, glyph: SlugGlyphRecord) -> f32 {
    let edge_width = max(max(pixel.x, pixel.y) * EDGE_FILTER_WIDTH, ROOT_EPSILON);
    let edge_width_sq = edge_width * edge_width;
    let terms = horizontal_coverage_terms(point, edge_width_sq, glyph);
    let inside = terms.winding != 0;
    let distance_sq = nearest_vertical_curve_distance_sq(
        point,
        edge_width_sq,
        glyph,
        terms.distance_sq,
    );
    if distance_sq > edge_width_sq {
        return select(0.0, 1.0, inside);
    }

    let distance = sqrt(distance_sq);
    let signed_distance = select(-distance, distance, inside);
    return smoothstep(-edge_width, edge_width, signed_distance);
}

fn slug_coverage(uv: vec2<f32>, glyph: SlugGlyphRecord) -> f32 {
    let point = design_position(uv, glyph);
    let pixel = max(abs(fwidth(point)), vec2<f32>(ROOT_EPSILON));
    return distance_coverage(point, pixel, glyph);
}

fn render_coverage(uv: vec2<f32>, glyph: SlugGlyphRecord) -> f32 {
    if uniforms.render_mode == RENDER_MODE_SOLID_QUAD {
        return 1.0;
    }

    let coverage = slug_coverage(uv, glyph);
    if uniforms.render_mode == RENDER_MODE_PUNCH_OUT {
        return 1.0 - coverage;
    }

    return coverage;
}

#ifdef PREPASS_PIPELINE
@fragment
fn fragment(in: VertexOutput) {
#ifdef VERTEX_UVS_A
#ifdef VERTEX_UVS_B
    let glyph = glyphs[glyph_index(in.uv_b)];
    if render_coverage(in.uv, glyph) < 0.5 {
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
    // Shadow proxies only contribute depth in the prepass; they never paint
    // in the main color pass.
    if uniforms.is_shadow_proxy == 1u {
        discard;
    }
#ifndef VERTEX_UVS_A
    discard;
#endif
#ifndef VERTEX_UVS_B
    discard;
#endif

    let glyph = glyphs[glyph_index(in.uv_b)];
    let coverage = render_coverage(in.uv, glyph);
    let final_alpha = coverage * uniforms.fill_color.a;
    if final_alpha < DISCARD_ALPHA {
        discard;
    }

    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = vec4<f32>(
        uniforms.fill_color.rgb,
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

    return out;
}
#endif
