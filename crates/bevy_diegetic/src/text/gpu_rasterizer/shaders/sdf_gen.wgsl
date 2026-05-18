// Single-channel signed-distance-field generation kernel.
//
// One workgroup grid per glyph (indexed by workgroup_id.z via the
// glyph header array). Each thread writes one texel of the output
// atlas page after a per-edge nearest-distance search and a
// horizontal-ray-cast sign correction (non-zero winding rule).

const EDGE_KIND_MASK: u32 = 3u;
const EDGE_KIND_LINEAR: u32 = 0u;
const EDGE_KIND_QUADRATIC: u32 = 1u;
const EDGE_KIND_CUBIC: u32 = 2u;

const NEWTON_ITER: u32 = 4u;

struct EdgeSegment {
    p0x: f32, p0y: f32,
    p1x: f32, p1y: f32,
    p2x: f32, p2y: f32,
    p3x: f32, p3y: f32,
    kind: u32,
}

struct GlyphHeader {
    edge_offset:    u32,
    edge_count:     u32,
    atlas_origin_x: u32,
    atlas_origin_y: u32,
    bitmap_w:       u32,
    bitmap_h:       u32,
    _padding0:      u32,
    _padding1:      u32,
}

struct RasterParams {
    sdf_range:      f32,
    padding_texels: u32,
    distance_field: u32,
    glyph_count:    u32,
}

@group(0) @binding(0) var<storage, read>  edges:   array<EdgeSegment>;
@group(0) @binding(1) var<storage, read>  glyphs:  array<GlyphHeader>;
@group(0) @binding(2) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<uniform>        params:  RasterParams;

fn bezier_linear(t: f32, p0: vec2<f32>, p1: vec2<f32>) -> vec2<f32> {
    return mix(p0, p1, t);
}

