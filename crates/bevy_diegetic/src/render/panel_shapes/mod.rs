//! Batched rendering for panel-owned line primitives.

mod batching;
mod path;
mod primitive;
mod relationship;

use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;

use self::batching::DiegeticPanelShapeBatch;
use self::batching::PanelShapeBatchStore;
use self::batching::commit_panel_line_batch_buffers;
use self::batching::reconcile_panel_line_batches;
use self::batching::update_panel_line_batch_bounds;
use super::PanelChildSystems;
use super::material_table;
use super::material_table::BatchResourcesReady;

/// Plugin that adds batched panel-line rendering.
pub(super) struct PanelShapePlugin;

impl Plugin for PanelShapePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PanelShapeBatchStore>().add_systems(
            PostUpdate,
            (
                reconcile_panel_line_batches,
                material_table::register_path_batch_materials::<DiegeticPanelShapeBatch>,
                update_panel_line_batch_bounds,
                commit_panel_line_batch_buffers,
            )
                .chain()
                .in_set(PanelChildSystems::Build)
                .in_set(BatchResourcesReady)
                .before(VisibilitySystems::CheckVisibility),
        );
    }
}
