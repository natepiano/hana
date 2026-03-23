// Compute shader reference implementation for scanning a glyph's MSDF
// atlas region to find the tight bounding box of visible pixels
// (where compute_alpha >= 0.02).
//
// NOTE: The actual ink bounds computation is performed CPU-side in
// `MsdfAtlas::scan_ink_bounds_uv()` using bilinear-filtered sampling
// that matches `textureSampleLevel`. The results are written to the
// `MsdfTextUniform` uniform and the fragment shader reads them as
// pre-computed bounds. This file is kept as a GPU reference for the
// algorithm — it is not currently dispatched.

// Input: glyph parameters
struct GlyphParams {
    // UV region of this glyph in the atlas [u_min, v_min, u_max, v_max]
    uv_min: vec2<f32>,
    uv_max: vec2<f32>,
    // SDF parameters needed for compute_alpha
    sdf_range: f32,
    atlas_width: f32,
    atlas_height: f32,
    // screen_px_range computed on the CPU from camera + quad size.
    // Replaces fwidth(uv) which is unavailable in compute shaders.
    screen_px_range: f32,
}

// Output: computed bounding box
struct InkBBox {
    uv_min: vec2<f32>,
    uv_max: vec2<f32>,
}

@group(0) @binding(0) var<uniform> params: GlyphParams;
@group(0) @binding(1) var atlas_texture: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;
@group(0) @binding(3) var<storage, read_write> result: InkBBox;

fn median(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

fn compute_alpha(uv: vec2<f32>) -> f32 {
    let s = textureSampleLevel(atlas_texture, atlas_sampler, uv, 0.0);
    let sd = median(s.r, s.g, s.b) - 0.5;
    // Use CPU-provided screen_px_range (accounts for current zoom level).
    let screen_px_dist = params.screen_px_range * sd;
    return clamp(screen_px_dist + 0.5, 0.0, 1.0);
}

@compute @workgroup_size(1)
fn main() {
    let u_start = params.uv_min.x;
    let v_start = params.uv_min.y;
    let u_end = params.uv_max.x;
    let v_end = params.uv_max.y;

    let du = 0.5 / params.atlas_width;
    let dv = 0.5 / params.atlas_height;

    // Scan inward from each edge to find the bounding box.

    // Left edge
    var box_u_min = u_end;
    var su = u_start;
    loop {
        if su > u_end { break; }
        var sv = v_start;
        var found = false;
        loop {
            if sv > v_end { break; }
            if compute_alpha(vec2<f32>(su, sv)) >= 0.02 {
                found = true;
                break;
            }
            sv += dv;
        }
        if found {
            box_u_min = su;
            break;
        }
        su += du;
    }

    // Right edge
    var box_u_max = u_start;
    su = u_end;
    loop {
        if su < u_start { break; }
        var sv = v_start;
        var found = false;
        loop {
            if sv > v_end { break; }
            if compute_alpha(vec2<f32>(su, sv)) >= 0.02 {
                found = true;
                break;
            }
            sv += dv;
        }
        if found {
            box_u_max = su;
            break;
        }
        su -= du;
    }

    // Top edge
    var box_v_min = v_end;
    var sv2 = v_start;
    loop {
        if sv2 > v_end { break; }
        var su2 = u_start;
        var found = false;
        loop {
            if su2 > u_end { break; }
            if compute_alpha(vec2<f32>(su2, sv2)) >= 0.02 {
                found = true;
                break;
            }
            su2 += du;
        }
        if found {
            box_v_min = sv2;
            break;
        }
        sv2 += dv;
    }

    // Bottom edge
    var box_v_max = v_start;
    sv2 = v_end;
    loop {
        if sv2 < v_start { break; }
        var su2 = u_start;
        var found = false;
        loop {
            if su2 > u_end { break; }
            if compute_alpha(vec2<f32>(su2, sv2)) >= 0.02 {
                found = true;
                break;
            }
            su2 += du;
        }
        if found {
            box_v_max = sv2;
            break;
        }
        sv2 -= dv;
    }

    // Adjust max edges (the scan finds the last visible sample center;
    // the visible edge extends ~0.75 steps past it).
    box_u_max += du * 0.75;
    box_v_max += dv * 0.75;

    result.uv_min = vec2<f32>(box_u_min, box_v_min);
    result.uv_max = vec2<f32>(box_u_max, box_v_max);
}
