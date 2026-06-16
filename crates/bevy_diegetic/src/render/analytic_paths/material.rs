use bevy::asset::Asset;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MATERIAL_BIND_GROUP_INDEX;
use bevy::pbr::MaterialExtension;
use bevy::pbr::MaterialExtensionKey;
use bevy::pbr::MaterialExtensionPipeline;
use bevy::pbr::StandardMaterial;
use bevy::prelude::Handle;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::RenderPipelineDescriptor;
use bevy::render::render_resource::ShaderType;
use bevy::render::render_resource::SpecializedMeshPipelineError;
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;

use super::constants::ANALYTIC_PATH_SHADER_PATH;
use super::constants::ANALYTIC_PATH_VERTEX_PULL_SHADER_HANDLE;
use crate::layout::GlyphRenderMode;

/// Visible render mode for the path shader.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u32)]
pub(crate) enum RenderMode {
    /// Normal coverage fill.
    #[default]
    Text     = 1,
    /// Inverted coverage inside each path quad.
    PunchOut = 2,
}

impl From<RenderMode> for u32 {
    fn from(mode: RenderMode) -> Self { mode as Self }
}

impl From<GlyphRenderMode> for RenderMode {
    fn from(render_mode: GlyphRenderMode) -> Self {
        match render_mode {
            GlyphRenderMode::Text => Self::Text,
            GlyphRenderMode::PunchOut => Self::PunchOut,
        }
    }
}

/// Material used by the path renderer.
pub(crate) type PathMaterial = ExtendedMaterial<StandardMaterial, PathExtension>;

/// Uniforms consumed by the path shader.
#[derive(Clone, Debug, ShaderType)]
struct PathUniform {
    /// Linear fill color.
    fill_color:       Vec4,
    /// Visible render mode for this pass.
    render_mode:      u32,
    /// Per-layer depth offset applied to the OIT fragment position for coplanar
    /// layer ordering.
    oit_depth_offset: f32,
    /// Non-zero enables sub-pixel supersampling of path coverage (anti-aliases
    /// grazing-angle edges without MSAA).
    supersample:      u32,
    /// Non-zero switches the edge AA band from the scalar design-space
    /// `edge_width` to a screen-space band derived from the distance gradient,
    /// which fixes the convex-corner flare at grazing angles.
    aa_band:          u32,
    /// Minimum on-screen stroke width in device pixels for hairline-dilated
    /// paths (`PathRecord::min_feature > 0`). Mirrored from
    /// [`HairlineWidth`](crate::HairlineWidth) by `sync_hairline_width`.
    hairline_min_px:  f32,
}

/// Constructor default for `PathUniform::hairline_min_px`; `sync_hairline_width`
/// replaces it with the window-scale-derived value on the asset-added event.
const HAIRLINE_DEFAULT_DEVICE_PX: f32 = 2.0;

/// Text material extension over `StandardMaterial`.
#[derive(Asset, AsBindGroup, Clone, Debug, TypePath)]
#[bind_group_data(PathExtensionKey)]
pub struct PathExtension {
    /// Shader uniforms.
    #[uniform(100)]
    uniforms:     PathUniform,
    /// Shared path record band-packed quadratic curve records.
    #[storage(101, read_only)]
    curves:       Handle<ShaderBuffer>,
    /// Shared path horizontal/vertical band records.
    #[storage(102, read_only)]
    bands:        Handle<ShaderBuffer>,
    /// Shared path records, indexed by each atlas record's `atlas_index`.
    #[storage(103, read_only)]
    path_records: Handle<ShaderBuffer>,
    /// Per-path instance records read by the vertex-pulling stage.
    #[storage(104, read_only, visibility(vertex))]
    instances:    Handle<ShaderBuffer>,
    /// Per-run records (world transform, fill color, render mode, depth
    /// nudge) read by the vertex-pulling stages.
    #[storage(105, read_only, visibility(vertex, fragment))]
    run_records:  Handle<ShaderBuffer>,
    /// Routes this material's vertex stages (main, prepass, shadow) through
    /// `analytic_path_vertex_pull.wgsl` instead of the standard mesh vertex stage.
    vertex_pull:  bool,
}

#[cfg(test)]
pub(crate) const fn path_material_oit_depth_offset(material: &PathMaterial) -> f32 {
    material.extension.uniforms.oit_depth_offset
}

/// Pipeline-specialization key for [`PathExtension`]: which vertex stage a
/// material compiles against.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct PathExtensionKey {
    /// Mirror of [`PathExtension::vertex_pull`].
    vertex_pull: bool,
}

impl From<&PathExtension> for PathExtensionKey {
    fn from(extension: &PathExtension) -> Self {
        Self {
            vertex_pull: extension.vertex_pull,
        }
    }
}

impl MaterialExtension for PathExtension {
    fn fragment_shader() -> ShaderRef { ANALYTIC_PATH_SHADER_PATH.into() }

    fn prepass_fragment_shader() -> ShaderRef { ANALYTIC_PATH_SHADER_PATH.into() }

