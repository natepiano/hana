// MSDF Text Fragment Shader — MaterialExtension for StandardMaterial
//
// Renders glyphs from an MSDF atlas texture. Uses the median-of-three
// technique for clean edges at any scale, with adaptive anti-aliasing
// based on screen pixel range.
//
// Supports three render modes:
//   0 = Text       — normal MSDF alpha (smooth text edges)
//   1 = PunchOut   — inverted MSDF alpha (background with text cut out)
//   2 = SolidQuad  — full opacity within glyph bounds
//
// Shadow proxy mode (is_shadow_proxy = 1): the mesh is invisible in the
// main pass (all fragments discarded) but contributes shaped shadows via
// the prepass using AlphaMode::Mask. The prepass only needs the alpha
// test (discard or not) — depth is written by the hardware automatically.
//
// Extends StandardMaterial's PBR pipeline: all lighting, shadows, and
// double-sided normal handling come from the base material.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::VertexOutput,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

// Render mode constants — must match GlyphRenderMode enum discriminants.
// Invisible (0) never reaches the shader — the renderer skips the visible mesh.
const RENDER_MODE_TEXT: u32       = 1u;
const RENDER_MODE_PUNCH_OUT: u32  = 2u;
const RENDER_MODE_SOLID_QUAD: u32 = 3u;

struct MsdfTextUniform {
    sdf_range: f32,
    atlas_width: f32,
    atlas_height: f32,
    hue_offset: f32,
    render_mode: u32,
    is_shadow_proxy: u32,
    // Ink bounding box in UV space [u_min, v_min, u_max, v_max].
    // When all non-zero, the shader draws a 1px yellow rectangle at
    // these UV coordinates. Used by the typography overlay.
    ink_uv_min: vec2<f32>,
    ink_uv_max: vec2<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> msdf: MsdfTextUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var msdf_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var msdf_sampler: sampler;

/// Rotates a color's hue by the given angle in radians.
fn rotate_hue(color: vec3<f32>, angle: f32) -> vec3<f32> {
    let k = vec3<f32>(0.57735, 0.57735, 0.57735);
    let cos_a = cos(angle);
    let sin_a = sin(angle);
    return color * cos_a + cross(k, color) * sin_a + k * dot(k, color) * (1.0 - cos_a);
}

fn median(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

fn screen_px_range(uv: vec2<f32>) -> f32 {
    let unit_range = vec2<f32>(
        msdf.sdf_range / msdf.atlas_width,
        msdf.sdf_range / msdf.atlas_height,
    );
    let screen_tex_size = vec2<f32>(1.0) / fwidth(uv);
    return max(
        0.5 * dot(unit_range, screen_tex_size),
        1.0,
    );
}

/// Returns the median SDF value at the given UV coordinate.
fn sample_median(sample_uv: vec2<f32>) -> f32 {
    let s = textureSample(msdf_texture, msdf_sampler, sample_uv);
    return median(s.r, s.g, s.b);
}

/// Scans the glyph's atlas region to find the bounding box of visible
/// pixels, then returns true if the current fragment is on that
/// bounding box boundary. Expensive but exact.
///
/// `ink_uv_min`/`ink_uv_max` define the atlas region to scan (the full
/// glyph bitmap UV extent).
fn is_on_ink_box(uv: vec2<f32>) -> bool {
    let u_start = msdf.ink_uv_min.x;
    let v_start = msdf.ink_uv_min.y;
    let u_end = msdf.ink_uv_max.x;
    let v_end = msdf.ink_uv_max.y;

    // Step size: half a texel to catch sub-texel boundaries.
    let du = 0.5 / msdf.atlas_width;
    let dv = 0.5 / msdf.atlas_height;

    // Find the bounding box by scanning inward from each edge.
    // Much faster than scanning the entire bitmap — O(width+height)
    // instead of O(width*height).

    // Left edge: scan columns left-to-right, find first with any visible pixel.
    var box_u_min = u_end;
    var su = u_start;
    loop {
        if su > u_end { break; }
        var sv2 = v_start;
        var found = false;
        loop {
            if sv2 > v_end { break; }
            if compute_alpha(vec2<f32>(su, sv2)) >= 0.02 {
                found = true;
                break;
            }
            sv2 += dv;
        }
        if found {
            box_u_min = su;
            break;
        }
        su += du;
    }

    // Right edge: scan columns right-to-left.
    var box_u_max = u_start;
    su = u_end;
    loop {
        if su < u_start { break; }
        var sv2 = v_start;
        var found = false;
        loop {
            if sv2 > v_end { break; }
            if compute_alpha(vec2<f32>(su, sv2)) >= 0.02 {
                found = true;
                break;
            }
            sv2 += dv;
        }
        if found {
            box_u_max = su;
            break;
        }
        su -= du;
    }

    // Top edge: scan rows top-to-bottom.
    var box_v_min = v_end;
    var sv = v_start;
    loop {
        if sv > v_end { break; }
        var su2 = u_start;
        var found = false;
        loop {
            if su2 > u_end { break; }
            if compute_alpha(vec2<f32>(su2, sv)) >= 0.02 {
                found = true;
                break;
            }
            su2 += du;
        }
        if found {
            box_v_min = sv;
            break;
        }
        sv += dv;
    }

    // Bottom edge: scan rows bottom-to-top.
    var box_v_max = v_start;
    sv = v_end;
    loop {
        if sv < v_start { break; }
        var su2 = u_start;
        var found = false;
        loop {
            if su2 > u_end { break; }
            if compute_alpha(vec2<f32>(su2, sv)) >= 0.02 {
                found = true;
                break;
            }
            su2 += du;
        }
        if found {
            box_v_max = sv;
            break;
        }
        sv -= dv;
    }

    // No visible pixels found.
    if box_u_max < box_u_min {
        return false;
    }

    // The scan finds the last sample point where alpha >= threshold.
    // The visible edge extends to the next sample point.
    box_u_max += du * 0.75;
    box_v_max += dv * 0.75;

    // Half-pixel width in UV space for a ~1px screen line.
    let line_du = fwidth(uv.x) * 0.5;
    let line_dv = fwidth(uv.y) * 0.5;

    // Check if inside the box region (with margin).
    let inside_u = uv.x >= box_u_min - line_du && uv.x <= box_u_max + line_du;
    let inside_v = uv.y >= box_v_min - line_dv && uv.y <= box_v_max + line_dv;

    if !inside_u || !inside_v {
        return false;
    }

    // On the left or right edge.
    let on_lr = abs(uv.x - box_u_min) < line_du || abs(uv.x - box_u_max) < line_du;
    // On the top or bottom edge.
    let on_tb = abs(uv.y - box_v_min) < line_dv || abs(uv.y - box_v_max) < line_dv;

    return on_lr || on_tb;
}

/// Computes the final alpha based on the render mode.
fn compute_alpha(uv: vec2<f32>) -> f32 {
    if msdf.render_mode == RENDER_MODE_SOLID_QUAD {
        return 1.0;
    }

    let msdf_sample = textureSample(msdf_texture, msdf_sampler, uv);
    let sd = median(msdf_sample.r, msdf_sample.g, msdf_sample.b) - 0.5;
    let screen_px_dist = screen_px_range(uv) * sd;
    let msdf_alpha = clamp(screen_px_dist + 0.5, 0.0, 1.0);

    if msdf.render_mode == RENDER_MODE_PUNCH_OUT {
        return 1.0 - msdf_alpha;
    }

    // RENDER_MODE_TEXT (default).
    return msdf_alpha;
}

// ── Prepass entry point (shadow maps, depth prepass) ──────────────────
//
// Shadow maps only need depth + discard. No return value needed — the
// hardware writes depth automatically for non-discarded fragments.
// FragmentOutput is conditionally compiled behind PREPASS_FRAGMENT and
// is not available for plain shadow passes.

#ifdef PREPASS_PIPELINE
@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) {
    // Only shadow proxies need MSDF decode in the prepass. Non-proxy
    // meshes pass through without texture sampling — the full quad
    // writes depth, producing a rectangular shadow (SolidQuad behavior).
    if msdf.is_shadow_proxy == 1u {
        let final_alpha = compute_alpha(in.uv);
        if final_alpha < 0.5 {
            discard;
        }
    }
}
#else

// ── Main pass entry point (forward rendering) ────────────────────────

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    // Shadow proxy: invisible in the main pass.
    if msdf.is_shadow_proxy == 1u {
        discard;
    }

