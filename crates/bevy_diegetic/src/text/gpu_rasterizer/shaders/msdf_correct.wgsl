// MSDF error-correction kernel.
//
// Mirrors fdsm's `correct_error_msdf` with `ErrorCorrectionMode::
// EdgePriority` + `DistanceCheckMode::Never`. Each thread is one
// texel of one glyph. Reads its 3×3 neighborhood from the `scratch`
// MSDF texture written by `msdf_gen.wgsl`, decides for itself whether
// it is protected (corner + edge protection), runs the linear and
// diagonal artifact tests against its four axial and four diagonal
// neighbors, and writes either the input texel or a flattened
// `(median, median, median)` to the atlas page when an artifact fires.
//
// Single-pass design: every texel re-derives its own protection state
// from the same data fdsm's two-pass `protect_edges` / `find_errors`
// would have consulted. No cross-thread state is shared, so no
// barriers are needed.

#import bevy_diegetic::gpu_rasterizer::msdf_common::{
    EDGE_KIND_MASK,
    EdgeSegment,
    GlyphHeader,
    RasterParams,
    EdgeDist,
    distance_linear,
    distance_quadratic,
    distance_cubic,
    signed_pseudo_distance,
    winding_linear,
    winding_quadratic,
    winding_cubic,
}

const ARTIFACT_T_EPSILON: f32 = 0.01;
const MIN_DEVIATION_RATIO: f32 = 1.1111111; // 10/9
const PROTECTION_RADIUS_TOLERANCE: f32 = 1.001;
const SQRT2: f32 = 1.4142136;
const MIN_IMPROVE_RATIO: f32 = 1.1111111; // 10/9
const INF_DIST: f32 = 1e30;

struct CornerPoint {
    x: f32,
    y: f32,
}

@group(0) @binding(0) var<storage, read> edges:   array<EdgeSegment>;
@group(0) @binding(1) var<storage, read> glyphs:  array<GlyphHeader>;
@group(0) @binding(2) var scratch:               texture_2d<f32>;
@group(0) @binding(3) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(4) var<uniform>       params:  RasterParams;
@group(0) @binding(5) var<storage, read> corners: array<CornerPoint>;

fn median3(a: f32, b: f32, c: f32) -> f32 {
    return max(min(a, b), min(max(a, b), c));
}

fn median(c: vec3<f32>) -> f32 {
    return median3(c.x, c.y, c.z);
}

fn load_msdf(coord: vec2<i32>) -> vec3<f32> {
    return textureLoad(scratch, coord, 0).rgb;
}

// True if THIS texel is one of the four texels straddling any corner
// recorded for this glyph. fdsm protects (l, b), (r, b), (l, t),
// (r, t) where (l, b) = floor(corner - 0.5).
fn texel_protected_by_corner(local_x: u32, local_y: u32, header: GlyphHeader) -> bool {
    let tx = i32(local_x);
    let ty = i32(local_y);
    for (var i: u32 = 0u; i < header.corner_count; i = i + 1u) {
        let cp = corners[header.corner_offset + i];
        let lf = floor(cp.x - 0.5);
        let bf = floor(cp.y - 0.5);
        let l = i32(lf);
        let b = i32(bf);
        let r = l + 1;
        let t = b + 1;
        if ((tx == l || tx == r) && (ty == b || ty == t)) {
            return true;
        }
    }
    return false;
}

// Returns the channel-mask bits where an edge crosses between two
// texels (`edge_between_texels` from fdsm). For each channel C in
// {R, G, B}: if a[C] − b[C] reverses sign across the texel pair AND
// the interpolated median at the crossing equals that channel, set
// bit C.
fn edge_between_texels_channel(a: vec3<f32>, b: vec3<f32>, channel: u32) -> bool {
    let ac = a[channel];
    let bc = b[channel];
    let denom = ac - bc;
    if (abs(denom) < 1e-12) {
        return false;
    }
    let t = (ac - 0.5) / denom;
    if (t <= 0.0 || t >= 1.0) {
        return false;
    }
    let mixed = mix(a, b, t);
    return abs(median(mixed) - mixed[channel]) < 1e-6;
}

