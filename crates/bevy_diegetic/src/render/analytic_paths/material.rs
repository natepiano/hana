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

/// Visible render mode for the text shader.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u32)]
pub(crate) enum RenderMode {
    /// Normal coverage fill.
    #[default]
    Text     = 1,
    /// Inverted coverage inside each glyph quad.
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

/// Material used by the text renderer.
pub(crate) type TextMaterial = ExtendedMaterial<StandardMaterial, TextExtension>;

/// Uniforms consumed by the text shader.
#[derive(Clone, Debug, ShaderType)]
struct TextUniform {
    /// Linear fill color.
    fill_color:       Vec4,
    /// Visible render mode for this pass.
    render_mode:      u32,
    /// Per-layer depth offset applied to the OIT fragment position for coplanar
    /// layer ordering.
    oit_depth_offset: f32,
    /// Non-zero enables sub-pixel supersampling of glyph coverage (anti-aliases
    /// grazing-angle edges without MSAA).
    supersample:      u32,
    /// Non-zero switches the edge AA band from the scalar design-space
    /// `edge_width` to a screen-space band derived from the distance gradient,
    /// which fixes the convex-corner flare at grazing angles.
    aa_band:          u32,
}

/// Text material extension over `StandardMaterial`.
#[derive(Asset, AsBindGroup, Clone, Debug, TypePath)]
#[bind_group_data(TextExtensionKey)]
pub struct TextExtension {
    /// Shader uniforms.
    #[uniform(100)]
    uniforms:          TextUniform,
    /// Shared glyph-atlas band-packed quadratic curve records.
    #[storage(101, read_only)]
    curves:            Handle<ShaderBuffer>,
    /// Shared glyph-atlas horizontal/vertical band records.
    #[storage(102, read_only)]
    bands:             Handle<ShaderBuffer>,
    /// Shared glyph-atlas records, indexed by each glyph record's `atlas_index`.
    #[storage(103, read_only)]
    glyphs:            Handle<ShaderBuffer>,
    /// Per-glyph instance records read by the vertex-pulling stage.
    #[storage(104, read_only, visibility(vertex))]
    instances:         Handle<ShaderBuffer>,
    /// Per-run records (world transform, fill color, render mode, depth
    /// nudge) read by the vertex-pulling stages.
    #[storage(105, read_only, visibility(vertex, fragment))]
    run_records:       Handle<ShaderBuffer>,
    /// Routes this material's vertex stages (main, prepass, shadow) through
    /// `analytic_path_vertex_pull.wgsl` instead of the standard mesh vertex stage.
    vertex_pull:       bool,
    /// Vertex-pull debug aid: displaces each quad by its recovered glyph
    /// index, making the slab-base subtraction visible as a staircase.
    debug_glyph_index: bool,
}

/// Pipeline-specialization key for [`TextExtension`]: which vertex stage a
/// material compiles against.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct TextExtensionKey {
    /// Mirror of [`TextExtension::vertex_pull`].
    vertex_pull:       bool,
    /// Mirror of [`TextExtension::debug_glyph_index`].
    debug_glyph_index: bool,
}

impl From<&TextExtension> for TextExtensionKey {
    fn from(extension: &TextExtension) -> Self {
        Self {
            vertex_pull:       extension.vertex_pull,
            debug_glyph_index: extension.debug_glyph_index,
        }
    }
}

impl MaterialExtension for TextExtension {
    fn fragment_shader() -> ShaderRef { ANALYTIC_PATH_SHADER_PATH.into() }

    fn prepass_fragment_shader() -> ShaderRef { ANALYTIC_PATH_SHADER_PATH.into() }

