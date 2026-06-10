//! Batched rendering for panel-owned line primitives.

mod batching;
mod path;
mod primitive;

use bevy::prelude::*;

use self::batching::PanelLineBatchStore;
use self::batching::commit_panel_line_batch_buffers;
use self::batching::reconcile_panel_line_batches;
use self::batching::update_panel_line_batch_bounds;
use super::PanelChildSystems;

/// Plugin that adds batched panel-line rendering.
pub(super) struct PanelLinePlugin;

impl Plugin for PanelLinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PanelLineBatchStore>().add_systems(
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