fn edge_between_texels(a: vec3<f32>, b: vec3<f32>) -> u32 {
    var mask: u32 = 0u;
    if (edge_between_texels_channel(a, b, 0u)) { mask = mask | 1u; }
    if (edge_between_texels_channel(a, b, 1u)) { mask = mask | 2u; }
    if (edge_between_texels_channel(a, b, 2u)) { mask = mask | 4u; }
    return mask;
}

// fdsm's `protect_extreme_channels` for THIS texel: if a channel C is
// in the edge-crossing mask AND THIS texel's value for that channel
// differs from THIS texel's median, the texel must be protected.
fn texel_protected_extreme(a: vec3<f32>, am: f32, mask: u32) -> bool {
    if ((mask & 1u) != 0u && a.x != am) { return true; }
    if ((mask & 2u) != 0u && a.y != am) { return true; }
    if ((mask & 4u) != 0u && a.z != am) { return true; }
    return false;
}

// Per-pair protect-edges check: returns true if THIS texel (color `a`,
// median `am`) is protected because it shares an edge with neighbor
// `b` (median `bm`).
fn check_protect_edge(a: vec3<f32>, am: f32, b: vec3<f32>, bm: f32, radius: f32) -> bool {
    if (abs(am - 0.5) + abs(bm - 0.5) >= radius) {
        return false;
    }
    let mask = edge_between_texels(a, b);
    return texel_protected_extreme(a, am, mask);
}

// fdsm's BaseArtifactClassifier::range_test → ARTIFACT bit.
// `at`, `bt` are the parameter values of the two range endpoints;
// `xt` is the parameter at the interpolated sample.
fn range_test_artifact(
    span: f32,
    guarded: bool,
    at: f32,
    bt: f32,
    xt: f32,
    am: f32,
    bm: f32,
    xm: f32,
) -> bool {
    let above = (am > 0.5 && bm > 0.5 && xm <= 0.5);
    let below = (am < 0.5 && bm < 0.5 && xm >= 0.5);
    let disagree = (!guarded) && (median3(am, bm, xm) != xm);
    if (!(above || below || disagree)) {
        return false;
    }
    let ax_span = (xt - at) * span;
    let bx_span = (bt - xt) * span;
    let within = (abs(xm - am) <= ax_span) && (abs(xm - bm) <= bx_span);
    return !within;
}

// has_linear_artifact_inner from fdsm.
fn inner_linear(
    span: f32,
    guarded: bool,
    am: f32,
    bm: f32,
    a: vec3<f32>,
    b: vec3<f32>,
    da: f32,
    db: f32,
) -> bool {
    let denom = da - db;
    if (abs(denom) < 1e-12) {
        return false;
    }
    let t = da / denom;
    if (t <= ARTIFACT_T_EPSILON || t >= (1.0 - ARTIFACT_T_EPSILON)) {
        return false;
    }
    let mixed = mix(a, b, t);
    let xm = median(mixed);
    return range_test_artifact(span, guarded, 0.0, 1.0, t, am, bm, xm);
}

// fdsm has_linear_artifact: only flags the texel farther from 0.5.
fn has_linear_artifact(
    span: f32,
    guarded: bool,
    am: f32,
    a: vec3<f32>,
    b: vec3<f32>,
) -> bool {
    let bm = median(b);
    if (abs(am - 0.5) <= abs(bm - 0.5)) {
        return false;
    }
    return inner_linear(span, guarded, am, bm, a, b, a.y - a.x, b.y - b.x)
        || inner_linear(span, guarded, am, bm, a, b, a.z - a.y, b.z - b.y)
        || inner_linear(span, guarded, am, bm, a, b, a.x - a.z, b.x - b.z);
}

// Roots of `a t² + b t + c = 0` (up to 2). Out-of-range sentinels are
// returned as -1.0 and rejected by the caller's t-range check.
fn solve_quadratic_2(qa: f32, qb: f32, qc: f32) -> array<f32, 2> {
    var out = array<f32, 2>(-1.0, -1.0);
    if (abs(qa) < 1e-12) {
        if (abs(qb) > 1e-12) {
            out[0] = -qc / qb;
        }
        return out;
    }
    let disc = qb * qb - 4.0 * qa * qc;
    if (disc < 0.0) {
        return out;
    }
    let sq = sqrt(disc);
    out[0] = (-qb - sq) / (2.0 * qa);
    out[1] = (-qb + sq) / (2.0 * qa);
    return out;
}