fn bezier_quadratic(t: f32, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> vec2<f32> {
    let one_minus = 1.0 - t;
    return one_minus * one_minus * p0 + 2.0 * one_minus * t * p1 + t * t * p2;
}

fn bezier_cubic(t: f32, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> vec2<f32> {
    let one_minus = 1.0 - t;
    let mt2 = one_minus * one_minus;
    let t2 = t * t;
    return mt2 * one_minus * p0 + 3.0 * mt2 * t * p1 + 3.0 * one_minus * t2 * p2 + t2 * t * p3;
}

fn bezier_quadratic_deriv(t: f32, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> vec2<f32> {
    return 2.0 * (1.0 - t) * (p1 - p0) + 2.0 * t * (p2 - p1);
}

fn bezier_cubic_deriv(t: f32, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> vec2<f32> {
    let one_minus = 1.0 - t;
    return 3.0 * one_minus * one_minus * (p1 - p0)
         + 6.0 * one_minus * t * (p2 - p1)
         + 3.0 * t * t * (p3 - p2);
}

// Distance from a point to a line segment p0..p1.
fn distance_linear(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>) -> f32 {
    let d = p1 - p0;
    let len_sq = max(dot(d, d), 1e-20);
    let t = clamp(dot(pt - p0, d) / len_sq, 0.0, 1.0);
    return length(pt - (p0 + t * d));
}

// Distance from a point to a quadratic bezier, found by Newton-refining
// the closest parameter starting from several seeds.
fn distance_quadratic(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> f32 {
    var best = distance_linear(pt, p0, p2);
    var ts = array<f32, 5>(0.0, 0.25, 0.5, 0.75, 1.0);
    for (var s = 0u; s < 5u; s = s + 1u) {
        var t = ts[s];
        for (var it = 0u; it < NEWTON_ITER; it = it + 1u) {
            let bt = bezier_quadratic(t, p0, p1, p2);
            let dbt = bezier_quadratic_deriv(t, p0, p1, p2);
            let dot_d = dot(dbt, dbt);
            if (dot_d < 1e-20) { break; }
            let delta = dot(bt - pt, dbt) / dot_d;
            t = clamp(t - delta, 0.0, 1.0);
        }
        let dist = length(pt - bezier_quadratic(t, p0, p1, p2));
        if (dist < best) { best = dist; }
    }
    return best;
}

fn distance_cubic(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> f32 {
    var best = distance_linear(pt, p0, p3);
    var ts = array<f32, 5>(0.0, 0.25, 0.5, 0.75, 1.0);
    for (var s = 0u; s < 5u; s = s + 1u) {
        var t = ts[s];
        for (var it = 0u; it < NEWTON_ITER; it = it + 1u) {
            let bt = bezier_cubic(t, p0, p1, p2, p3);
            let dbt = bezier_cubic_deriv(t, p0, p1, p2, p3);
            let dot_d = dot(dbt, dbt);
            if (dot_d < 1e-20) { break; }
            let delta = dot(bt - pt, dbt) / dot_d;
            t = clamp(t - delta, 0.0, 1.0);
        }
        let dist = length(pt - bezier_cubic(t, p0, p1, p2, p3));
        if (dist < best) { best = dist; }
    }
    return best;
}

// Signed winding contribution of a single line segment for a
// rightward horizontal ray cast from `pt`.
fn winding_linear(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>) -> i32 {
    let dy = p1.y - p0.y;
    if (abs(dy) < 1e-20) { return 0; }
    let t = (pt.y - p0.y) / dy;
    if (t < 0.0 || t >= 1.0) { return 0; }
    let x = p0.x + t * (p1.x - p0.x);
    if (x < pt.x) { return 0; }
    return select(-1, 1, dy > 0.0);
}

fn winding_quadratic(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> i32 {
    // Approximate via subdivision into linear segments.
    var acc: i32 = 0;
    let steps = 8u;
    var prev = p0;
    for (var i = 1u; i <= steps; i = i + 1u) {
        let t = f32(i) / f32(steps);
        let next = bezier_quadratic(t, p0, p1, p2);
        acc = acc + winding_linear(pt, prev, next);
        prev = next;
    }
    return acc;
}

fn winding_cubic(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> i32 {
    var acc: i32 = 0;
    let steps = 12u;
    var prev = p0;
    for (var i = 1u; i <= steps; i = i + 1u) {
        let t = f32(i) / f32(steps);
        let next = bezier_cubic(t, p0, p1, p2, p3);
        acc = acc + winding_linear(pt, prev, next);
        prev = next;
    }
    return acc;
}

@compute @workgroup_size(8, 8, 1)
fn sdf_main(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(workgroup_id) wg: vec3<u32>,
) {
    let glyph_idx = wg.z;
    if (glyph_idx >= params.glyph_count) { return; }
    let header = glyphs[glyph_idx];
    if (gid.x >= header.bitmap_w || gid.y >= header.bitmap_h) { return; }

    let pt = vec2<f32>(f32(gid.x) + 0.5, f32(gid.y) + 0.5);

    var min_dist: f32 = 1e30;
    var winding: i32 = 0;
    for (var i = 0u; i < header.edge_count; i = i + 1u) {
        let e = edges[header.edge_offset + i];
        let kind = e.kind & EDGE_KIND_MASK;
        let p0 = vec2<f32>(e.p0x, e.p0y);
        let p1 = vec2<f32>(e.p1x, e.p1y);
        let p2 = vec2<f32>(e.p2x, e.p2y);
        let p3 = vec2<f32>(e.p3x, e.p3y);

        var d: f32 = 1e30;
        var w: i32 = 0;
        if (kind == EDGE_KIND_LINEAR) {
            d = distance_linear(pt, p0, p1);
            w = winding_linear(pt, p0, p1);
        } else if (kind == EDGE_KIND_QUADRATIC) {
            d = distance_quadratic(pt, p0, p1, p2);
            w = winding_quadratic(pt, p0, p1, p2);
        } else if (kind == EDGE_KIND_CUBIC) {
            d = distance_cubic(pt, p0, p1, p2, p3);
            w = winding_cubic(pt, p0, p1, p2, p3);
        }
        min_dist = min(min_dist, d);
        winding = winding + w;
    }

    let sign_inside: f32 = select(-1.0, 1.0, winding != 0);
    let signed_dist = min_dist * sign_inside;
    let normalized = clamp(signed_dist / params.sdf_range + 0.5, 0.0, 1.0);

    let out_xy = vec2<i32>(
        i32(header.atlas_origin_x + gid.x),
        i32(header.atlas_origin_y + gid.y),
    );
    textureStore(output, out_xy, vec4<f32>(normalized, normalized, normalized, 1.0));
}
