use bevy::ecs::system::SystemParamItem;
use bevy::ecs::system::lifetimeless::SRes;
use bevy::mesh::MeshVertexAttributeId;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::MeshInputUniform;
use bevy::pbr::MeshPipeline;
use bevy::pbr::MeshPipelineKey;
use bevy::pbr::MeshUniform;
use bevy::pbr::RenderMeshInstances;
use bevy::pbr::SkinUniforms;
use bevy::prelude::*;
use bevy::shader::ShaderDefVal;
use bevy_render::batching::GetBatchData;
use bevy_render::batching::GetFullBatchData;
use bevy_render::batching::gpu_preprocessing::IndirectParametersCpuMetadata;
use bevy_render::batching::gpu_preprocessing::UntypedPhaseIndirectParametersBuffers;
use bevy_render::mesh::RenderMesh;
use bevy_render::mesh::allocator::MeshAllocator;
use bevy_render::render_asset::RenderAssets;
use bevy_render::render_resource::BindGroupLayoutDescriptor;
use bevy_render::render_resource::BindGroupLayoutEntries;
use bevy_render::render_resource::BlendState;
use bevy_render::render_resource::ColorTargetState;
use bevy_render::render_resource::ColorWrites;
use bevy_render::render_resource::CompareFunction;
use bevy_render::render_resource::DepthBiasState;
use bevy_render::render_resource::Face;
use bevy_render::render_resource::FragmentState;
use bevy_render::render_resource::GpuArrayBuffer;
use bevy_render::render_resource::RenderPipelineDescriptor;
use bevy_render::render_resource::Sampler;
use bevy_render::render_resource::SamplerBindingType;
use bevy_render::render_resource::SamplerDescriptor;
use bevy_render::render_resource::ShaderStages;
use bevy_render::render_resource::SpecializedMeshPipeline;
use bevy_render::render_resource::SpecializedMeshPipelineError;
use bevy_render::render_resource::TextureFormat;
use bevy_render::render_resource::TextureSampleType;
use bevy_render::render_resource::VertexAttribute;
use bevy_render::render_resource::VertexFormat;
use bevy_render::render_resource::binding_types;
use bevy_render::renderer::RenderDevice;
use bevy_render::sync_world::MainEntity;
use bevy_render::view::ViewTarget;
use nonmax::NonMaxU32;

use super::constants::ATTRIBUTE_OUTLINE_NORMAL;
use super::constants::FRAGMENT_SHADER_ENTRY_POINT;
use super::constants::HAS_OUTLINE_NORMALS_SHADER_DEF;
use super::constants::HULL_DEPTH_BIND_GROUP_LAYOUT_LABEL;
use super::constants::HULL_OUTLINE_INSTANCE_BIND_GROUP_LAYOUT_LABEL;
use super::constants::HULL_OUTLINE_PIPELINE_LABEL;
use super::constants::HULL_SHADER_HANDLE;
use super::constants::MISSING_BATCH_SET_INDEX;
use super::constants::OUTLINE_NORMAL_SHADER_LOCATION;
use super::constants::PER_OBJECT_BUFFER_BATCH_SIZE_SHADER_DEF;
use super::indexing_mode::IndexingMode;
use super::uniforms::OutlineUniform;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DynamicRange {
    Sdr,
    Hdr,
}

impl DynamicRange {
    const fn is_hdr(self) -> bool { matches!(self, Self::Hdr) }
}

