// Three-channel MSDF generation kernel.
//
// One workgroup grid per glyph. Each thread writes one RGBA texel of
// the output atlas page after a per-edge nearest-distance search per
// channel, signed pseudo-distance correction at endpoints, and a
// horizontal-ray-cast sign reconciliation (non-zero winding rule).
//
// Mirrors fdsm's `generate_msdf` algorithm (Chlumský 2015):
//   1. Picking: per channel C, find the edge with smallest unsigned
//      true distance (clamped foot), restricted to edges whose color
//      mask bit C is set.
//   2. Output: convert that edge's distance into a signed pseudo
//      distance via `distance_to_pseudodistance`. The sign for the
//      clamped-foot case comes from cross(tangent, foot - pt); the
//      pseudo branch (parameter outside [0,1]) extends along the
//      endpoint tangent.
//   3. Sign reconciliation: a separate winding count flips all three
//      channel signs if they disagree with the global non-zero
//      winding rule.

const EDGE_KIND_MASK: u32 = 3u;
const EDGE_KIND_LINEAR: u32 = 0u;
const EDGE_KIND_QUADRATIC: u32 = 1u;
const EDGE_KIND_CUBIC: u32 = 2u;
const EDGE_CHANNEL_MASK_SHIFT: u32 = 2u;
const EDGE_CHANNEL_MASK_BITS: u32 = 7u;

const NEWTON_ITER: u32 = 4u;
const SQRT_3_OVER_2: f32 = 0.8660254;
const DEGENERATE_EPS: f32 = 1e-20;
const INF_DIST: f32 = 1e30;

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

