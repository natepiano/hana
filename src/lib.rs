//! Bevy plugin for rendering mesh outlines using jump-flood and hull-extrusion methods.

mod camera;
mod compose;
mod constants;
mod extract;
mod flood;
mod hull_pipeline;
mod indexing_mode;
mod mask;
mod mask_pipeline;
mod node;
mod outline;
mod outline_builder;
mod outline_normals;
mod propagation;
mod queue;
mod render;
mod shaders;
mod texture;
mod uniforms;
mod view;

use bevy::core_pipeline::core_3d::graph::Core3d;
use bevy::core_pipeline::core_3d::graph::Node3d;
use bevy::pbr;
use bevy::prelude::*;
use bevy_render::Render;
use bevy_render::RenderApp;
use bevy_render::RenderDebugFlags;
use bevy_render::RenderSystems;
use bevy_render::extract_component::ExtractComponentPlugin;
use bevy_render::render_graph::RenderGraphExt;
use bevy_render::render_graph::ViewNodeRunner;
use bevy_render::render_phase::AddRenderCommand;
use bevy_render::render_phase::BinnedRenderPhasePlugin;
use bevy_render::render_phase::DrawFunctions;
use bevy_render::render_phase::ViewBinnedRenderPhases;
use bevy_render::render_resource::GpuArrayBuffer;
use bevy_render::render_resource::SpecializedMeshPipelines;
use bevy_render::renderer::RenderDevice;
pub use camera::OutlineCamera;
use compose::ComposeOutputPipeline;
pub use constants::ATTRIBUTE_OUTLINE_NORMAL;
use extract::ActiveOutlineModes;
use extract::ExtractedOutlineUniforms;
use flood::JumpFloodPipeline;
use hull_pipeline::HullPipeline;
use mask::HullOutlinePhase;
use mask::JfaOutlinePhase;
use mask_pipeline::MeshMaskPipeline;
use node::OutlineNode;
use node::OutlineRenderGraphNode;
pub use outline::LineStyle;
pub use outline::NoOutline;
pub use outline::Outline;
pub use outline::OutlineActivity;
pub use outline::OutlineMethod;
pub use outline::OverlapMode;
pub use outline_builder::HullModeState;
pub use outline_builder::JumpFloodState;
pub use outline_builder::OutlineBuilder;
pub use outline_builder::OutlineModeState;
pub use outline_builder::ScreenHullState;
pub use outline_builder::WorldHullState;
pub use outline_normals::generate_outline_normals;
use render::DrawHull;
use render::DrawOutline;
use render::HullOutlineBindGroup;
use render::HullOutlineUniformBuffer;
use render::OutlineBindGroup;
use render::OutlineUniformBuffer;
use shaders::ShaderPlugin;

/// Bevy plugin that registers outline rendering systems, pipelines, and render graph nodes.
pub struct LiminalPlugin;

impl Plugin for LiminalPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ShaderPlugin,
            ExtractComponentPlugin::<OutlineCamera>::default(),
        ));

        // Propagation observers
        app.add_observer(propagation::propagate_outline_to_descendants);
        app.add_observer(propagation::propagate_outline_on_child_added);
        app.add_observer(propagation::propagate_outline_on_mesh_added);
        app.add_observer(propagation::propagate_outline_on_scene_ready);
        app.add_observer(propagation::remove_outline_from_descendants);

        // Outline normal generation observers
        app.add_observer(outline_normals::generate_normals_on_outline_added);
        app.add_observer(outline_normals::generate_normals_on_mesh_added);

        // Change detection for propagated outlines
        app.add_systems(PostUpdate, propagation::sync_propagated_outlines);

        // Ensure the main pass depth texture has TEXTURE_BINDING so the compose
        // shader can sample it for correct occlusion of transmissive/transparent geometry.
        app.add_observer(camera::configure_outline_camera_depth_texture);

        app.add_plugins((
            BinnedRenderPhasePlugin::<JfaOutlinePhase, MeshMaskPipeline>::new(
                RenderDebugFlags::default(),
            ),
            BinnedRenderPhasePlugin::<HullOutlinePhase, HullPipeline>::new(
                RenderDebugFlags::default(),
            ),
        ));

        app.sub_app_mut(RenderApp)
            .init_resource::<DrawFunctions<JfaOutlinePhase>>()
            .init_resource::<DrawFunctions<HullOutlinePhase>>()
            .init_resource::<SpecializedMeshPipelines<MeshMaskPipeline>>()
            .init_resource::<SpecializedMeshPipelines<HullPipeline>>()
            .init_resource::<ViewBinnedRenderPhases<JfaOutlinePhase>>()
            .init_resource::<ViewBinnedRenderPhases<HullOutlinePhase>>()
            .init_resource::<OutlineBindGroup>()
            .init_resource::<HullOutlineBindGroup>()
            .init_resource::<ActiveOutlineModes>()
            .init_resource::<ExtractedOutlineUniforms>()
            .add_systems(
                ExtractSchedule,
                (
                    extract::extract_outline_uniforms,
                    view::update_views.after(pbr::extract_skins),
                ),
            )
            .add_systems(
                Render,
                (
                    extract::update_active_outline_modes
                        .in_set(RenderSystems::Queue)
                        .before(RenderSystems::QueueMeshes),
                    queue::queue_outline.in_set(RenderSystems::QueueMeshes),
                    queue::queue_hull_outline.in_set(RenderSystems::QueueMeshes),
                    render::prepare_outline_buffer.in_set(RenderSystems::PrepareResources),
                    render::prepare_hull_outline_buffer.in_set(RenderSystems::PrepareResources),
                    (
                        flood::prepare_flood_settings,
                        texture::prepare_flood_textures,
                        render::prepare_outline_bind_group.after(texture::prepare_flood_textures),
                        render::prepare_hull_outline_bind_group,
                        render::prepare_hull_depth_view_bind_groups
                            .after(texture::prepare_flood_textures),
                    )
                        .in_set(RenderSystems::PrepareBindGroups),
                ),
            )
            .add_render_command::<JfaOutlinePhase, DrawOutline>()
            .add_render_command::<HullOutlinePhase, DrawHull>()
            .add_render_graph_node::<ViewNodeRunner<OutlineNode>>(
                Core3d,
                OutlineRenderGraphNode::OutlineNode,
            )
            .add_render_graph_edges(
                Core3d,
                (
                    Node3d::EndMainPass,
                    OutlineRenderGraphNode::OutlineNode,
                    Node3d::Bloom,
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        let render_device = render_app.world().resource::<RenderDevice>();
        let outline_uniform_buffer =
            OutlineUniformBuffer(GpuArrayBuffer::new(&render_device.limits()));
        let hull_outline_uniform_buffer =
            HullOutlineUniformBuffer(GpuArrayBuffer::new(&render_device.limits()));

        render_app
            .insert_resource(outline_uniform_buffer)
            .insert_resource(hull_outline_uniform_buffer)
            .init_resource::<MeshMaskPipeline>()
            .init_resource::<HullPipeline>()
            .init_resource::<JumpFloodPipeline>()
            .init_resource::<ComposeOutputPipeline>();
    }
}
