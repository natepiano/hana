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

#import bevy_diegetic::gpu_rasterizer::msdf_common::{
    EDGE_KIND_MASK,
    EDGE_KIND_LINEAR,
    EDGE_KIND_QUADRATIC,
    EDGE_KIND_CUBIC,
    EDGE_CHANNEL_MASK_SHIFT,
    EDGE_CHANNEL_MASK_BITS,
    DEGENERATE_EPS,
    INF_DIST,
    EdgeSegment,
    GlyphHeader,
    RasterParams,
    EdgeDist,
    distance_linear,
    distance_quadratic,
    distance_cubic,
    signed_pseudo_distance,
    perp2,
}

@group(0) @binding(0) var<storage, read>  edges:   array<EdgeSegment>;
@group(0) @binding(1) var<storage, read>  glyphs:  array<GlyphHeader>;
@group(0) @binding(2) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<uniform>        params:  RasterParams;

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
    var best_orth_r: f32 = -1.0;
    var best_orth_g: f32 = -1.0;
    var best_orth_b: f32 = -1.0;
    var signed_r: f32 = 0.0;
    var signed_g: f32 = 0.0;
    var signed_b: f32 = 0.0;

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
        if (kind == EDGE_KIND_LINEAR) {
            ed = distance_linear(pt, p0, p1);
            p_start = p0;
            p_end = p1;
            dir_start = p1 - p0;
            dir_end = p1 - p0;
        } else if (kind == EDGE_KIND_QUADRATIC) {
            ed = distance_quadratic(pt, p0, p1, p2);
            p_start = p0;
            p_end = p2;
            dir_start = p1 - p0;
            dir_end = p2 - p1;
        } else if (kind == EDGE_KIND_CUBIC) {
            ed = distance_cubic(pt, p0, p1, p2, p3);
            p_start = p0;
            p_end = p3;
            dir_start = p1 - p0;
            dir_end = p3 - p2;
        } else {
            continue;
        }
        let edge_signed = signed_pseudo_distance(pt, ed, p_start, p_end, dir_start, dir_end);

        // Orthogonality tiebreaker — mirrors fdsm's
        // `Ord for DistanceAndOrthogonality` which orders by
        // `abs(distance_squared)` then by higher orthogonality.
        let tan_len = max(length(ed.tangent), DEGENERATE_EPS);
        let tan_n = ed.tangent / tan_len;
        let pmb = ed.foot - pt;
        let pmb_len = max(length(pmb), DEGENERATE_EPS);
        let pmb_n = pmb / pmb_len;
        let orth = abs(perp2(tan_n, pmb_n));

        let take_r = ed.dist_sq < best_sq_r
            || (ed.dist_sq == best_sq_r && orth > best_orth_r);
        if ((chan & 1u) != 0u && take_r) {
            best_sq_r = ed.dist_sq;
            best_orth_r = orth;
            signed_r = edge_signed;
        }
        let take_g = ed.dist_sq < best_sq_g
            || (ed.dist_sq == best_sq_g && orth > best_orth_g);
        if ((chan & 2u) != 0u && take_g) {
            best_sq_g = ed.dist_sq;
            best_orth_g = orth;
            signed_g = edge_signed;
        }
        let take_b = ed.dist_sq < best_sq_b
            || (ed.dist_sq == best_sq_b && orth > best_orth_b);
        if ((chan & 4u) != 0u && take_b) {
            best_sq_b = ed.dist_sq;
            best_orth_b = orth;
            signed_b = edge_signed;
        }
    }

    // Per-channel pseudo-distances are written as-is. Sign correction
    // happens in `msdf_correct.wgsl` via the truth-override pass, which
    // mirrors fdsm's separate `correct_sign_msdf` stage. A blanket flip
    // here would only swap signs while preserving the small
    // pseudo-distance magnitudes that endpoint-tangent extensions
    // produce at far-away points — leaving values near the 0.5 boundary
    // that render as ghost slivers along extended tangent rays.

    let r = clamp(signed_r / params.sdf_range + 0.5, 0.0, 1.0);
    let g = clamp(signed_g / params.sdf_range + 0.5, 0.0, 1.0);
    let b = clamp(signed_b / params.sdf_range + 0.5, 0.0, 1.0);

    let out_xy = vec2<i32>(
        i32(header.atlas_origin_x + gid.x),
        i32(header.atlas_origin_y + gid.y),
    );
    textureStore(output, out_xy, vec4<f32>(r, g, b, 1.0));
}
