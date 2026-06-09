// Vertex-pulled panel-line batch shader.

#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_functions,
    view_transformations::position_world_to_clip,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::VertexOutput
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}
#endif

#ifdef OIT_ENABLED
#import bevy_core_pipeline::oit::oit_draw
#import bevy_pbr::pbr_types::{
    STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS,
    STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE,
}
#endif

#import bevy_diegetic::sdf_stroke::{
    inflate_subpixel_half_size,
    rect_strip_alpha,
}

struct PanelLineRecord {
    transform: mat4x4<f32>,
    // xy = mesh half-size, z = SDF kind, w = sorted depth nudge.
    mesh_half_kind_depth: vec4<f32>,
    // xy = shape half-size, z = OIT depth offset, w unused.
    shape_oit: vec4<f32>,
    clip_rect: vec4<f32>,
    color: vec4<f32>,
    params: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<storage, read> line_records: array<PanelLineRecord>;

const DEPTH_NUDGE_CLIP_PER_LAYER: f32 = 0.000002;

struct PulledVertex {
    clip_position: vec4<f32>,
    world_position: vec4<f32>,
    world_normal: vec3<f32>,
    local: vec2<f32>,
    record_index: f32,
}

fn pull_vertex(vertex_index: u32, instance_index: u32) -> PulledVertex {
    var out: PulledVertex;
    let local_index = vertex_index - mesh[instance_index].first_vertex_index;
    let record_index = local_index / 4u;
    let corner = local_index % 4u;

    if record_index >= arrayLength(&line_records) {
        out.clip_position = vec4<f32>(0.0, 0.0, 0.0, 1.0);
        return out;
    }

    let record = line_records[record_index];
    let half = record.mesh_half_kind_depth.xy;
    let corner_x = f32(corner == 1u || corner == 2u);
    let corner_top = f32(corner <= 1u);
    let local = vec2<f32>(
        mix(-half.x, half.x, corner_x),
        mix(-half.y, half.y, corner_top),
    );
    let world = record.transform * vec4<f32>(local, 0.0, 1.0);
    var clip = position_world_to_clip(world.xyz);
#ifndef OIT_ENABLED
    clip.z += record.mesh_half_kind_depth.w * DEPTH_NUDGE_CLIP_PER_LAYER * clip.w;
#endif

    out.clip_position = clip;
    out.world_position = world;
    out.world_normal = normalize((record.transform * vec4<f32>(0.0, 0.0, 1.0, 0.0)).xyz);
    out.local = local;
    out.record_index = f32(record_index);
    return out;
}

#ifdef PREPASS_PIPELINE
@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let pulled = pull_vertex(vertex_index, instance_index);
    var out: VertexOutput;
    out.position = pulled.clip_position;
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = pulled.clip_position.z;
    out.position.z = min(out.position.z, 1.0);
#endif
#ifdef VERTEX_UVS_A
    out.uv = pulled.local;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vec2<f32>(pulled.record_index, 0.0);
#endif
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = pulled.world_normal;
#endif
    out.world_position = pulled.world_position;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = instance_index;
#endif
    return out;
}
#else
@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let pulled = pull_vertex(vertex_index, instance_index);
    var out: VertexOutput;
    out.position = pulled.clip_position;
    out.world_position = pulled.world_position;
    out.world_normal = pulled.world_normal;
#ifdef VERTEX_UVS_A
    out.uv = pulled.local;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vec2<f32>(pulled.record_index, 0.0);
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        instance_index, mesh_functions::get_world_from_local(instance_index)[3]);
#endif
    return out;
}
#endif

fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

fn sd_triangle(p: vec2<f32>, half_size: vec2<f32>, params: vec4<f32>) -> f32 {
    let a = vec2<f32>(half_size.x + params.x, 0.0);
    let b = vec2<f32>(-half_size.x, half_size.y);
    let c = vec2<f32>(-half_size.x, -half_size.y);

    let d = min(sd_segment(p, a, b), min(sd_segment(p, b, c), sd_segment(p, c, a)));

    let s1 = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let s2 = (c.x - b.x) * (p.y - b.y) - (c.y - b.y) * (p.x - b.x);
    let s3 = (a.x - c.x) * (p.y - c.y) - (a.y - c.y) * (p.x - c.x);
    let has_neg = s1 < 0.0 || s2 < 0.0 || s3 < 0.0;
    let has_pos = s1 > 0.0 || s2 > 0.0 || s3 > 0.0;
    let inside = !(has_neg && has_pos);

    return select(d, -d, inside);
}

fn sd_circle(p: vec2<f32>, half_size: vec2<f32>) -> f32 {
    return length(p) - min(half_size.x, half_size.y);
}

fn axis_from_params(params: vec2<f32>) -> vec2<f32> {
    if dot(params, params) <= 0.000001 {
        return vec2<f32>(1.0, 0.0);
    }
    return normalize(params);
}

