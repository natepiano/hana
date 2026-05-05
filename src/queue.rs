use bevy::core_pipeline::prepass::MotionVectorPrepass;
use bevy::core_pipeline::prepass::NormalPrepass;
use bevy::ecs::change_detection::Tick;
use bevy::pbr::MeshPipelineKey;
use bevy::pbr::RenderMeshInstances;
use bevy::prelude::*;
use bevy_render::batching::gpu_preprocessing::GpuPreprocessingSupport;
use bevy_render::mesh::RenderMesh;
use bevy_render::mesh::allocator::MeshAllocator;
use bevy_render::render_asset::RenderAssets;
use bevy_render::render_phase::BinnedRenderPhaseType;
use bevy_render::render_phase::DrawFunctions;
use bevy_render::render_phase::ViewBinnedRenderPhases;
use bevy_render::render_resource::PipelineCache;
use bevy_render::render_resource::SpecializedMeshPipelines;
use bevy_render::view::ExtractedView;
use bevy_render::view::RenderVisibleEntities;

use super::DrawHull;
use super::DrawOutline;
use super::camera::OutlineCamera;
use super::constants::ATTRIBUTE_OUTLINE_NORMAL;
use super::extract::ActiveOutlineModes;
use super::extract::ExtractedOutlineUniforms;
use super::hull_pipeline::HullPipeline;
use super::hull_pipeline::HullPipelineKey;
use super::hull_pipeline::OutlineNormalPresence;
use super::mask::HullOutlinePhase;
use super::mask::JfaOutlinePhase;
use super::mask::OutlineBatchSetKey;
use super::mask::OutlineBinKey;
use super::mask_pipeline::HullPresence;
use super::mask_pipeline::MaskPipelineKey;
use super::mask_pipeline::MeshMaskPipeline;
use super::outline::OutlineMethod;

pub(crate) fn queue_outline(
    extracted_outlines: Res<ExtractedOutlineUniforms>,
    draw_functions: Res<DrawFunctions<JfaOutlinePhase>>,
    mut outline_phases: ResMut<ViewBinnedRenderPhases<JfaOutlinePhase>>,
    mesh_outline_pipeline: Res<MeshMaskPipeline>,
    mut mesh_outline_pipelines: ResMut<SpecializedMeshPipelines<MeshMaskPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    mesh_allocator: Res<MeshAllocator>,
    render_meshes: Res<RenderAssets<RenderMesh>>,
    render_mesh_instances: Res<RenderMeshInstances>,
    gpu_preprocessing_support: Res<GpuPreprocessingSupport>,
    active: Res<ActiveOutlineModes>,
    views: Query<
        (
            Entity,
            &ExtractedView,
            &RenderVisibleEntities,
            &Msaa,
            Has<NormalPrepass>,
            Has<MotionVectorPrepass>,
        ),
        With<OutlineCamera>,
    >,
    mut change_tick: Local<Tick>,
) {
    let draw_function = draw_functions.read().id::<DrawOutline>();

    for (_, view, visible_entities, msaa, has_normal_prepass, has_motion_vector_prepass) in
        views.iter()
    {
        let Some(outline_phase) = outline_phases.get_mut(&view.retained_view_entity) else {
            continue;
        };

        let mut view_key = MeshPipelineKey::from_msaa_samples(msaa.samples())
            | MeshPipelineKey::DEPTH_PREPASS
            | MeshPipelineKey::from_hdr(view.hdr);

        if has_normal_prepass {
            view_key |= MeshPipelineKey::NORMAL_PREPASS;
        }
        if has_motion_vector_prepass {
            view_key |= MeshPipelineKey::MOTION_VECTOR_PREPASS;
        }

        for &(render_entity, main_entity) in visible_entities.get::<Mesh3d>() {
            if !extracted_outlines.by_main_entity.contains_key(&main_entity) {
                continue;
            }
            let Some(mesh_instance) = render_mesh_instances.render_mesh_queue_data(main_entity)
            else {
                tracing::warn!(target: "bevy_liminal", "No mesh instance found for entity {main_entity:?}");
                continue;
            };

            let (vertex_slab, index_slab) = mesh_allocator.mesh_slabs(&mesh_instance.mesh_asset_id);

            let Some(mesh) = render_meshes.get(mesh_instance.mesh_asset_id) else {
                tracing::warn!(target: "bevy_liminal", "No mesh found for entity {main_entity:?}");
                continue;
            };

            let mut mesh_key = view_key;
            mesh_key |= MeshPipelineKey::from_primitive_topology(mesh.primitive_topology())
                | MeshPipelineKey::from_bits_retain(mesh.key_bits.bits());

            let Ok(pipeline_id) = mesh_outline_pipelines.specialize(
                &pipeline_cache,
                &mesh_outline_pipeline,
                MaskPipelineKey {
                    mesh:          mesh_key,
                    hull_presence: if active.methods.has_hull() {
                        HullPresence::Present
                    } else {
                        HullPresence::Absent
                    },
                },
                &mesh.layout,
            ) else {
                tracing::warn!(target: "bevy_liminal", "Failed to specialize mesh pipeline");
                continue;
            };

            let next_change_tick = change_tick.get() + 1;
            change_tick.set(next_change_tick);

            outline_phase.add(
                OutlineBatchSetKey {
                    pipeline: pipeline_id,
                    draw_function,
                    vertex_slab: vertex_slab.unwrap_or_default(),
                    index_slab,
                },
                OutlineBinKey {
                    asset_id: mesh_instance.mesh_asset_id.untyped(),
                    main_entity,
                },
                (render_entity, main_entity),
                mesh_instance.current_uniform_index,
                BinnedRenderPhaseType::mesh(
                    mesh_instance.should_batch(),
                    &gpu_preprocessing_support,
                ),
                *change_tick,
            );
        }
    }
}