impl From<bool> for DynamicRange {
    fn from(is_hdr: bool) -> Self { if is_hdr { Self::Hdr } else { Self::Sdr } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum OutlineNormalPresence {
    Present,
    Absent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct HullPipelineKey {
    pub(crate) mesh:                    MeshPipelineKey,
    pub(crate) dynamic_range:           DynamicRange,
    pub(crate) outline_normal_presence: OutlineNormalPresence,
}

#[derive(Resource)]
pub(crate) struct HullPipeline {
    pub(crate) mesh:                         MeshPipeline,
    pub(crate) outline_layout:               BindGroupLayoutDescriptor,
    pub(crate) depth_layout:                 BindGroupLayoutDescriptor,
    pub(crate) per_object_buffer_batch_size: Option<u32>,
    pub(crate) occlusion_sampler:            Sampler,
}

impl FromWorld for HullPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>().clone();

        let outline_layout = BindGroupLayoutDescriptor::new(
            HULL_OUTLINE_INSTANCE_BIND_GROUP_LAYOUT_LABEL,
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (GpuArrayBuffer::<OutlineUniform>::binding_layout(
                    &render_device.limits(),
                ),),
            ),
        );
        let depth_layout = BindGroupLayoutDescriptor::new(
            HULL_DEPTH_BIND_GROUP_LAYOUT_LABEL,
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    binding_types::sampler(SamplerBindingType::Filtering),
                    binding_types::texture_depth_2d(),
                    binding_types::texture_2d(TextureSampleType::Float { filterable: true }),
                ),
            ),
        );
        let per_object_buffer_batch_size =
            GpuArrayBuffer::<OutlineUniform>::batch_size(&render_device.limits());

        Self {
            mesh: MeshPipeline::from_world(world),
            outline_layout,
            depth_layout,
            per_object_buffer_batch_size,
            occlusion_sampler: render_device.create_sampler(&SamplerDescriptor::default()),
        }
    }
}

impl SpecializedMeshPipeline for HullPipeline {
    type Key = HullPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut descriptor = self.mesh.specialize(key.mesh, layout)?;

        descriptor.vertex.shader = HULL_SHADER_HANDLE;

        let mut shader_defs = vec![];
        if let Some(per_object_buffer_batch_size) = self.per_object_buffer_batch_size {
            shader_defs.push(ShaderDefVal::UInt(
                PER_OBJECT_BUFFER_BATCH_SIZE_SHADER_DEF.into(),
                per_object_buffer_batch_size,
            ));
        }

        if matches!(key.outline_normal_presence, OutlineNormalPresence::Present) {
            shader_defs.push(ShaderDefVal::Bool(
                HAS_OUTLINE_NORMALS_SHADER_DEF.into(),
                true,
            ));

            if let Some(index) = layout
                .0
                .attribute_ids()
                .iter()
                .position(|id| *id == MeshVertexAttributeId::from(ATTRIBUTE_OUTLINE_NORMAL))
            {
                let mesh_layout = layout.0.layout();
                let attr = &mesh_layout.attributes[index];
                descriptor.vertex.buffers[0]
                    .attributes
                    .push(VertexAttribute {
                        format:          VertexFormat::Float32x3,
                        offset:          attr.offset,
                        shader_location: OUTLINE_NORMAL_SHADER_LOCATION,
                    });
            }
        }

        descriptor.vertex.shader_defs.extend(shader_defs.clone());

        let color_format = if key.dynamic_range.is_hdr() {
            ViewTarget::TEXTURE_FORMAT_HDR
        } else {
            TextureFormat::bevy_default()
        };