    // Standing contract of the path renderer: the camera depth prepass cannot
    // run vertex-pull batches. Bevy strips the material bind group from
    // depth-only opaque prepass pipelines (`is_depth_only_opaque_prepass`),
    // and the vertex-pull vertex stage reads bindings 104/105 from that
    // group — pipeline creation fails with a wgpu validation error. The main
    // opaque pass writes its own depth (`GreaterEqual`, write enabled), so
    // skipping the prepass is an early-z loss only. Shadow views still queue
    // (`enable_shadows`); the ones bevy routes depth-only fall back to the
    // standard vertex stage in `specialize` (see
    // `material_group_is_stripped`). Any future change that re-enables the
    // prepass must keep (or consciously extend) that guard.
    fn enable_prepass() -> bool { false }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if key.bind_group_data.vertex_pull && !material_group_is_stripped(descriptor) {
            // One WGSL file serves the main, prepass, and shadow pipelines;
            // its `#ifdef PREPASS_PIPELINE` gate picks the entry point. The
            // swap happens here rather than in `vertex_shader()` /
            // `prepass_vertex_shader()` because those are material-type-wide,
            // and stripped-group pipelines must keep the standard vertex
            // stage (see `material_group_is_stripped`).
            descriptor.vertex.shader = ANALYTIC_PATH_VERTEX_PULL_SHADER_HANDLE;
            // The fragment sources `fill_color` / `render_mode` from the run
            // table (binding 105) instead of the material uniform, so a batch
            // renders every run with its own color and mode.
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("GLYPH_VERTEX_PULL".into());
            }
        }
        Ok(())
    }
}

/// Whether bevy substituted the empty layout for the material bind group in
/// this pipeline. Depth-only pipelines do this — shadow views for alpha modes
/// without `MAY_DISCARD` (e.g. `Multiply`) — and the vertex-pull stage reads
/// bindings 104/105 from the material group, so swapping it in would fail
/// wgpu validation. Such pipelines keep the standard vertex stage instead:
/// the inert batch mesh's all-zero positions rasterize nothing, so the batch
/// casts no shadow there. This also catches any future mode or engine change
/// that strips the material group.
fn material_group_is_stripped(descriptor: &RenderPipelineDescriptor) -> bool {
    descriptor
        .layout
        .get(MATERIAL_BIND_GROUP_INDEX)
        .is_none_or(|material_layout| material_layout.entries.is_empty())
}

/// Inputs for one vertex-pulling batch material.
pub(crate) struct BatchPathMaterialInput {
    /// Base material settings.
    pub base:             StandardMaterial,
    /// Placeholder fill color; vertex-pulling fragments read per-run color from
    /// `run_records`.
    pub fill_color:       Vec4,
    /// Placeholder render mode; vertex-pulling fragments read per-run mode from
    /// `run_records`.
    pub render_mode:      RenderMode,
    /// Per-layer depth offset for coplanar OIT layer ordering.
    pub oit_depth_offset: f32,
    /// Whether the material supersamples the path footprint.
    pub supersample:      bool,
    /// Whether the material uses the screen-space anisotropic edge band.
    pub aa_band:          bool,
    /// Shared path record band-packed quadratic curve records.
    pub curves:           Handle<ShaderBuffer>,
    /// Shared path horizontal/vertical band records.
    pub bands:            Handle<ShaderBuffer>,
    /// Shared path records, indexed by each atlas record's `atlas_index`.
    pub path_records:     Handle<ShaderBuffer>,
    /// Per-path instance records read by the vertex-pulling stage.
    pub instances:        Handle<ShaderBuffer>,
    /// Per-run records read by the vertex-pulling stage.
    pub run_records:      Handle<ShaderBuffer>,
}

/// Creates a vertex-pulling `PathMaterial` for one batch.
#[must_use]
pub(crate) fn batch_path_material(input: BatchPathMaterialInput) -> PathMaterial {
    let BatchPathMaterialInput {
        base,
        fill_color,
        render_mode,
        oit_depth_offset,
        supersample,
        aa_band,
        curves,
        bands,
        path_records,
        instances,
        run_records,
    } = input;
    ExtendedMaterial {
        base,
        extension: PathExtension {
            uniforms: PathUniform {
                fill_color,
                render_mode: u32::from(render_mode),
                oit_depth_offset,
                supersample: u32::from(supersample),
                aa_band: u32::from(aa_band),
                hairline_min_px: HAIRLINE_DEFAULT_DEVICE_PX,
            },
            curves,
            bands,
            path_records,
            instances,
            run_records,
            vertex_pull: true,
        },
    }
}

/// Repoints a batch material at replacement record buffers after capacity
/// growth.
pub(crate) fn set_batch_path_material_buffers(
    material: &mut PathMaterial,
    instances: Handle<ShaderBuffer>,
    run_records: Handle<ShaderBuffer>,
) {
    material.extension.instances = instances;
    material.extension.run_records = run_records;
}

/// Repoints a path material at replacement shared-atlas buffers after the
/// atlas grows. The batch record buffers (bindings 104/105) are per-batch,
/// not atlas-owned, so they are untouched here.
pub(crate) fn set_path_material_atlas(
    material: &mut PathMaterial,
    curves: Handle<ShaderBuffer>,
    bands: Handle<ShaderBuffer>,
    path_records: Handle<ShaderBuffer>,
) {
    material.extension.curves = curves;
    material.extension.bands = bands;
    material.extension.path_records = path_records;
}

/// Updates the hairline minimum stroke width (device pixels) on a path
/// material.
pub(crate) const fn set_path_material_hairline(material: &mut PathMaterial, device_px: f32) {
    material.extension.uniforms.hairline_min_px = device_px;
}

/// Updates the shader anti-aliasing switches on a path material.
pub(crate) fn set_path_material_anti_alias(
    material: &mut PathMaterial,
    supersample: bool,
    aa_band: bool,
) {
    material.extension.uniforms.supersample = u32::from(supersample);
    material.extension.uniforms.aa_band = u32::from(aa_band);
}
