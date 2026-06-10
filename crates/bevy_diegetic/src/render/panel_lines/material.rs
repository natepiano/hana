//! Batched panel-line material.

use bevy::asset::Asset;
use bevy::asset::uuid_handle;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MATERIAL_BIND_GROUP_INDEX;
use bevy::pbr::MaterialExtension;
use bevy::pbr::MaterialExtensionKey;
use bevy::pbr::MaterialExtensionPipeline;
use bevy::pbr::StandardMaterial;
use bevy::prelude::AlphaMode;
use bevy::prelude::Handle;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::RenderPipelineDescriptor;
use bevy::render::render_resource::SpecializedMeshPipelineError;
use bevy::render::storage::ShaderBuffer;
use bevy::shader::Shader;
use bevy::shader::ShaderRef;

/// Vertex/fragment shader for panel-line vertex pulling.
pub(super) const PANEL_LINE_BATCH_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("7bb40ca7-c20a-42bf-8a70-983418623f99");

/// Material used by the batched panel-line renderer.
pub(super) type PanelLineBatchMaterial =
    ExtendedMaterial<StandardMaterial, PanelLineBatchExtension>;

/// Extension over `StandardMaterial` that supplies the line record buffer.
#[derive(Asset, AsBindGroup, Clone, Debug, TypePath)]
#[bind_group_data(PanelLineBatchExtensionKey)]
pub(super) struct PanelLineBatchExtension {
    /// Per-primitive records read by the vertex and fragment stages.
    #[storage(100, read_only, visibility(vertex, fragment))]
    records:     Handle<ShaderBuffer>,
    /// Routes this material through the vertex-pulling shader.
    vertex_pull: bool,
}

/// Pipeline-specialization key for [`PanelLineBatchExtension`].
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(super) struct PanelLineBatchExtensionKey {
    vertex_pull: bool,
}

impl From<&PanelLineBatchExtension> for PanelLineBatchExtensionKey {
    fn from(extension: &PanelLineBatchExtension) -> Self {
        Self {
            vertex_pull: extension.vertex_pull,
        }
    }
}

impl MaterialExtension for PanelLineBatchExtension {
    fn fragment_shader() -> ShaderRef { PANEL_LINE_BATCH_SHADER_HANDLE.into() }

    fn prepass_fragment_shader() -> ShaderRef { PANEL_LINE_BATCH_SHADER_HANDLE.into() }

    // Matches the text batch constraint: vertex-pull batches read the material
    // bind group in the vertex stage, while Bevy may strip that group from
    // depth-only pipelines.
    fn enable_prepass() -> bool { false }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if key.bind_group_data.vertex_pull && !material_group_is_stripped(descriptor) {
            descriptor.vertex.shader = PANEL_LINE_BATCH_SHADER_HANDLE;
        }
        Ok(())
    }
}

fn material_group_is_stripped(descriptor: &RenderPipelineDescriptor) -> bool {
    descriptor
        .layout
        .get(MATERIAL_BIND_GROUP_INDEX)
        .is_none_or(|material_layout| material_layout.entries.is_empty())
}

/// Inputs for one batched line material.
pub(super) struct BatchLineMaterialInput {
    /// Base material settings.
    pub base:       StandardMaterial,
    /// Per-batch record buffer.
    pub records:    Handle<ShaderBuffer>,
    /// Pipeline depth bias shared by the batch key.
    pub depth_bias: f32,
}

/// Creates a vertex-pulling line material for one batch.
#[must_use]
pub(super) fn batch_line_material(input: BatchLineMaterialInput) -> PanelLineBatchMaterial {
    let BatchLineMaterialInput {
        mut base,
        records,
        depth_bias,
    } = input;
    base.alpha_mode = AlphaMode::Mask(0.0);
    base.double_sided = true;
    base.cull_mode = None;
    base.depth_bias = depth_bias;
    ExtendedMaterial {
        base,
        extension: PanelLineBatchExtension {
            records,
            vertex_pull: true,
        },
    }
}

/// Repoints a batch material at replacement record buffers after capacity
/// growth.
pub(super) fn set_batch_line_material_buffer(
    material: &mut PanelLineBatchMaterial,
    records: Handle<ShaderBuffer>,
) {
    material.extension.records = records;
}