fn interpolated_median_bilinear(a: vec3<f32>, l: vec3<f32>, q: vec3<f32>, t: f32) -> f32 {
    let v0 = t * (t * q.x + l.x) + a.x;
    let v1 = t * (t * q.y + l.y) + a.y;
    let v2 = t * (t * q.z + l.z) + a.z;
    return median3(v0, v1, v2);
}

fn extremum_range_test(
    span: f32,
    guarded: bool,
    am: f32,
    dm: f32,
    a: vec3<f32>,
    l: vec3<f32>,
    q: vec3<f32>,
    t: f32,
    xm: f32,
    t_ex: f32,
) -> bool {
    if (!(t_ex > 0.0 && t_ex < 1.0)) {
        return false;
    }
    let em_at = interpolated_median_bilinear(a, l, q, t_ex);
    var t_end0: f32;
    var t_end1: f32;
    var em0: f32;
    var em1: f32;
    if (t_ex > t) {
        // range = [0, t_ex]
        t_end0 = 0.0;
        t_end1 = t_ex;
        em0 = am;
        em1 = em_at;
    } else {
        // range = [t_ex, 1]
        t_end0 = t_ex;
        t_end1 = 1.0;
        em0 = em_at;
        em1 = dm;
    }
    return range_test_artifact(span, guarded, t_end0, t_end1, t, em0, em1, xm);
}

fn inner_diagonal(
    span: f32,
    guarded: bool,
    am: f32,
    dm: f32,
    a: vec3<f32>,
    l: vec3<f32>,
    q: vec3<f32>,
    da: f32,
    dbc_minus_da: f32,
    dd: f32,
    t_ex0: f32,
    t_ex1: f32,
) -> bool {
    let roots = solve_quadratic_2(dd - dbc_minus_da, dbc_minus_da - da, da);
    var hit = false;
    for (var i: u32 = 0u; i < 2u; i = i + 1u) {
        let t = roots[i];
        if (t <= ARTIFACT_T_EPSILON || t >= (1.0 - ARTIFACT_T_EPSILON)) {
            continue;
        }
        let xm = interpolated_median_bilinear(a, l, q, t);
        var ok = range_test_artifact(span, guarded, 0.0, 1.0, t, am, dm, xm);
        ok = ok || extremum_range_test(span, guarded, am, dm, a, l, q, t, xm, t_ex0);
        ok = ok || extremum_range_test(span, guarded, am, dm, a, l, q, t, xm, t_ex1);
        if (ok) {
            hit = true;
        }
    }
    return hit;
}

fn has_diagonal_artifact(
    span: f32,
    guarded: bool,
    am: f32,
    a: vec3<f32>,
    b: vec3<f32>,
    c: vec3<f32>,
    d: vec3<f32>,
) -> bool {
    let dm = median(d);
    if (abs(am - 0.5) < abs(dm - 0.5)) {
        return false;
    }
    let abc = vec3<f32>(a.x - b.x - c.x, a.y - b.y - c.y, a.z - b.z - c.z);
    let l = vec3<f32>(-a.x - abc.x, -a.y - abc.y, -a.z - abc.z);
    let q = vec3<f32>(d.x + abc.x, d.y + abc.y, d.z + abc.z);
    let t_ex0 = -0.5 * l.x / q.x;
    let t_ex1 = -0.5 * l.y / q.y;
    let t_ex2 = -0.5 * l.z / q.z;
    return inner_diagonal(span, guarded, am, dm, a, l, q, a.y - a.x, abc.x - abc.y, d.y - d.x, t_ex0, t_ex1)
        || inner_diagonal(span, guarded, am, dm, a, l, q, a.z - a.y, abc.y - abc.z, d.z - d.y, t_ex1, t_ex2)
        || inner_diagonal(span, guarded, am, dm, a, l, q, a.x - a.z, abc.z - abc.x, d.x - d.z, t_ex2, t_ex0);
}

// ---------------------------------------------------------------------
// AtEdge distance-check pass helpers. The per-edge distance machinery
// itself lives in `msdf_common.wgsl` (imported at the top of this
// file); only the union-of-edges `true_signed_distance` is
// kept here because it is specific to the correction kernel.
// ---------------------------------------------------------------------


