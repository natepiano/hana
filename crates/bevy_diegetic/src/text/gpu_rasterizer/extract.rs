//! Main-to-render job collection for the GPU rasterizer.

use bevy::ecs::system::ResMut;

use super::request::GpuRenderJobExtract;
use crate::text::atlas_slot::AtlasSlot;

/// Drains per-atlas GPU jobs into the extracted render payload.
pub(super) fn collect_gpu_render_jobs(
    mut slot: ResMut<AtlasSlot>,
    mut extract: ResMut<GpuRenderJobExtract>,
) {
    extract.pending.clear();
    slot.drain_gpu_render_jobs(&mut extract.pending);
}