    // Standing contract of the text path: the camera depth prepass cannot
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
            if key.bind_group_data.debug_glyph_index {
                descriptor
                    .vertex
                    .shader_defs
                    .push("GLYPH_PULL_DEBUG_INDEX".into());
            }
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
pub(crate) struct BatchTextMaterialInput {
    /// Base material settings.
    pub base:              StandardMaterial,
    /// Placeholder fill color; vertex-pulling fragments read per-run color from
    /// `run_records`.
    pub fill_color:        Vec4,
    /// Placeholder render mode; vertex-pulling fragments read per-run mode from
    /// `run_records`.
    pub render_mode:       RenderMode,
    /// Per-layer depth offset for coplanar OIT layer ordering.
    pub oit_depth_offset:  f32,
    /// Whether the material supersamples the glyph footprint.
    pub supersample:       bool,
    /// Whether the material uses the screen-space anisotropic edge band.
    pub aa_band:           bool,
    /// Shared glyph-atlas band-packed quadratic curve records.
    pub curves:            Handle<ShaderBuffer>,
    /// Shared glyph-atlas horizontal/vertical band records.
    pub bands:             Handle<ShaderBuffer>,
    /// Shared glyph-atlas records, indexed by each glyph record's `atlas_index`.
    pub glyphs:            Handle<ShaderBuffer>,
    /// Per-glyph instance records read by the vertex-pulling stage.
    pub instances:         Handle<ShaderBuffer>,
    /// Per-run records read by the vertex-pulling stage.
    pub run_records:       Handle<ShaderBuffer>,
    /// Whether to enable the glyph-index staircase debug displacement.
    pub debug_glyph_index: bool,
}

/// Creates a vertex-pulling `TextMaterial` for one batch.
#[must_use]
pub(crate) fn batch_text_material(input: BatchTextMaterialInput) -> TextMaterial {
    let BatchTextMaterialInput {
        base,
        fill_color,
        render_mode,
        oit_depth_offset,
        supersample,
        aa_band,
        curves,
        bands,
        glyphs,
        instances,
        run_records,
        debug_glyph_index,
    } = input;
    ExtendedMaterial {
        base,
        extension: TextExtension {
            uniforms: TextUniform {
                fill_color,
                render_mode: u32::from(render_mode),
                oit_depth_offset,
                supersample: u32::from(supersample),
                aa_band: u32::from(aa_band),
            },
            curves,
            bands,
            glyphs,
            instances,
            run_records,
            vertex_pull: true,
            debug_glyph_index,
        },
    }
}

/// Repoints a batch material at replacement record buffers after capacity
/// growth.
pub(crate) fn set_batch_text_material_buffers(
    material: &mut TextMaterial,
    instances: Handle<ShaderBuffer>,
    run_records: Handle<ShaderBuffer>,
) {
    material.extension.instances = instances;
    material.extension.run_records = run_records;
}

/// Repoints a text material at replacement shared-atlas buffers after the
/// atlas grows. The batch record buffers (bindings 104/105) are per-batch,
/// not atlas-owned, so they are untouched here.
pub(crate) fn set_text_material_atlas(
    material: &mut TextMaterial,
    curves: Handle<ShaderBuffer>,
    bands: Handle<ShaderBuffer>,
    glyphs: Handle<ShaderBuffer>,
) {
    material.extension.curves = curves;
    material.extension.bands = bands;
    material.extension.glyphs = glyphs;
}

/// Updates the shader anti-aliasing switches on a text material.
pub(crate) fn set_text_material_anti_alias(
    material: &mut TextMaterial,
    supersample: bool,
    aa_band: bool,
) {
    material.extension.uniforms.supersample = u32::from(supersample);
    material.extension.uniforms.aa_band = u32::from(aa_band);
}

/// Toggles the vertex-pull glyph-index debug displacement and returns the new
/// state.
#[cfg(feature = "batch_proof")]
pub(crate) const fn toggle_text_material_debug_glyph_index(material: &mut TextMaterial) -> bool {
    material.extension.debug_glyph_index = !material.extension.debug_glyph_index;
    material.extension.debug_glyph_index
}