// True signed distance at point `pt` (pixel coords local to the glyph
// bitmap) for the union of all edges. Returns the **unsigned** distance
// to the closest edge, signed by the winding rule. Mirrors fdsm's
// `correct_sign_msdf` reference, which uses true (not pseudo) distance
// so the magnitude is always the actual nearest-point distance — not
// the perpendicular-to-tangent extension that can collapse to near-zero
// for points far from any edge along an endpoint tangent ray, which
// would otherwise produce a ghost boundary along that ray.
fn true_signed_distance(pt: vec2<f32>, header: GlyphHeader) -> f32 {
    var best_sq = INF_DIST;
    var best_orth: f32 = -1.0;
    var winding: i32 = 0;
    for (var i = 0u; i < header.edge_count; i = i + 1u) {
        let e = edges[header.edge_offset + i];
        let kind = e.kind & EDGE_KIND_MASK;
        let p0 = vec2<f32>(e.p0x, e.p0y);
        let p1 = vec2<f32>(e.p1x, e.p1y);
        let p2 = vec2<f32>(e.p2x, e.p2y);
        let p3 = vec2<f32>(e.p3x, e.p3y);
        var ed: EdgeDist;
        var w: i32 = 0;
        if (kind == 0u) {
            ed = distance_linear(pt, p0, p1);
            w = winding_linear(pt, p0, p1);
        } else if (kind == 1u) {
            ed = distance_quadratic(pt, p0, p1, p2);
            w = winding_quadratic(pt, p0, p1, p2);
        } else if (kind == 2u) {
            ed = distance_cubic(pt, p0, p1, p2, p3);
            w = winding_cubic(pt, p0, p1, p2, p3);
        } else {
            continue;
        }
        winding = winding + w;
        let tan_len = max(length(ed.tangent), 1e-20);
        let tan_n = ed.tangent / tan_len;
        let pmb = ed.foot - pt;
        let pmb_len = max(length(pmb), 1e-20);
        let pmb_n = pmb / pmb_len;
        let orth = abs(tan_n.x * pmb_n.y - tan_n.y * pmb_n.x);
        let take = ed.dist_sq < best_sq || (ed.dist_sq == best_sq && orth > best_orth);
        if (take) {
            best_sq = ed.dist_sq;
            best_orth = orth;
        }
    }
    let unsigned_dist = sqrt(best_sq);
    let inside = winding != 0;
    return select(-unsigned_dist, unsigned_dist, inside);
}

// Manual bilinear sample of the scratch MSDF at sub-texel atlas
// position `atlas_p`. The 2×2 support is the four texels straddling
// `atlas_p`; weights come from the fractional offset within that
// 2×2 box.
fn bilinear_msdf(atlas_p: vec2<f32>) -> vec3<f32> {
    let floor_xy = floor(atlas_p - 0.5);
    let frac = atlas_p - 0.5 - floor_xy;
    let x0 = i32(floor_xy.x);
    let y0 = i32(floor_xy.y);
    let p00 = load_msdf(vec2<i32>(x0, y0));
    let p10 = load_msdf(vec2<i32>(x0 + 1, y0));
    let p01 = load_msdf(vec2<i32>(x0, y0 + 1));
    let p11 = load_msdf(vec2<i32>(x0 + 1, y0 + 1));
    let w00 = (1.0 - frac.x) * (1.0 - frac.y);
    let w10 = frac.x * (1.0 - frac.y);
    let w01 = (1.0 - frac.x) * frac.y;
    let w11 = frac.x * frac.y;
    return p00 * w00 + p10 * w10 + p01 * w01 + p11 * w11;
}

