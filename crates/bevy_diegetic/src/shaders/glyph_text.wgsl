// Glyph Text Fragment Shader — MaterialExtension for StandardMaterial
//
// Renders glyphs from a signed-distance-field atlas texture. The
// `distance_field` uniform selects sampling strategy:
//   0 = MSDF  — median of R, G, B for sharp corners.
//   1 = SDF   — single channel (R) for smoother curves.
//   2 = MTSDF — RGB is MSDF, alpha is signed true SDF. Takes
//               max(median, alpha): comb holes fill (alpha wins),
//               sharp corner wedges keep their extension past the
//               rounded true-SDF edge (median wins), and sub-texel
//               inward notches soften into faint AA traces instead
//               of comb-amplified dark lines at low atlas sizes.
// All branches feed the same adaptive anti-aliasing path.
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
#import bevy_pbr::prepass_io::VertexOutput
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

struct GlyphMaterialUniform {
    sdf_range: f32,
    atlas_width: f32,
    atlas_height: f32,
    hue_offset: f32,
    render_mode: u32,
    is_shadow_proxy: u32,
    // 0 = MSDF (median of R, G, B); 1 = SDF (R only).
    distance_field: u32,
    clip_rect: vec4<f32>,
    oit_depth_offset: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> uniforms: GlyphMaterialUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var atlas_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var atlas_sampler: sampler;

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
        uniforms.sdf_range / uniforms.atlas_width,
        uniforms.sdf_range / uniforms.atlas_height,
    );
    let screen_tex_size = vec2<f32>(1.0) / fwidth(uv);
    return max(
        0.5 * dot(unit_range, screen_tex_size),
        1.0,
    );
}

/// Computes the final alpha based on the render mode and distance-field
/// variant. MSDF takes median of RGB; SDF reads only R; MTSDF takes the
/// median of RGB clamped to ±tolerance around the alpha (true SDF).
fn compute_alpha(uv: vec2<f32>) -> f32 {
    if uniforms.render_mode == RENDER_MODE_SOLID_QUAD {
        return 1.0;
    }

    let atlas_sample = textureSample(atlas_texture, atlas_sampler, uv);
    var distance: f32;
    if uniforms.distance_field == 1u {
        distance = atlas_sample.r;
    } else if uniforms.distance_field == 2u {
        // MTSDF: max(median, alpha). Pushes the result toward
        // whichever channel reports "more inside" — which gives the
        // right answer in every regime: comb holes fill (alpha wins),
        // sharp corners stay sharp (median wins past the rounded
        // true-SDF edge), sub-texel inward notches at low atlas
        // resolution soften into faint AA traces (alpha is less
        // negative than the comb-amplified median) instead of hard
        // dark lines while still resolving cleanly when the atlas
        // size makes the feature multi-texel.
        let msdf_median = median(atlas_sample.r, atlas_sample.g, atlas_sample.b);
        let alpha_sdf = atlas_sample.a;
        distance = max(msdf_median, alpha_sdf);
    } else {
        distance = median(atlas_sample.r, atlas_sample.g, atlas_sample.b);
    }
    let sd = distance - 0.5;
    let screen_px_dist = screen_px_range(uv) * sd;
    let glyph_alpha = clamp(screen_px_dist + 0.5, 0.0, 1.0);

    if uniforms.render_mode == RENDER_MODE_PUNCH_OUT {
        return 1.0 - glyph_alpha;
    }

    // RENDER_MODE_TEXT (default).
    return glyph_alpha;
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
    if in.uv_b.x < uniforms.clip_rect.x || in.uv_b.x > uniforms.clip_rect.z ||
       in.uv_b.y < uniforms.clip_rect.y || in.uv_b.y > uniforms.clip_rect.w {
        discard;
    }

    // Only shadow proxies need MSDF decode in the prepass. Non-proxy
    // meshes pass through without texture sampling — the full quad
    // writes depth, producing a rectangular shadow (SolidQuad behavior).
    if uniforms.is_shadow_proxy == 1u {
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
    if in.uv_b.x < uniforms.clip_rect.x || in.uv_b.x > uniforms.clip_rect.z ||
       in.uv_b.y < uniforms.clip_rect.y || in.uv_b.y > uniforms.clip_rect.w {
        discard;
    }

    // Shadow proxy: invisible in the main pass.
    if uniforms.is_shadow_proxy == 1u {
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
    if in.color.a > 0.0 && uniforms.hue_offset != 0.0 {
        let rotated = rotate_hue(pbr_input.material.base_color.rgb, uniforms.hue_offset);
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
        oit_pos.z += uniforms.oit_depth_offset;
        oit_draw(oit_pos, out.color);
        discard;
    }
#endif

    return out;
}
#endif
