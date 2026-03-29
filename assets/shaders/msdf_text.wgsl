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
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
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

#ifdef OIT_ENABLED
#import bevy_core_pipeline::oit::oit_draw
#import bevy_pbr::pbr_types::{
    STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS,
    STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE,
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
    clip_rect: vec4<f32>,
    oit_depth_offset: f32,
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
    // Clip to parent scissor rect (panel-local Y-up coordinates).
    if in.uv_b.x < msdf.clip_rect.x || in.uv_b.x > msdf.clip_rect.z ||
       in.uv_b.y < msdf.clip_rect.y || in.uv_b.y > msdf.clip_rect.w {
        discard;
    }

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
    // Clip to parent scissor rect (panel-local Y-up coordinates).
    if in.uv_b.x < msdf.clip_rect.x || in.uv_b.x > msdf.clip_rect.z ||
       in.uv_b.y < msdf.clip_rect.y || in.uv_b.y > msdf.clip_rect.w {
        discard;
    }

    // Shadow proxy: invisible in the main pass.
    if msdf.is_shadow_proxy == 1u {
        discard;
    }

    let final_alpha = compute_alpha(in.uv);

    if final_alpha < 0.02 {
        discard;
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

    // Lighting: respect the unlit flag from StandardMaterial.
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    // OIT: offset position.z so coplanar layers get distinct depths in
    // the OIT linked list. Pipeline depth_bias does NOT affect
    // in.position.z, so we apply the offset here.
#ifdef OIT_ENABLED
    let alpha_mode = pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if alpha_mode != STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE {
        var oit_pos = in.position;
        oit_pos.z += msdf.oit_depth_offset;
        oit_draw(oit_pos, out.color);
        discard;
    }
#endif

    return out;
}
#endif
