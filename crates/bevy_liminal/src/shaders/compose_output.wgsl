#import bevy_pbr::{
    view_transformations::{ndc_to_uv},
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;
@group(0) @binding(2) var flood_texture: texture_2d<f32>;
@group(0) @binding(3) var appearance_texture: texture_2d<f32>;
#ifdef MULTISAMPLED
@group(0) @binding(4) var depth_texture: texture_depth_multisampled_2d;
#else
@group(0) @binding(4) var depth_texture: texture_depth_2d;
#endif
@group(0) @binding(5) var outline_depth_texture: texture_depth_2d;
#ifdef MULTISAMPLED
@group(0) @binding(6) var main_depth_texture: texture_depth_multisampled_2d;
#else
@group(0) @binding(6) var main_depth_texture: texture_depth_2d;
#endif

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = textureSample(screen_texture, texture_sampler, in.uv);
    let flood_data = textureSample(flood_texture, texture_sampler, in.uv);
    let seed_uv = flood_data.xy;

    // Early return if no outline data
    if seed_uv.x <= 0.0 || seed_uv.y <= 0.0 {
        return color;
    }

    // Check if this pixel is ON an outlined mesh by sampling the outline depth
    // at the current pixel. The outline depth texture is cleared to 0.0 (far plane
    // in reverse-Z), so any pixel with depth > 0 belongs to an outlined mesh.
    // This prevents the outline from drawing on the mesh itself, which is critical
    // for transmissive/transparent materials that don't write to the scene depth buffers.
    let self_outline_depth = textureSample(outline_depth_texture, texture_sampler, in.uv);
    if self_outline_depth > 0.0 {
        return color;
    }

    // Get depths — use the closer of prepass and main pass depth so that both
    // opaque (prepass) and transmissive/transparent (main pass) geometry occlude outlines.
    // Bevy uses reverse-Z, so larger depth is closer.
    let coords = vec2<i32>(in.uv * vec2<f32>(textureDimensions(depth_texture)));
#ifdef MULTISAMPLED
    let prepass_depth = textureLoad(depth_texture, coords, 0);
#else
    let prepass_depth = textureSample(depth_texture, texture_sampler, in.uv);
#endif
#ifdef MULTISAMPLED
    let main_depth = textureLoad(main_depth_texture, coords, 0);
#else
    let main_depth = textureSample(main_depth_texture, texture_sampler, in.uv);
#endif
    let current_depth = max(prepass_depth, main_depth);
    let outline_depth = textureSample(outline_depth_texture, texture_sampler, seed_uv);

    // Get appearance data for this outline
    let appearance = textureSample(appearance_texture, texture_sampler, seed_uv);
    let outline_color = appearance.rgb;

    // Reverse-Z: only render outline when the outline seed depth is closer than the
    // currently visible scene depth at this pixel.
    if outline_depth > current_depth {
        // Apply outline color
        color = vec4<f32>(outline_color, 1.0);
    }

    return color;
}