fn oriented_coords(p: vec2<f32>, axis: vec2<f32>) -> vec2<f32> {
    let normal = vec2<f32>(-axis.y, axis.x);
    return vec2<f32>(dot(p, axis), dot(p, normal));
}

fn sd_box(p: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let q = abs(p) - half_size;
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0);
}

fn sd_diamond(p: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let a = vec2<f32>(half_size.x, 0.0);
    let b = vec2<f32>(0.0, half_size.y);
    let c = vec2<f32>(-half_size.x, 0.0);
    let d = vec2<f32>(0.0, -half_size.y);

    let dist = min(
        min(sd_segment(p, a, b), sd_segment(p, b, c)),
        min(sd_segment(p, c, d), sd_segment(p, d, a)),
    );

    let s1 = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let s2 = (c.x - b.x) * (p.y - b.y) - (c.y - b.y) * (p.x - b.x);
    let s3 = (d.x - c.x) * (p.y - c.y) - (d.y - c.y) * (p.x - c.x);
    let s4 = (a.x - d.x) * (p.y - d.y) - (a.y - d.y) * (p.x - d.x);
    let has_neg = s1 < 0.0 || s2 < 0.0 || s3 < 0.0 || s4 < 0.0;
    let has_pos = s1 > 0.0 || s2 > 0.0 || s3 > 0.0 || s4 > 0.0;
    let inside = !(has_neg && has_pos);

    return select(dist, -dist, inside);
}

fn sd_form(record: PanelLineRecord, local: vec2<f32>) -> f32 {
    let kind = u32(record.mesh_half_kind_depth.z + 0.5);
    let half_size = record.shape_oit.xy;
    if kind == 2u {
        return sd_circle(local, half_size);
    }
    let cap_local = oriented_coords(local, axis_from_params(record.params.zw));
    if kind == 5u {
        return sd_triangle(cap_local, half_size, record.params);
    }
    if kind == 6u {
        return sd_box(cap_local, half_size);
    }
    if kind == 7u {
        return sd_diamond(cap_local, half_size);
    }
    return sd_box(oriented_coords(local, axis_from_params(record.params.xy)), half_size);
}

fn form_aa_width(kind: u32, dist: f32, params: vec4<f32>) -> f32 {
    if kind == 5u {
        return fwidth(dist) * 0.75 * max(0.1, params.y);
    }
    return fwidth(dist) * 0.75;
}

fn record_from_input(in: VertexOutput) -> PanelLineRecord {
#ifdef VERTEX_UVS_B
    let record_index = u32(floor(in.uv_b.x + 0.5));
#else
    let record_index = 0u;
#endif
    return line_records[record_index];
}

#ifdef PREPASS_PIPELINE
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) {
    let record = record_from_input(in);
    let local = in.uv;
    if local.x < record.clip_rect.x || local.x > record.clip_rect.z
        || local.y < record.clip_rect.y || local.y > record.clip_rect.w {
        discard;
    }
    let dist = sd_form(record, local);
    if dist > 0.0 {
        discard;
    }
}
#else
@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    let record = record_from_input(in);
    let local = in.uv;
    if local.x < record.clip_rect.x || local.x > record.clip_rect.z
        || local.y < record.clip_rect.y || local.y > record.clip_rect.w {
        discard;
    }

    let kind = u32(record.mesh_half_kind_depth.z + 0.5);
    let is_line_form = kind == 4u;
    let line_local = oriented_coords(local, axis_from_params(record.params.xy));
    let line_pixel_size = vec2<f32>(fwidth(line_local.x), fwidth(line_local.y));
    let line_inflated = inflate_subpixel_half_size(record.shape_oit.xy, line_pixel_size);
    let line_alpha = select(
        0.0,
        rect_strip_alpha(line_local, line_inflated.xy, line_inflated.z),
        is_line_form,
    );

    var coverage = line_alpha;
    if !is_line_form {
        let dist = sd_form(record, local);
        let base_aa = form_aa_width(kind, dist, record.params);
        coverage = 1.0 - smoothstep(-base_aa, base_aa, dist);
    }

    if coverage < 0.001 {
        discard;
    }

    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = vec4<f32>(
        record.color.rgb,
        record.color.a * coverage,
    );

    if pbr_input.material.base_color.a < 0.001 {
        discard;
    }

    pbr_input.material.base_color = alpha_discard(
        pbr_input.material,
        pbr_input.material.base_color,
    );

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

#ifdef OIT_ENABLED
    let alpha_mode = pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if alpha_mode != STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE {
        var oit_pos = in.position;
        oit_pos.z += record.shape_oit.z;
        oit_draw(oit_pos, out.color);
        discard;
    }
#endif

    return out;
}
#endif