    let final_alpha = compute_alpha(in.uv);

    if final_alpha < 0.02 {
        // Even transparent fragments may need to draw the box line.
        if msdf.ink_uv_max.x > msdf.ink_uv_min.x && is_on_ink_box(in.uv) {
            var out: FragmentOutput;
            out.color = vec4<f32>(1.0, 1.0, 0.0, 1.0);
            return out;
        }
        discard;
    }

    // Draw bounding box line if enabled.
    if msdf.ink_uv_max.x > msdf.ink_uv_min.x && is_on_ink_box(in.uv) {
        var out: FragmentOutput;
        out.color = vec4<f32>(1.0, 1.0, 0.0, 1.0);
        return out;
    }

    // Standard PBR input — handles double-sided, normals, lighting, everything.
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // Apply hue rotation to vertex color if needed.
#ifdef VERTEX_COLORS
    if in.color.a > 0.0 && msdf.hue_offset != 0.0 {
        let rotated = rotate_hue(pbr_input.material.base_color.rgb, msdf.hue_offset);
        pbr_input.material.base_color = vec4<f32>(rotated, pbr_input.material.base_color.a);
    }
#endif

    // Apply final alpha on top.
    pbr_input.material.base_color.a *= final_alpha;

    pbr_input.material.base_color = alpha_discard(
        pbr_input.material,
        pbr_input.material.base_color,
    );

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    return out;
}
#endif
