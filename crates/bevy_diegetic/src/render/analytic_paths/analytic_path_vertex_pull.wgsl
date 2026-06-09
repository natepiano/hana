// Vertex-pulling stage for batched glyph text.
//
// The batch mesh is inert: capacity-sized zeroed vertices whose only job is
// switching the VERTEX_UVS_A / VERTEX_UVS_B pipeline defs on. Every visible
// value is pulled from two storage tables — one GlyphInstanceRecord per glyph
// quad, one RunRecord per text run — indexed from the vertex index. One file
// serves the main, prepass, and shadow pipelines: `#ifdef PREPASS_PIPELINE`
// picks the entry point, and both call the same `pull_vertex` helper so the
// expansion and the depth nudge cannot drift between passes.

#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_functions,
    view_transformations::position_world_to_clip,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::VertexOutput
#else
#import bevy_pbr::forward_io::VertexOutput
#endif

// Mirrors `GlyphInstanceRecord` in `glyph/packing.rs` (std430, 40 B stride).
struct GlyphInstanceRecord {
    rect_min: vec2<f32>,
    rect_size: vec2<f32>,
    uv_min: vec2<f32>,
    uv_size: vec2<f32>,
    atlas_index: u32,
    run_index: u32,
}

// Mirrors `RunRecord` in `glyph/packing.rs` (std430, 96 B stride).
struct RunRecord {
    transform: mat4x4<f32>,
    fill_color: vec4<f32>,
    render_mode: u32,
    depth_nudge: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(104) var<storage, read> instances: array<GlyphInstanceRecord>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var<storage, read> run_records: array<RunRecord>;

// Clip-space depth shift per depth_nudge layer unit, applied post-projection
// so coplanar runs resolve to distinct depths. Provisional magnitude;
// Step 3b verifies Geometry-mode layering against `panel_rendering`.
const DEPTH_NUDGE_CLIP_PER_LAYER: f32 = 0.000002;

// World-space Y lift per recovered glyph index in debug-staircase mode.
const GLYPH_PULL_DEBUG_STEP: f32 = 0.01;

struct PulledVertex {
    clip_position: vec4<f32>,
    world_position: vec4<f32>,
    world_normal: vec3<f32>,
    uv: vec2<f32>,
    uv_b: vec2<f32>,
}

// Expands one inert-mesh vertex into its glyph-quad corner. Both vertex entry
// points route through here, so the corner math and the depth nudge are one
// code path for the main, prepass, and shadow passes.
fn pull_vertex(vertex_index: u32, instance_index: u32) -> PulledVertex {
    var out: PulledVertex;

    // wgpu's vertex_index includes the draw's base_vertex, and the mesh
    // allocator packs meshes into shared slabs, so the slab base must come
    // off before deriving glyph and corner (same correction as
    // bevy_pbr's wireframe.wgsl).
    let local_index = vertex_index - mesh[instance_index].first_vertex_index;
    let glyph = local_index / 4u;
    let corner = local_index % 4u;

    // The index buffer is capacity-sized while the instance buffer is
    // live-sized: collapse the capacity tail to degenerate quads so
    // robustness clamping can never re-blend the last record.
    if glyph >= arrayLength(&instances) {
        out.clip_position = vec4<f32>(0.0, 0.0, 0.0, 1.0);
        return out;
    }

    let record = instances[glyph];

    // Bevy compiles shaders without bounds checks, so a run_index outside the
    // run table would read arbitrary memory and emit a quad anywhere on
    // screen. Collapse such a record to a degenerate quad.
    if record.run_index >= arrayLength(&run_records) {
        out.clip_position = vec4<f32>(0.0, 0.0, 0.0, 1.0);
        return out;
    }
    let run = run_records[record.run_index];

    // Corner order matches RunMeshBuilder::push_glyph:
    // 0 = (left, top), 1 = (right, top), 2 = (right, bottom), 3 = (left, bottom).
    let corner_x = f32(corner == 1u || corner == 2u);
    let corner_top = f32(corner <= 1u);
    let local = record.rect_min + vec2<f32>(corner_x, corner_top) * record.rect_size;

    var world = run.transform * vec4<f32>(local, 0.0, 1.0);
#ifdef GLYPH_PULL_DEBUG_INDEX
    world.y += f32(glyph) * GLYPH_PULL_DEBUG_STEP;
#endif
    out.world_position = world;

    var clip = position_world_to_clip(world.xyz);
#ifndef OIT_ENABLED
    clip.z += run.depth_nudge * DEPTH_NUDGE_CLIP_PER_LAYER * clip.w;
#endif
    out.clip_position = clip;

    // Records carry no normal: rotate layout-space +z by the run transform.
    out.world_normal = normalize((run.transform * vec4<f32>(0.0, 0.0, 1.0, 0.0)).xyz);

    // uv = padded glyph quad UVs (uv_min sits at the top-left corner);
    // uv_b.x = atlas record index, uv_b.y = run index, both recovered in the
    // fragment with u32(floor(..)).
    out.uv = record.uv_min + vec2<f32>(corner_x, 1.0 - corner_top) * record.uv_size;
    out.uv_b = vec2<f32>(f32(record.atlas_index), f32(record.run_index));
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
    out.uv = pulled.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = pulled.uv_b;
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
    out.uv = pulled.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = pulled.uv_b;
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