fn bezier_cubic_deriv2(t: f32, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> vec2<f32> {
    let one_minus = 1.0 - t;
    return 6.0 * one_minus * (p2 - 2.0 * p1 + p0)
         + 6.0 * t * (p3 - 2.0 * p2 + p1);
}

fn cbrt_signed(x: f32) -> f32 {
    if (x < 0.0) { return -pow(-x, 1.0 / 3.0); }
    return pow(x, 1.0 / 3.0);
}

fn solve_cubic_normed(a: f32, b: f32, c: f32, roots: ptr<function, array<f32, 3>>) -> u32 {
    let a2 = a * a;
    let q = (1.0 / 9.0) * (a2 - 3.0 * b);
    let r = (1.0 / 54.0) * (a * (2.0 * a2 - 9.0 * b) + 27.0 * c);
    let r2 = r * r;
    let q3 = q * q * q;
    let a_third = a * (1.0 / 3.0);
    if (r2 < q3) {
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

// Per-edge true-distance result.
//
// `dist_sq` is the squared distance from `pt` to the clamped foot.
// `param` is the unclamped parameter t (can fall outside [0,1] when
// the closest foot is at an endpoint and the point lies past it —
// used to decide whether pseudo-distance correction applies).
// `foot` is the clamped foot point on the curve.
// `tangent` is the curve direction at the clamped parameter, unnormalized.
struct EdgeDist {
    dist_sq: f32,
    param:   f32,
    foot:    vec2<f32>,
    tangent: vec2<f32>,
}

fn distance_linear(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>) -> EdgeDist {
    let d = p1 - p0;
    let len_sq = max(dot(d, d), DEGENERATE_EPS);
    let t_raw = dot(pt - p0, d) / len_sq;
    let t_c = clamp(t_raw, 0.0, 1.0);
    let foot = p0 + t_c * d;
    let diff = pt - foot;
    var out: EdgeDist;
    out.dist_sq = dot(diff, diff);
    out.param = t_raw;
    out.foot = foot;
    out.tangent = d;
    return out;
}

fn distance_quadratic(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> EdgeDist {
    let pv = pt - p0;
    let pv1 = p1 - p0;
    let pv2 = p2 - 2.0 * p1 + p0;

    // Initial: endpoint check. Mirrors fdsm's PreparedQuadraticSegment::distance.
    var best_sq = dot(pv, pv);
    var best_t = dot(pv, pv1) / max(dot(pv1, pv1), DEGENERATE_EPS);
    let p2mo = p2 - pt;
    let d2 = dot(p2mo, p2mo);
    if (d2 < best_sq) {
        best_sq = d2;
        let ep_end = p2 - p1;
        best_t = dot(pt - p1, ep_end) / max(dot(ep_end, ep_end), DEGENERATE_EPS);
    }

    let a_norm_sq = dot(pv2, pv2);
    if (a_norm_sq >= DEGENERATE_EPS) {
        let ainv = 1.0 / a_norm_sq;
        var roots: array<f32, 3>;
        let n = solve_cubic_normed(
            3.0 * dot(pv1, pv2) * ainv,
            (2.0 * dot(pv1, pv1) - dot(pv2, pv)) * ainv,
            -dot(pv1, pv) * ainv,
            &roots,
        );
        for (var i = 0u; i < n; i = i + 1u) {
            let tr = roots[i];
            if (tr >= 0.0 && tr <= 1.0) {
                let q = p0 + pv1 * (2.0 * tr) + pv2 * (tr * tr);
                let diff = q - pt;
                let dsq = dot(diff, diff);
                if (dsq < best_sq) {
                    best_sq = dsq;
                    best_t = tr;
                }
            }
        }
    }

    let t_c = clamp(best_t, 0.0, 1.0);
    let foot = bezier_quadratic(t_c, p0, p1, p2);
    let tangent = bezier_quadratic_deriv(t_c, p0, p1, p2);
    var out: EdgeDist;
    out.dist_sq = best_sq;
    out.param = best_t;
    out.foot = foot;
    out.tangent = tangent;
    return out;
}

fn distance_cubic(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> EdgeDist {
    let pv = p0 - pt;
    let pv1 = p1 - p0;
    let pv3_end = p3 - p2;
    // Initial: endpoint check.
    var best_sq = dot(pv, pv);
    var best_t = -dot(pv, pv1) / max(dot(pv1, pv1), DEGENERATE_EPS);
    let p3mo = p3 - pt;
    let d2 = dot(p3mo, p3mo);
    if (d2 < best_sq) {
        best_sq = d2;
        best_t = dot(pt - p2, pv3_end) / max(dot(pv3_end, pv3_end), DEGENERATE_EPS);
    }

    var ts = array<f32, 9>(0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0);
    for (var s = 0u; s < 9u; s = s + 1u) {
        var t = ts[s];
        for (var it = 0u; it < NEWTON_ITER; it = it + 1u) {
            let bt = bezier_cubic(t, p0, p1, p2, p3);
            let d1 = bezier_cubic_deriv(t, p0, p1, p2, p3);
            let d2v = bezier_cubic_deriv2(t, p0, p1, p2, p3);
            let qe = bt - pt;
            let denom = dot(d1, d1) + dot(qe, d2v);
            if (abs(denom) < DEGENERATE_EPS) { break; }
            t = t - dot(qe, d1) / denom;
            if (t <= 0.0 || t >= 1.0) { break; }
            let bt2 = bezier_cubic(t, p0, p1, p2, p3);
            let diff = pt - bt2;
            let dsq = dot(diff, diff);
            if (dsq < best_sq) {
                best_sq = dsq;
                best_t = t;
            }
        }
    }

    let t_c = clamp(best_t, 0.0, 1.0);
    let foot = bezier_cubic(t_c, p0, p1, p2, p3);
    let tangent = bezier_cubic_deriv(t_c, p0, p1, p2, p3);
    var out: EdgeDist;
    out.dist_sq = best_sq;
    out.param = best_t;
    out.foot = foot;
    out.tangent = tangent;
    return out;
}

// 2D perp / cross product: `a.x * b.y - a.y * b.x`.
fn perp2(a: vec2<f32>, b: vec2<f32>) -> f32 {
    return a.x * b.y - a.y * b.x;
}

// Computes the signed pseudo-distance for one edge.
//
// Mirrors `Segment::distance_to_pseudodistance` from fdsm. When the
// closest foot was at an endpoint and the point lies past it along the
// endpoint tangent, the pseudo-distance is the signed perpendicular
// distance to the tangent line; otherwise it is the clamped-foot
// signed distance.
//
// `dir_start` and `dir_end` are the unnormalized endpoint tangents
// (matching fdsm's `direction_at_start` / `direction_at_end` outputs).
fn signed_pseudo_distance(
    pt: vec2<f32>,
    ed: EdgeDist,
    p_start: vec2<f32>,
    p_end: vec2<f32>,
    dir_start: vec2<f32>,
    dir_end: vec2<f32>,
) -> f32 {
    let unsigned_dist = sqrt(ed.dist_sq);
    let pmb = ed.foot - pt;
    let pmb_len = max(length(pmb), DEGENERATE_EPS);
    let pmb_n = pmb / pmb_len;
    let tan_len = max(length(ed.tangent), DEGENERATE_EPS);
    let tan_n = ed.tangent / tan_len;
    let cross_main = perp2(tan_n, pmb_n);
    let main_sign = select(-1.0, 1.0, cross_main >= 0.0);
    let signed_main = unsigned_dist * main_sign;

    if (ed.param < 0.0) {
        let dir_len = max(length(dir_start), DEGENERATE_EPS);
        let dir = dir_start / dir_len;
        let aq = pt - p_start;
        let ts = dot(aq, dir);
        if (ts < 0.0) {
            let pseudo = perp2(aq, dir);
            if (pseudo * pseudo <= ed.dist_sq) {
                return pseudo;
            }
        }
    } else if (ed.param > 1.0) {
        let dir_len = max(length(dir_end), DEGENERATE_EPS);
        let dir = dir_end / dir_len;
        let bq = pt - p_end;
        let ts = dot(bq, dir);
        if (ts > 0.0) {
            let pseudo = perp2(bq, dir);
            if (pseudo * pseudo <= ed.dist_sq) {
                return pseudo;
            }
        }
    }
    return signed_main;
}

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
fn msdf_main(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(workgroup_id) wg: vec3<u32>,
) {
    let glyph_idx = wg.z;
    if (glyph_idx >= params.glyph_count) { return; }
    let header = glyphs[glyph_idx];
    if (gid.x >= header.bitmap_w || gid.y >= header.bitmap_h) { return; }

    let pt = vec2<f32>(f32(gid.x) + 0.5, f32(gid.y) + 0.5);

    var best_sq_r: f32 = INF_DIST;
    var best_sq_g: f32 = INF_DIST;
    var best_sq_b: f32 = INF_DIST;
    var signed_r: f32 = 0.0;
    var signed_g: f32 = 0.0;
    var signed_b: f32 = 0.0;
    var winding: i32 = 0;

    for (var i = 0u; i < header.edge_count; i = i + 1u) {
        let e = edges[header.edge_offset + i];
        let kind = e.kind & EDGE_KIND_MASK;
        let chan = (e.kind >> EDGE_CHANNEL_MASK_SHIFT) & EDGE_CHANNEL_MASK_BITS;
        let p0 = vec2<f32>(e.p0x, e.p0y);
        let p1 = vec2<f32>(e.p1x, e.p1y);
        let p2 = vec2<f32>(e.p2x, e.p2y);
        let p3 = vec2<f32>(e.p3x, e.p3y);

        var ed: EdgeDist;
        var p_start: vec2<f32>;
        var p_end: vec2<f32>;
        var dir_start: vec2<f32>;
        var dir_end: vec2<f32>;
        var w: i32 = 0;
        if (kind == EDGE_KIND_LINEAR) {
            ed = distance_linear(pt, p0, p1);
            p_start = p0;
            p_end = p1;
            dir_start = p1 - p0;
            dir_end = p1 - p0;
            w = winding_linear(pt, p0, p1);
        } else if (kind == EDGE_KIND_QUADRATIC) {
            ed = distance_quadratic(pt, p0, p1, p2);
            p_start = p0;
            p_end = p2;
            dir_start = p1 - p0;
            dir_end = p2 - p1;
            w = winding_quadratic(pt, p0, p1, p2);
        } else if (kind == EDGE_KIND_CUBIC) {
            ed = distance_cubic(pt, p0, p1, p2, p3);
            p_start = p0;
            p_end = p3;
            dir_start = p1 - p0;
            dir_end = p3 - p2;
            w = winding_cubic(pt, p0, p1, p2, p3);
        } else {
            continue;
        }
        winding = winding + w;
        let edge_signed = signed_pseudo_distance(pt, ed, p_start, p_end, dir_start, dir_end);
        if ((chan & 1u) != 0u && ed.dist_sq < best_sq_r) {
            best_sq_r = ed.dist_sq;
            signed_r = edge_signed;
        }
        if ((chan & 2u) != 0u && ed.dist_sq < best_sq_g) {
            best_sq_g = ed.dist_sq;
            signed_g = edge_signed;
        }
        if ((chan & 4u) != 0u && ed.dist_sq < best_sq_b) {
            best_sq_b = ed.dist_sq;
            signed_b = edge_signed;
        }
    }

    // Global sign reconciliation: if winding-rule "inside" disagrees
    // with the per-channel signs (more than one channel on the wrong
    // side), flip all three to match the winding-rule decision.
    let inside_winding = winding != 0;
    var pos_count: i32 = 0;
    if (signed_r > 0.0) { pos_count = pos_count + 1; }
    if (signed_g > 0.0) { pos_count = pos_count + 1; }
    if (signed_b > 0.0) { pos_count = pos_count + 1; }
    let median_positive = pos_count >= 2;
    if (inside_winding == median_positive) {
        signed_r = -signed_r;
        signed_g = -signed_g;
        signed_b = -signed_b;
    }

    let r = clamp(signed_r / params.sdf_range + 0.5, 0.0, 1.0);
    let g = clamp(signed_g / params.sdf_range + 0.5, 0.0, 1.0);
    let b = clamp(signed_b / params.sdf_range + 0.5, 0.0, 1.0);

    let out_xy = vec2<i32>(
        i32(header.atlas_origin_x + gid.x),
        i32(header.atlas_origin_y + gid.y),
    );
    textureStore(output, out_xy, vec4<f32>(r, g, b, 1.0));
}