pub(crate) fn queue_hull_outline(
    active: Res<ActiveOutlineModes>,
    extracted_outlines: Res<ExtractedOutlineUniforms>,
    draw_functions: Res<DrawFunctions<HullOutlinePhase>>,
    mut outline_phases: ResMut<ViewBinnedRenderPhases<HullOutlinePhase>>,
    hull_pipeline: Res<HullPipeline>,
    mut hull_pipelines: ResMut<SpecializedMeshPipelines<HullPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    mesh_allocator: Res<MeshAllocator>,
    render_meshes: Res<RenderAssets<RenderMesh>>,
    render_mesh_instances: Res<RenderMeshInstances>,
    gpu_preprocessing_support: Res<GpuPreprocessingSupport>,
    views: Query<
        (
            Entity,
            &ExtractedView,
            &RenderVisibleEntities,
            &Msaa,
            Has<NormalPrepass>,
        ),
        With<OutlineCamera>,
    >,
    mut change_tick: Local<Tick>,
) {
    if !active.methods.has_hull() {
        return;
    }

    let draw_function = draw_functions.read().id::<DrawHull>();

    for (_, view, visible_entities, msaa, has_normal_prepass) in views.iter() {
        let Some(outline_phase) = outline_phases.get_mut(&view.retained_view_entity) else {
            continue;
        };

        let mut view_key = MeshPipelineKey::from_msaa_samples(msaa.samples())
            | MeshPipelineKey::DEPTH_PREPASS
            | MeshPipelineKey::from_hdr(view.hdr);

        if has_normal_prepass {
            view_key |= MeshPipelineKey::NORMAL_PREPASS;
        }

        for &(render_entity, main_entity) in visible_entities.get::<Mesh3d>() {
            let Some(outline) = extracted_outlines.by_main_entity.get(&main_entity) else {
                continue;
            };
            if !matches!(
                outline.outline_method,
                OutlineMethod::WorldHull | OutlineMethod::ScreenHull
            ) {
                continue;
            }

            let Some(mesh_instance) = render_mesh_instances.render_mesh_queue_data(main_entity)
            else {
                tracing::warn!(target: "bevy_liminal", "No mesh instance found for entity {main_entity:?}");
                continue;
            };

            let (vertex_slab, index_slab) = mesh_allocator.mesh_slabs(&mesh_instance.mesh_asset_id);

            let Some(mesh) = render_meshes.get(mesh_instance.mesh_asset_id) else {
                tracing::warn!(target: "bevy_liminal", "No mesh found for entity {main_entity:?}");
                continue;
            };

            let mut mesh_key = view_key;
            mesh_key |= MeshPipelineKey::from_primitive_topology(mesh.primitive_topology())
                | MeshPipelineKey::from_bits_retain(mesh.key_bits.bits());

            let Ok(pipeline_id) = hull_pipelines.specialize(
                &pipeline_cache,
                &hull_pipeline,
                HullPipelineKey {
                    mesh:                    mesh_key,
                    dynamic_range:           view.hdr.into(),
                    outline_normal_presence: if mesh.layout.0.contains(ATTRIBUTE_OUTLINE_NORMAL) {
                        OutlineNormalPresence::Present
                    } else {
                        OutlineNormalPresence::Absent
                    },
                },
                &mesh.layout,
            ) else {
                tracing::warn!(target: "bevy_liminal", "Failed to specialize hull mesh pipeline");
                continue;
            };

            let next_change_tick = change_tick.get() + 1;
            change_tick.set(next_change_tick);

            outline_phase.add(
                OutlineBatchSetKey {
                    pipeline: pipeline_id,
                    draw_function,
                    vertex_slab: vertex_slab.unwrap_or_default(),
                    index_slab,
                },
                OutlineBinKey {
                    asset_id: mesh_instance.mesh_asset_id.untyped(),
                    main_entity,
                },
                (render_entity, main_entity),
                mesh_instance.current_uniform_index,
                BinnedRenderPhaseType::mesh(
                    mesh_instance.should_batch(),
                    &gpu_preprocessing_support,
                ),
                *change_tick,
            );
        }
    }
}