// fdsm's ShapeDistanceCheckerArtifactClassifier::evaluate. Returns
// true if flattening THIS texel (center color `c`, median `cm`) to
// `(cm, cm, cm)` brings the bilinear-sampled MSDF closer to the
// true signed pseudo-distance reference at the artifact's sub-texel
// position, by at least `MIN_IMPROVE_RATIO`.
fn distance_check_passes(
    t: f32,
    direction: vec2<f32>,
    c: vec3<f32>,
    cm: f32,
    center_atlas_xy: vec2<i32>,
    center_local_xy: vec2<f32>,
    header: GlyphHeader,
) -> bool {
    let t_vec = t * direction;
    let atlas_p = vec2<f32>(f32(center_atlas_xy.x), f32(center_atlas_xy.y)) + 0.5 + t_vec;
    let local_p = center_local_xy + t_vec;
    let old_msd = bilinear_msdf(atlas_p);
    let a_weight = (1.0 - abs(t_vec.x)) * (1.0 - abs(t_vec.y));
    let a_psd = cm;
    let new_msd = old_msd + a_weight * (vec3<f32>(a_psd) - c);
    let old_psd = median(old_msd);
    let new_psd = median(new_msd);
    let inv_range = 1.0 / params.sdf_range;
    let true_dist = true_signed_distance(local_p, header);
    let ref_psd = inv_range * true_dist + 0.5;
    return MIN_IMPROVE_RATIO * abs(new_psd - ref_psd) < abs(old_psd - ref_psd);
}

// Linear-artifact probe that returns the CANDIDATE `t` value when the
// range_test would have flagged CANDIDATE (sign-flip "above"/"below"
// branches only, since pass 2 runs with all-guarded). Returns -1.0
// when no CANDIDATE fires.
fn linear_candidate_t_inner(
    span: f32,
    am: f32,
    bm: f32,
    a: vec3<f32>,
    b: vec3<f32>,
    da: f32,
    db: f32,
) -> f32 {
    let denom = da - db;
    if (abs(denom) < 1e-12) { return -1.0; }
    let t = da / denom;
    if (t <= ARTIFACT_T_EPSILON || t >= (1.0 - ARTIFACT_T_EPSILON)) { return -1.0; }
    let mixed = mix(a, b, t);
    let xm = median(mixed);
    let above = (am > 0.5 && bm > 0.5 && xm <= 0.5);
    let below = (am < 0.5 && bm < 0.5 && xm >= 0.5);
    if (!(above || below)) { return -1.0; }
    // ARTIFACT condition: span check fails → already artifact, no
    // distance check needed. Otherwise it's a CANDIDATE awaiting
    // distance check.
    let ax_span = t * span;
    let bx_span = (1.0 - t) * span;
    let within = (abs(xm - am) <= ax_span) && (abs(xm - bm) <= bx_span);
    if (!within) {
        // Sentinel: any t below 0 indicates "no candidate";
        // we encode "already artifact" as t + 2 (so caller knows
        // to skip the distance check) — but for simplicity here,
        // pass 1 will already have flagged this texel and pass 2
        // won't even be reached for direct artifacts. Treat as
        // candidate either way.
        return t;
    }
    return t;
}

// Returns the first CANDIDATE t across the three channel-pair tests
// in fdsm's has_linear_artifact, or -1.0 when no candidate fires.
fn linear_candidate_t(
    span: f32,
    am: f32,
    a: vec3<f32>,
    b: vec3<f32>,
) -> f32 {
    let bm = median(b);
    if (abs(am - 0.5) <= abs(bm - 0.5)) { return -1.0; }
    let t0 = linear_candidate_t_inner(span, am, bm, a, b, a.y - a.x, b.y - b.x);
    if (t0 >= 0.0) { return t0; }
    let t1 = linear_candidate_t_inner(span, am, bm, a, b, a.z - a.y, b.z - b.y);
    if (t1 >= 0.0) { return t1; }
    let t2 = linear_candidate_t_inner(span, am, bm, a, b, a.x - a.z, b.x - b.z);
    return t2;
}

// True if the pair (center, neighbor in `direction`) registers a
// linear CANDIDATE and the distance check confirms it as an artifact.
fn has_linear_artifact_with_check(
    span: f32,
    cm: f32,
    c: vec3<f32>,
    n: vec3<f32>,
    direction: vec2<f32>,
    center_atlas_xy: vec2<i32>,
    center_local_xy: vec2<f32>,
    header: GlyphHeader,
) -> bool {
    let t = linear_candidate_t(span, cm, c, n);
    if (t < 0.0) { return false; }
    return distance_check_passes(t, direction, c, cm, center_atlas_xy, center_local_xy, header);
}

