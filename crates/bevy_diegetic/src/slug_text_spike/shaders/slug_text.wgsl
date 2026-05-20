// Isolated Slug feasibility shader.
//
// This first pass evaluates even-odd fill coverage from quadratic curve
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
const DISCARD_ALPHA: f32 = 0.02;
const COVERAGE_SAMPLE_COUNT: f32 = 5.0;
const RENDER_MODE_TEXT: u32 = 1u;
const RENDER_MODE_PUNCH_OUT: u32 = 2u;
const RENDER_MODE_SOLID_QUAD: u32 = 3u;

struct SlugTextUniform {
    fill_color: vec4<f32>,
    render_mode: u32,
}

struct SlugCurveRecord {
    start_control: vec4<f32>,
    end: vec4<f32>,
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

fn quadratic_point(curve: SlugCurveRecord, t: f32) -> vec2<f32> {
    let inverse_t = 1.0 - t;
    let start = curve.start_control.xy;
    let control = curve.start_control.zw;
    let end = curve.end.xy;
    return inverse_t * inverse_t * start +
        2.0 * inverse_t * t * control +
        t * t * end;
}

fn crossing_for_t(curve: SlugCurveRecord, point: vec2<f32>, t: f32) -> u32 {
    if t < 0.0 || t >= 1.0 {
        return 0u;
    }

    let crossing = quadratic_point(curve, t).x > point.x;
    return select(0u, 1u, crossing);
}

fn curve_crossings(curve: SlugCurveRecord, point: vec2<f32>) -> u32 {
    let start_y = curve.start_control.y;
    let control_y = curve.start_control.w;
    let end_y = curve.end.y;
    let a = start_y - 2.0 * control_y + end_y;
    let b = 2.0 * (control_y - start_y);
    let c = start_y - point.y;

    if abs(a) < ROOT_EPSILON {
        if abs(b) < ROOT_EPSILON {
            return 0u;
        }
        return crossing_for_t(curve, point, -c / b);
    }

    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return 0u;
    }

    let root = sqrt(discriminant);
    return crossing_for_t(curve, point, (-b - root) / (2.0 * a)) +
        crossing_for_t(curve, point, (-b + root) / (2.0 * a));
}

fn band_index(point: vec2<f32>, glyph: SlugGlyphRecord) -> u32 {
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

fn inside_at(point: vec2<f32>, glyph: SlugGlyphRecord) -> bool {
    let band = bands[glyph.band_range.x + band_index(point, glyph)];
    var crossings = 0u;

    for (var offset = 0u; offset < band.count; offset += 1u) {
        crossings += curve_crossings(curves[band.start + offset], point);
    }

    return (crossings & 1u) == 1u;
}

fn inside_value(point: vec2<f32>, glyph: SlugGlyphRecord) -> f32 {
    return select(0.0, 1.0, inside_at(point, glyph));
}

fn slug_coverage(uv: vec2<f32>, glyph: SlugGlyphRecord) -> f32 {
    let point = design_position(uv, glyph);
    let pixel = max(abs(fwidth(point)) * 0.5, vec2<f32>(ROOT_EPSILON));
    return (
        inside_value(point, glyph) +
        inside_value(point + vec2<f32>(pixel.x, 0.0), glyph) +
        inside_value(point - vec2<f32>(pixel.x, 0.0), glyph) +
        inside_value(point + vec2<f32>(0.0, pixel.y), glyph) +
        inside_value(point - vec2<f32>(0.0, pixel.y), glyph)
    ) / COVERAGE_SAMPLE_COUNT;
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
