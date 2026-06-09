//! Batched rendering for panel-owned line primitives.

mod batching;
mod material;
mod primitive;

use bevy::asset::load_internal_asset;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;

use self::batching::PanelLineBatchStore;
use self::batching::commit_panel_line_batch_buffers;
use self::batching::reconcile_panel_line_batches;
use self::batching::update_panel_line_batch_bounds;
use self::material::PANEL_LINE_BATCH_SHADER_HANDLE;
use self::material::PanelLineBatchMaterial;
use super::PanelChildSystems;

/// Plugin that adds batched panel-line rendering.
pub(super) struct PanelLinePlugin;

impl Plugin for PanelLinePlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            PANEL_LINE_BATCH_SHADER_HANDLE,
            "panel_line_batch.wgsl",
            Shader::from_wgsl
        );
        app.init_resource::<PanelLineBatchStore>()
            .add_plugins(MaterialPlugin::<PanelLineBatchMaterial>::default())
            .add_systems(
                PostUpdate,
                (
                    reconcile_panel_line_batches,
                    update_panel_line_batch_bounds,
                    commit_panel_line_batch_buffers,
                )
                    .chain()
                    .in_set(PanelChildSystems::Build),
            );
    }
}
