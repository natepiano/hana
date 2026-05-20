// Shared math + struct definitions for the GPU glyph rasterizer
// kernels. Imported by `sdf_gen.wgsl`, `msdf_gen.wgsl`, and
// `msdf_correct.wgsl` so the per-edge distance machinery (bezier
// evaluators, closed-form distance solvers, signed pseudo-distance,
// winding-rule sign reconciliation) lives in one place.

#define_import_path bevy_diegetic::gpu_rasterizer::msdf_common

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
    corner_offset:  u32,
    corner_count:   u32,
}

struct RasterParams {
    sdf_range:      f32,
    padding_texels: u32,
    distance_field: u32,
    glyph_count:    u32,
}

// Per-edge true-distance result for the MSDF / signed-pseudo-distance
// pipeline. `param` is the unclamped curve parameter — when it lies
// outside [0, 1] the closest foot is at an endpoint and the caller
// switches to the pseudo-distance branch.
struct EdgeDist {
    dist_sq: f32,
    param:   f32,
    foot:    vec2<f32>,
    tangent: vec2<f32>,
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

fn bezier_cubic_deriv2(t: f32, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> vec2<f32> {
    let one_minus = 1.0 - t;
    return 6.0 * one_minus * (p2 - 2.0 * p1 + p0) + 6.0 * t * (p3 - 2.0 * p2 + p1);
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

// 2D perp / cross product: `a.x * b.y - a.y * b.x`.
fn perp2(a: vec2<f32>, b: vec2<f32>) -> f32 {
    return a.x * b.y - a.y * b.x;
}

fn distance_linear(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>) -> EdgeDist {
    let d = p1 - p0;
    let len_sq = max(dot(d, d), DEGENERATE_EPS);
    let t_raw = dot(pt - p0, d) / len_sq;
    let t_c = clamp(t_raw, 0.0, 1.0);
    // When t_c clamps to an endpoint, use the stored endpoint directly so
    // sibling segments sharing a corner produce bit-exact equal foot/dist_sq
    // — `p0 + (p1 - p0) * 1.0` is not guaranteed equal to `p1` in f32.
    var foot: vec2<f32>;
    if (t_c <= 0.0) {
        foot = p0;
    } else if (t_c >= 1.0) {
        foot = p1;
    } else {
        foot = p0 + t_c * d;
    }
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
    var foot: vec2<f32>;
    var dist_sq: f32;
    if (t_c <= 0.0) {
        foot = p0;
        let diff = pt - p0;
        dist_sq = dot(diff, diff);
    } else if (t_c >= 1.0) {
        foot = p2;
        let diff = pt - p2;
        dist_sq = dot(diff, diff);
    } else {
        foot = bezier_quadratic(t_c, p0, p1, p2);
        dist_sq = best_sq;
    }
    let tangent = bezier_quadratic_deriv(t_c, p0, p1, p2);
    var out: EdgeDist;
    out.dist_sq = dist_sq;
    out.param = best_t;
    out.foot = foot;
    out.tangent = tangent;
    return out;
}

fn distance_cubic(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> EdgeDist {
    let pv = p0 - pt;
    let pv1 = p1 - p0;
    let pv3_end = p3 - p2;
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
    var foot: vec2<f32>;
    var dist_sq: f32;
    if (t_c <= 0.0) {
        foot = p0;
        let diff = pt - p0;
        dist_sq = dot(diff, diff);
    } else if (t_c >= 1.0) {
        foot = p3;
        let diff = pt - p3;
        dist_sq = dot(diff, diff);
    } else {
        foot = bezier_cubic(t_c, p0, p1, p2, p3);
        dist_sq = best_sq;
    }
    let tangent = bezier_cubic_deriv(t_c, p0, p1, p2, p3);
    var out: EdgeDist;
    out.dist_sq = dist_sq;
    out.param = best_t;
    out.foot = foot;
    out.tangent = tangent;
    return out;
}

// Signed pseudo-distance for one edge — fdsm's
// `Segment::distance_to_pseudodistance`. When the foot is at an
// endpoint and the point lies past it, returns the perpendicular
// distance to the tangent line at that endpoint; otherwise the
// clamped-foot signed distance.
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

// Linear scanline crossing — mirrors fdsm's
// `PreparedLinearSegment::get_scanline_points`. Convention: include the
// LOWER-y endpoint, exclude the HIGHER-y endpoint, so two consecutive
// segments sharing a corner exactly on `pt.y` count the corner once.
fn winding_linear(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>) -> i32 {
    let off_y = p1.y - p0.y;
    let dy = pt.y - p0.y;
    let in_range = (dy >= 0.0 && dy < off_y) || (dy >= off_y && dy < 0.0);
    if (!in_range) { return 0; }
    if (abs(off_y) < 1e-20) { return 0; }
    let t = dy / off_y;
    let x = p0.x + t * (p1.x - p0.x);
    if (x < pt.x) { return 0; }
    return select(-1, 1, off_y > 0.0);
}

// Quadratic scanline winding — direct port of fdsm's
// `PreparedQuadraticSegment::get_scanline_points`, then summed across
// crossings with `xs >= pt.x`. The state machine (`next_dy` + endpoint
// flags + final fixup) is what makes endpoint-tangent grazes and
// shared-corner crossings count correctly — the naive
// "every real root in [0, 1] is a crossing" approach silently
// double-counts tangent endpoints and produced the upper-terminal spur
// on serif `S` glyphs (Crimson Text, Noto Sans).
fn winding_quadratic(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> i32 {
    var xs: array<f32, 2> = array<f32, 2>(0.0, 0.0);
    var deltas: array<i32, 2> = array<i32, 2>(0, 0);
    var total: u32 = 0u;
    var next_dy: i32 = select(-1, 1, pt.y > p0.y);

    let ab_y = p1.y - p0.y;
    let br_y = p2.y - p1.y - ab_y;
    let ab_x = p1.x - p0.x;
    let br_x = p2.x - p1.x - ab_x;

    xs[0] = p0.x;
    if (p0.y == pt.y) {
        let p0_is_min = (p0.y < p1.y) || (p0.y == p1.y && p0.y < p2.y);
        if (p0_is_min) {
            deltas[0] = 1;
            total = 1u;
        } else {
            next_dy = 1;
        }
    }

    // Solve br_y * t² + 2*ab_y * t + (p0.y - pt.y) = 0.
    var sol0: f32 = -1.0;
    var sol1: f32 = -1.0;
    var n_sol: u32 = 0u;
    let qa = br_y;
    let qb = 2.0 * ab_y;
    let qc = p0.y - pt.y;
    if (abs(qa) < 1e-12) {
        if (abs(qb) > 1e-20) {
            sol0 = -qc / qb;
            n_sol = 1u;
        }
    } else {
        let disc = qb * qb - 4.0 * qa * qc;
        if (disc >= 0.0) {
            let sq = sqrt(disc);
            let inv2a = 0.5 / qa;
            let r0 = (-qb - sq) * inv2a;
            let r1 = (-qb + sq) * inv2a;
            sol0 = min(r0, r1);
            sol1 = max(r0, r1);
            n_sol = 2u;
        }
    }

    for (var i: u32 = 0u; i < n_sol; i = i + 1u) {
        if (total >= 2u) { break; }
        let t = select(sol1, sol0, i == 0u);
        if (t >= 0.0 && t <= 1.0) {
            let x_at_t = p0.x + 2.0 * t * ab_x + t * t * br_x;
            let dy_half = ab_y + t * br_y;
            if (f32(next_dy) * dy_half >= 0.0) {
                xs[total] = x_at_t;
                deltas[total] = next_dy;
                next_dy = -next_dy;
                total = total + 1u;
            }
        }
    }

    if (p2.y == pt.y) {
        if (next_dy > 0 && total > 0u) {
            total = total - 1u;
            next_dy = -1;
        }
        let p2_is_min = (p2.y < p1.y) || (p2.y == p1.y && p2.y < p0.y);
        if (p2_is_min && total < 2u) {
            xs[total] = p2.x;
            if (next_dy < 0) {
                deltas[total] = -1;
                next_dy = 1;
                total = total + 1u;
            }
        }
    }

    let expected_exit = select(-1, 1, pt.y >= p2.y);
    if (next_dy != expected_exit) {
        if (total > 0u) {
            total = total - 1u;
        } else {
            var x_pick = p0.x;
            if (abs(p2.y - pt.y) < abs(p0.y - pt.y)) {
                x_pick = p2.x;
            }
            xs[total] = x_pick;
            deltas[total] = next_dy;
            total = total + 1u;
        }
    }

    var winding: i32 = 0;
    for (var i: u32 = 0u; i < total; i = i + 1u) {
        if (xs[i] >= pt.x) {
            winding = winding + deltas[i];
        }
    }
    return winding;
}

// Cubic scanline winding — direct port of fdsm's
// `PreparedCubicSegment::get_scanline_points`. Three solutions max;
// otherwise the state machine is the same as the quadratic.
fn winding_cubic(pt: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> i32 {
    var xs: array<f32, 3> = array<f32, 3>(0.0, 0.0, 0.0);
    var deltas: array<i32, 3> = array<i32, 3>(0, 0, 0);
    var total: u32 = 0u;
    var next_dy: i32 = select(-1, 1, pt.y > p0.y);

    let ab_y = p1.y - p0.y;
    let v12_y = p2.y - p1.y;
    let br_y = v12_y - ab_y;
    let as_y = (p3.y - p2.y) - v12_y - br_y;

    let ab_x = p1.x - p0.x;
    let v12_x = p2.x - p1.x;
    let br_x = v12_x - ab_x;
    let as_x = (p3.x - p2.x) - v12_x - br_x;

    xs[0] = p0.x;
    if (p0.y == pt.y) {
        let p0_is_min =
            (p0.y < p1.y)
            || (p0.y == p1.y && p0.y < p2.y)
            || (p0.y == p1.y && p0.y == p2.y && p0.y < p3.y);
        if (p0_is_min) {
            deltas[0] = 1;
            total = 1u;
        } else {
            next_dy = 1;
        }
    }

    // Solve as_y * t³ + 3*br_y * t² + 3*ab_y * t + (p0.y - pt.y) = 0.
    var sols: array<f32, 3> = array<f32, 3>(-1.0, -1.0, -1.0);
    var n_sol: u32 = 0u;
    let ay = as_y;
    let by = 3.0 * br_y;
    let cy = 3.0 * ab_y;
    let dy_c = p0.y - pt.y;
    if (abs(ay) < 1e-12) {
        // Degenerate cubic: quadratic in t.
        if (abs(by) < 1e-12) {
            if (abs(cy) > 1e-20) {
                sols[0] = -dy_c / cy;
                n_sol = 1u;
            }
        } else {
            let disc = cy * cy - 4.0 * by * dy_c;
            if (disc >= 0.0) {
                let sq = sqrt(disc);
                let inv2b = 0.5 / by;
                let r0 = (-cy - sq) * inv2b;
                let r1 = (-cy + sq) * inv2b;
                sols[0] = min(r0, r1);
                sols[1] = max(r0, r1);
                n_sol = 2u;
            }
        }
    } else {
        let inv_a = 1.0 / ay;
        var roots: array<f32, 3>;
        let nr = solve_cubic_normed(by * inv_a, cy * inv_a, dy_c * inv_a, &roots);
        // Sort up to 3 roots ascending.
        if (nr == 3u) {
            if (roots[0] > roots[1]) {
                let tmp = roots[0]; roots[0] = roots[1]; roots[1] = tmp;
            }
            if (roots[1] > roots[2]) {
                let tmp = roots[1]; roots[1] = roots[2]; roots[2] = tmp;
            }
            if (roots[0] > roots[1]) {
                let tmp = roots[0]; roots[0] = roots[1]; roots[1] = tmp;
            }
        }
        for (var k: u32 = 0u; k < nr; k = k + 1u) {
            sols[k] = roots[k];
        }
        n_sol = nr;
    }

    for (var i: u32 = 0u; i < n_sol; i = i + 1u) {
        if (total >= 3u) { break; }
        let t = sols[i];
        if (t >= 0.0 && t <= 1.0) {
            let x_at_t = p0.x + 3.0 * t * ab_x + 3.0 * t * t * br_x + t * t * t * as_x;
            let dy_third = ab_y + 2.0 * t * br_y + t * t * as_y;
            if (f32(next_dy) * dy_third >= 0.0) {
                xs[total] = x_at_t;
                deltas[total] = next_dy;
                next_dy = -next_dy;
                total = total + 1u;
            }
        }
    }

    if (p3.y == pt.y) {
        if (next_dy > 0 && total > 0u) {
            total = total - 1u;
            next_dy = -1;
        }
        let p3_is_min =
            (p3.y < p2.y)
            || (p3.y == p2.y && p3.y < p1.y)
            || (p3.y == p2.y && p3.y == p1.y && p3.y < p0.y);
        if (p3_is_min && total < 3u) {
            xs[total] = p3.x;
            if (next_dy < 0) {
                deltas[total] = -1;
                next_dy = 1;
                total = total + 1u;
            }
        }
    }

    let expected_exit = select(-1, 1, pt.y >= p3.y);
    if (next_dy != expected_exit) {
        if (total > 0u) {
            total = total - 1u;
        } else {
            var x_pick = p0.x;
            if (abs(p3.y - pt.y) < abs(p0.y - pt.y)) {
                x_pick = p3.x;
            }
            xs[total] = x_pick;
            deltas[total] = next_dy;
            total = total + 1u;
        }
    }

    var winding: i32 = 0;
    for (var i: u32 = 0u; i < total; i = i + 1u) {
        if (xs[i] >= pt.x) {
            winding = winding + deltas[i];
        }
    }
    return winding;
}
