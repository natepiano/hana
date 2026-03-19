// MSDF Text Fragment Shader
//
// Renders glyphs from an MSDF atlas texture. Uses the median-of-three
// technique for clean edges at any scale, with adaptive anti-aliasing
// based on screen pixel range.

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> material: MsdfTextMaterial;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var msdf_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var msdf_sampler: sampler;

struct MsdfTextMaterial {
    color: vec4<f32>,
    sdf_range: f32,
    atlas_width: f32,
    atlas_height: f32,
    _padding: f32,
}

fn median(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

fn screen_px_range(uv: vec2<f32>) -> f32 {
    let unit_range = vec2<f32>(
        material.sdf_range / material.atlas_width,
        material.sdf_range / material.atlas_height,
    );
    let screen_tex_size = vec2<f32>(1.0) / fwidth(uv);
    return max(
        0.5 * dot(unit_range, screen_tex_size),
        1.0,
    );
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let msdf = textureSample(msdf_texture, msdf_sampler, in.uv);
    let sd = median(msdf.r, msdf.g, msdf.b) - 0.5;
    let screen_px_dist = screen_px_range(in.uv) * sd;
    let alpha = clamp(screen_px_dist + 0.5, 0.0, 1.0);

    if alpha < 0.02 {
        discard;
    }

    return vec4<f32>(material.color.rgb, material.color.a * alpha);
}