// Diagonal CANDIDATE probe: returns the t value where the diagonal
// artifact would fire, or -1.0 when no CANDIDATE fires. Pass 2 runs
// with all-guarded so only the sign-flip "above"/"below" branches
// produce CANDIDATEs.
fn diagonal_candidate_t_inner(
    am: f32,
    dm: f32,
    a: vec3<f32>,
    l: vec3<f32>,
    q: vec3<f32>,
    da: f32,
    dbc_minus_da: f32,
    dd: f32,
    t_ex0: f32,
    t_ex1: f32,
) -> f32 {
    let roots = solve_quadratic_2(dd - dbc_minus_da, dbc_minus_da - da, da);
    for (var i: u32 = 0u; i < 2u; i = i + 1u) {
        let t = roots[i];
        if (t <= ARTIFACT_T_EPSILON || t >= (1.0 - ARTIFACT_T_EPSILON)) { continue; }
        let xm = interpolated_median_bilinear(a, l, q, t);
        let above = (am > 0.5 && dm > 0.5 && xm <= 0.5);
        let below = (am < 0.5 && dm < 0.5 && xm >= 0.5);
        if (above || below) { return t; }
        for (var k: u32 = 0u; k < 2u; k = k + 1u) {
            let t_ex = select(t_ex1, t_ex0, k == 0u);
            if (t_ex > 0.0 && t_ex < 1.0) {
                let em_at = interpolated_median_bilinear(a, l, q, t_ex);
                var em0: f32;
                var em1: f32;
                if (t_ex > t) {
                    em0 = am;
                    em1 = em_at;
                } else {
                    em0 = em_at;
                    em1 = dm;
                }
                let above_ex = (em0 > 0.5 && em1 > 0.5 && xm <= 0.5);
                let below_ex = (em0 < 0.5 && em1 < 0.5 && xm >= 0.5);
                if (above_ex || below_ex) { return t; }
            }
        }
    }
    return -1.0;
}

fn diagonal_candidate_t(
    am: f32,
    a: vec3<f32>,
    b: vec3<f32>,
    cc: vec3<f32>,
    d: vec3<f32>,
) -> f32 {
    let dm = median(d);
    if (abs(am - 0.5) < abs(dm - 0.5)) { return -1.0; }
    let abc = vec3<f32>(a.x - b.x - cc.x, a.y - b.y - cc.y, a.z - b.z - cc.z);
    let l = vec3<f32>(-a.x - abc.x, -a.y - abc.y, -a.z - abc.z);
    let q = vec3<f32>(d.x + abc.x, d.y + abc.y, d.z + abc.z);
    let t_ex0 = -0.5 * l.x / q.x;
    let t_ex1 = -0.5 * l.y / q.y;
    let t_ex2 = -0.5 * l.z / q.z;
    let t0 = diagonal_candidate_t_inner(am, dm, a, l, q, a.y - a.x, abc.x - abc.y, d.y - d.x, t_ex0, t_ex1);
    if (t0 >= 0.0) { return t0; }
    let t1 = diagonal_candidate_t_inner(am, dm, a, l, q, a.z - a.y, abc.y - abc.z, d.z - d.y, t_ex1, t_ex2);
    if (t1 >= 0.0) { return t1; }
    let t2 = diagonal_candidate_t_inner(am, dm, a, l, q, a.x - a.z, abc.z - abc.x, d.x - d.z, t_ex2, t_ex0);
    return t2;
}

fn has_diagonal_artifact_with_check(
    cm: f32,
    c: vec3<f32>,
    b1: vec3<f32>,
    b2: vec3<f32>,
    d: vec3<f32>,
    direction: vec2<f32>,
    center_atlas_xy: vec2<i32>,
    center_local_xy: vec2<f32>,
    header: GlyphHeader,
) -> bool {
    let t = diagonal_candidate_t(cm, c, b1, b2, d);
    if (t < 0.0) { return false; }
    return distance_check_passes(t, direction, c, cm, center_atlas_xy, center_local_xy, header);
}

