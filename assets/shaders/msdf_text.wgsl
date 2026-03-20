// MSDF Text Fragment Shader — MaterialExtension for StandardMaterial
//
// Renders glyphs from an MSDF atlas texture. Uses the median-of-three
// technique for clean edges at any scale, with adaptive anti-aliasing
// based on screen pixel range.
//
// Extends StandardMaterial's PBR pipeline: all lighting, shadows, and
// double-sided normal handling come from the base material.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    pbr_bindings,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

struct MsdfTextUniform {
    sdf_range: f32,
    atlas_width: f32,
    atlas_height: f32,
    hue_offset: f32,
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

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    // MSDF decode: compute per-pixel alpha from the signed distance field.
    let msdf_sample = textureSample(msdf_texture, msdf_sampler, in.uv);
    let sd = median(msdf_sample.r, msdf_sample.g, msdf_sample.b) - 0.5;
    let screen_px_dist = screen_px_range(in.uv) * sd;
    let msdf_alpha = clamp(screen_px_dist + 0.5, 0.0, 1.0);

    if msdf_alpha < 0.02 {
        discard;
    }

    // Standard PBR input — handles double-sided, normals, lighting, everything.
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // Read material alpha from the StandardMaterial uniform directly, because
    // pbr_input_from_standard_material overwrites base_color with vertex color
    // when VERTEX_COLORS is defined.
#ifdef BINDLESS
    let base_alpha = pbr_bindings::material_array[material_indices[slot].material].base_color.a;
#else
    let base_alpha = pbr_bindings::material.base_color.a;
#endif
#ifdef VERTEX_COLORS
    if in.color.a > 0.0 {
        var vc = in.color;
        if msdf.hue_offset != 0.0 {
            vc = vec4<f32>(rotate_hue(vc.rgb, msdf.hue_offset), vc.a);
        }
        pbr_input.material.base_color = vec4<f32>(vc.rgb, vc.a * base_alpha * msdf_alpha);
    } else {
        pbr_input.material.base_color.a = base_alpha * msdf_alpha;
    }
#else
    pbr_input.material.base_color.a = base_alpha * msdf_alpha;
#endif

    pbr_input.material.base_color = alpha_discard(
        pbr_input.material,
        pbr_input.material.base_color,
    );

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif

    return out;
}