        descriptor.fragment = Some(FragmentState {
            shader: HULL_SHADER_HANDLE,
            shader_defs,
            entry_point: Some(FRAGMENT_SHADER_ENTRY_POINT.into()),
            targets: vec![Some(ColorTargetState {
                format:     color_format,
                blend:      Some(BlendState::ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
        });

        if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
            depth_stencil.depth_write_enabled = true;
            depth_stencil.depth_compare = CompareFunction::GreaterEqual;
            depth_stencil.bias = DepthBiasState {
                constant:    0,
                slope_scale: 0.0,
                clamp:       0.0,
            };
        }
        descriptor.label = Some(HULL_OUTLINE_PIPELINE_LABEL.into());
        descriptor.layout.push(self.outline_layout.clone());
        descriptor.layout.push(self.depth_layout.clone());
        descriptor.primitive.cull_mode = Some(Face::Front);

        Ok(descriptor)
    }
}

impl GetBatchData for HullPipeline {
    type Param = (
        SRes<RenderMeshInstances>,
        SRes<RenderAssets<RenderMesh>>,
        SRes<MeshAllocator>,
        SRes<SkinUniforms>,
    );
    type CompareData = AssetId<Mesh>;
    type BufferData = MeshUniform;

    fn get_batch_data(
        (mesh_instances, _, mesh_allocator, skin_uniforms): &SystemParamItem<Self::Param>,
        (_, main_entity): (Entity, MainEntity),
    ) -> Option<(Self::BufferData, Option<Self::CompareData>)> {
        let RenderMeshInstances::CpuBuilding(ref mesh_instances) = **mesh_instances else {
            error!("`get_batch_data` should never be called in GPU mesh uniform building mode");
            return None;
        };
        let mesh_instance = mesh_instances.get(&main_entity)?;
        let first_vertex_index = mesh_allocator
            .mesh_vertex_slice(&mesh_instance.mesh_asset_id)
            .map_or(0, |slice| slice.range.start);

        let current_skin_index = skin_uniforms.skin_index(main_entity);
        let material_bind_group_index = mesh_instance.material_bindings_index;

        Some((
            MeshUniform::new(
                &mesh_instance.transforms,
                first_vertex_index,
                material_bind_group_index.slot,
                None,
                current_skin_index,
                Some(mesh_instance.tag),
            ),
            Some(mesh_instance.mesh_asset_id),
        ))
    }
}

impl GetFullBatchData for HullPipeline {
    type BufferInputData = MeshInputUniform;

    fn get_index_and_compare_data(
        (mesh_instances, _, _, _): &SystemParamItem<Self::Param>,
        main_entity: MainEntity,
    ) -> Option<(NonMaxU32, Option<Self::CompareData>)> {
        let RenderMeshInstances::GpuBuilding(ref mesh_instances) = **mesh_instances else {
            error!(
                "`get_index_and_compare_data` should never be called in CPU mesh uniform building mode"
            );
            return None;
        };
        let mesh_instance = mesh_instances.get(&main_entity)?;
        Some((
            mesh_instance.current_uniform_index,
            Some(mesh_instance.mesh_asset_id),
        ))
    }

    fn get_binned_batch_data(
        (mesh_instances, _, mesh_allocator, skin_uniforms): &SystemParamItem<Self::Param>,
        main_entity: MainEntity,
    ) -> Option<Self::BufferData> {
        let RenderMeshInstances::CpuBuilding(ref mesh_instances) = **mesh_instances else {
            error!(
                "`get_binned_batch_data` should never be called in GPU mesh uniform building mode"
            );
            return None;
        };
        let mesh_instance = mesh_instances.get(&main_entity)?;
        let first_vertex_index = mesh_allocator
            .mesh_vertex_slice(&mesh_instance.mesh_asset_id)
            .map_or(0, |slice| slice.range.start);

        let current_skin_index = skin_uniforms.skin_index(main_entity);

        Some(MeshUniform::new(
            &mesh_instance.transforms,
            first_vertex_index,
            mesh_instance.material_bindings_index.slot,
            None,
            current_skin_index,
            Some(mesh_instance.tag),
        ))
    }

    fn get_binned_index(
        (mesh_instances, _, _, _): &SystemParamItem<Self::Param>,
        main_entity: MainEntity,
    ) -> Option<NonMaxU32> {
        let RenderMeshInstances::GpuBuilding(ref mesh_instances) = **mesh_instances else {
            error!("`get_binned_index` should never be called in CPU mesh uniform building mode");
            return None;
        };
        mesh_instances
            .get(&main_entity)
            .map(|entity| entity.current_uniform_index)
    }

    fn write_batch_indirect_parameters_metadata(
        indexed: bool,
        base_output_index: u32,
        batch_set_index: Option<NonMaxU32>,
        phase_indirect_parameters_buffers: &mut UntypedPhaseIndirectParametersBuffers,
        indirect_parameters_offset: u32,
    ) {
        let indirect_parameters = IndirectParametersCpuMetadata {
            base_output_index,
            batch_set_index: batch_set_index.map_or(MISSING_BATCH_SET_INDEX, u32::from),
        };

        IndexingMode::from(indexed).write_metadata(
            phase_indirect_parameters_buffers,
            indirect_parameters_offset,
            indirect_parameters,
        );
    }
}