@compute @workgroup_size(8, 8, 1)
fn msdf_correct_main(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(workgroup_id) wg: vec3<u32>,
) {
    let glyph_idx = wg.z;
    if (glyph_idx >= params.glyph_count) {
        return;
    }
    let header = glyphs[glyph_idx];
    if (gid.x >= header.bitmap_w || gid.y >= header.bitmap_h) {
        return;
    }

    let atlas_xy = vec2<i32>(
        i32(header.atlas_origin_x + gid.x),
        i32(header.atlas_origin_y + gid.y),
    );
    let c = load_msdf(atlas_xy);
    let cm = median(c);

    let inv_range = 1.0 / params.sdf_range;
    let radius = PROTECTION_RADIUS_TOLERANCE * inv_range;
    let hspan = MIN_DEVIATION_RATIO * inv_range;
    let dspan = hspan * SQRT2;

    let xmax = i32(header.bitmap_w) - 1;
    let ymax = i32(header.bitmap_h) - 1;
    let lx = i32(gid.x);
    let ly = i32(gid.y);
    let has_l = lx > 0;
    let has_r = lx < xmax;
    let has_b = ly > 0;
    let has_t = ly < ymax;

    let ax0 = atlas_xy.x;
    let ay0 = atlas_xy.y;

    var nl = vec3<f32>(0.0);
    var nr = vec3<f32>(0.0);
    var nb = vec3<f32>(0.0);
    var nt = vec3<f32>(0.0);
    var nlb = vec3<f32>(0.0);
    var nrb = vec3<f32>(0.0);
    var nlt = vec3<f32>(0.0);
    var nrt = vec3<f32>(0.0);
    if (has_l) { nl = load_msdf(vec2<i32>(ax0 - 1, ay0)); }
    if (has_r) { nr = load_msdf(vec2<i32>(ax0 + 1, ay0)); }
    if (has_b) { nb = load_msdf(vec2<i32>(ax0, ay0 - 1)); }
    if (has_t) { nt = load_msdf(vec2<i32>(ax0, ay0 + 1)); }
    if (has_l && has_b) { nlb = load_msdf(vec2<i32>(ax0 - 1, ay0 - 1)); }
    if (has_r && has_b) { nrb = load_msdf(vec2<i32>(ax0 + 1, ay0 - 1)); }
    if (has_l && has_t) { nlt = load_msdf(vec2<i32>(ax0 - 1, ay0 + 1)); }
    if (has_r && has_t) { nrt = load_msdf(vec2<i32>(ax0 + 1, ay0 + 1)); }

    var guarded = texel_protected_by_corner(gid.x, gid.y, header);

    if (!guarded && has_l) {
        let m = median(nl);
        if (check_protect_edge(c, cm, nl, m, radius)) { guarded = true; }
    }
    if (!guarded && has_r) {
        let m = median(nr);
        if (check_protect_edge(c, cm, nr, m, radius)) { guarded = true; }
    }
    if (!guarded && has_b) {
        let m = median(nb);
        if (check_protect_edge(c, cm, nb, m, radius)) { guarded = true; }
    }
    if (!guarded && has_t) {
        let m = median(nt);
        if (check_protect_edge(c, cm, nt, m, radius)) { guarded = true; }
    }
    if (!guarded && has_l && has_b) {
        let m = median(nlb);
        if (check_protect_edge(c, cm, nlb, m, radius)) { guarded = true; }
    }
    if (!guarded && has_r && has_t) {
        let m = median(nrt);
        if (check_protect_edge(c, cm, nrt, m, radius)) { guarded = true; }
    }
    if (!guarded && has_l && has_t) {
        let m = median(nlt);
        if (check_protect_edge(c, cm, nlt, m, radius)) { guarded = true; }
    }
    if (!guarded && has_r && has_b) {
        let m = median(nrb);
        if (check_protect_edge(c, cm, nrb, m, radius)) { guarded = true; }
    }

    var error = false;
    if (has_l && has_linear_artifact(hspan, guarded, cm, c, nl)) { error = true; }
    if (!error && has_r && has_linear_artifact(hspan, guarded, cm, c, nr)) { error = true; }
    if (!error && has_b && has_linear_artifact(hspan, guarded, cm, c, nb)) { error = true; }
    if (!error && has_t && has_linear_artifact(hspan, guarded, cm, c, nt)) { error = true; }
    if (!error && has_l && has_b && has_diagonal_artifact(dspan, guarded, cm, c, nl, nb, nlb)) { error = true; }
    if (!error && has_l && has_t && has_diagonal_artifact(dspan, guarded, cm, c, nl, nt, nlt)) { error = true; }
    if (!error && has_r && has_b && has_diagonal_artifact(dspan, guarded, cm, c, nr, nb, nrb)) { error = true; }
    if (!error && has_r && has_t && has_diagonal_artifact(dspan, guarded, cm, c, nr, nt, nrt)) { error = true; }

    // Pass 2: fdsm's AtEdge distance-check pass. Runs only when the
    // first pass did not already mark this texel as an error; treats
    // every neighbor pair as if all texels are guarded (so the
    // sign-flip "above"/"below" CANDIDATEs are the only path) and
    // confirms each CANDIDATE with the true outline distance.
    if (!error) {
        let local_pt = vec2<f32>(f32(gid.x) + 0.5, f32(gid.y) + 0.5);
        if (has_l && has_linear_artifact_with_check(
            hspan, cm, c, nl, vec2<f32>(-1.0, 0.0), atlas_xy, local_pt, header)) { error = true; }
        if (!error && has_r && has_linear_artifact_with_check(
            hspan, cm, c, nr, vec2<f32>(1.0, 0.0), atlas_xy, local_pt, header)) { error = true; }
        if (!error && has_b && has_linear_artifact_with_check(
            hspan, cm, c, nb, vec2<f32>(0.0, -1.0), atlas_xy, local_pt, header)) { error = true; }
        if (!error && has_t && has_linear_artifact_with_check(
            hspan, cm, c, nt, vec2<f32>(0.0, 1.0), atlas_xy, local_pt, header)) { error = true; }
        if (!error && has_l && has_b && has_diagonal_artifact_with_check(
            cm, c, nl, nb, nlb, vec2<f32>(-1.0, -1.0), atlas_xy, local_pt, header)) { error = true; }
        if (!error && has_l && has_t && has_diagonal_artifact_with_check(
            cm, c, nl, nt, nlt, vec2<f32>(-1.0, 1.0), atlas_xy, local_pt, header)) { error = true; }
        if (!error && has_r && has_b && has_diagonal_artifact_with_check(
            cm, c, nr, nb, nrb, vec2<f32>(1.0, -1.0), atlas_xy, local_pt, header)) { error = true; }
        if (!error && has_r && has_t && has_diagonal_artifact_with_check(
            cm, c, nr, nt, nrt, vec2<f32>(1.0, 1.0), atlas_xy, local_pt, header)) { error = true; }
    }

    // Sign correction — mirror of fdsm's `correct_sign_msdf`. When the
    // median's side disagrees with the non-zero winding rule, flip all
    // three channels (`1 - c`). Flipping preserves per-channel
    // disagreement, which is what keeps corners sharp; flattening to
    // `(truth, truth, truth)` would destroy the MSDF structure and
    // round corners.
    let local_pt_center = vec2<f32>(f32(gid.x) + 0.5, f32(gid.y) + 0.5);
    let true_dist = true_signed_distance(local_pt_center, header);
    let median_inside = cm > 0.5;
    let truth_inside = true_dist > 0.0;
    let sign_disagree = median_inside != truth_inside;

    var out_rgb = c;
    if (sign_disagree) {
        out_rgb = vec3<f32>(1.0) - c;
    } else if (error) {
        out_rgb = vec3<f32>(cm, cm, cm);
    }

    // Alpha channel: MSDF mode hardcodes 1.0 (channel ignored by the
    // text fragment shader). MTSDF mode encodes the signed true
    // distance the same way RGB channels are encoded — `clamp(d /
    // range + 0.5, 0, 1)` — so the fragment shader can clamp the RGB
    // median to ±tolerance around the alpha value. This is what stops
    // per-channel MSDF comb from escaping in narrow features (CJK,
    // ornate Latin). The signed form is needed so the fragment shader
    // knows which side of the edge each texel is on.
#ifdef MTSDF
    let out_alpha = clamp(true_dist / params.sdf_range + 0.5, 0.0, 1.0);
#else
    let out_alpha = 1.0;
#endif
    textureStore(output, atlas_xy, vec4<f32>(out_rgb, out_alpha));
}
