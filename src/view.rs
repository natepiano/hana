use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy_render::Extract;
use bevy_render::batching::gpu_preprocessing::GpuPreprocessingSupport;
use bevy_render::render_phase::ViewBinnedRenderPhases;
use bevy_render::view::RetainedViewEntity;

use super::constants::PRIMARY_SUBVIEW_INDEX;
use super::mask::HullOutlinePhase;
use super::mask::JumpFloodOutlinePhase;

pub(crate) fn update_views(
    mut outline_phases: ResMut<ViewBinnedRenderPhases<JumpFloodOutlinePhase>>,
    mut hull_outline_phases: ResMut<ViewBinnedRenderPhases<HullOutlinePhase>>,
    camera_query: Extract<Query<(Entity, &Camera), With<Camera3d>>>,
    gpu_preprocessing_support: Res<GpuPreprocessingSupport>,
    mut live_entities: Local<HashSet<RetainedViewEntity>>,
) {
    live_entities.clear();

    for (main_entity, camera) in camera_query.iter() {
        if !camera.is_active {
            continue;
        }

        let retained_view_entity =
            RetainedViewEntity::new(main_entity.into(), None, PRIMARY_SUBVIEW_INDEX);
        outline_phases.prepare_for_new_frame(
            retained_view_entity,
            gpu_preprocessing_support.max_supported_mode,
        );
        hull_outline_phases.prepare_for_new_frame(
            retained_view_entity,
            gpu_preprocessing_support.max_supported_mode,
        );

        live_entities.insert(retained_view_entity);
    }
    outline_phases.retain(|view_entity, _| live_entities.contains(view_entity));
    hull_outline_phases.retain(|view_entity, _| live_entities.contains(view_entity));
}
