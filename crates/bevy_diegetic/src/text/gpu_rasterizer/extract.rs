//! Main↔render world plumbing for the GPU rasterizer.
//!
//! Most resources travel through `ExtractResourcePlugin<T>` (registered
//! in `mod.rs`). The render→main completion path needs a custom
//! drain-and-trigger step because Bevy 0.18 events do not auto-cross
//! the world boundary.

use bevy::ecs::system::Commands;
use bevy::ecs::system::Res;
use bevy::ecs::system::ResMut;

use super::super::atlas_slot::AtlasSlot;
use super::dispatch::GpuGlyphCompletionBuffer;
use super::dispatch::RenderAtlasPages;
use super::request::BuiltRequest;
use super::request::GpuGlyphCompleted;
use super::request::GpuGlyphRequestQueue;
use super::request::GpuGlyphRequestReceiver;

/// Main-world post-extract cleanup: clears the request queue so the
/// next frame's main-world enqueues start from empty.
///
/// Without this, every request would be re-dispatched across multiple
/// frames until the queue grows unbounded.
pub(super) fn clear_main_request_queue(mut queue: ResMut<GpuGlyphRequestQueue>) {
    queue.pending.clear();
}

/// Main-world system: drains the worker→main mpsc channel populated by
/// spawned edge-build tasks. Successfully-built requests land in
/// [`GpuGlyphRequestQueue`] for the next frame's extract; invisible
/// glyphs are recorded directly on the atlas.
pub(super) fn drain_request_channel(
    receiver: Res<GpuGlyphRequestReceiver>,
    mut queue: ResMut<GpuGlyphRequestQueue>,
    mut slot: ResMut<AtlasSlot>,
) {
    let messages = receiver.drain();
    for msg in messages {
        match msg {
            BuiltRequest::Built(req) => queue.pending.push(*req),
            BuiltRequest::Invisible(key) => {
                slot.rasterize_target_mut()
                    .insert_completed_gpu_invisible(key);
            },
        }
    }
}

/// Main-world system: drains the render-extracted
/// [`GpuGlyphCompletionBuffer`] and fires per-record
/// [`GpuGlyphCompleted`] events via `commands.trigger`.
///
/// The atlas-side observer registered in `mod.rs::build` finalizes
/// each event into a `insert_completed_gpu` call.
pub(super) fn drain_gpu_completions(buffer: Res<GpuGlyphCompletionBuffer>, mut commands: Commands) {
    let records = buffer.drain();
    for record in records {
        commands.trigger(GpuGlyphCompleted {
            key:          record.key,
            bitmap_size:  record.bitmap_size,
            bearing:      record.bearing,
            atlas_origin: record.atlas_origin,
            page_index:   record.page_index,
        });
    }
}

/// Main-world system that mirrors atlas page handles into the
/// [`RenderAtlasPages`] resource so the dispatcher can resolve them
/// through `RenderAssets<GpuImage>`.
pub(super) fn sync_render_atlas_pages(slot: Res<AtlasSlot>, mut pages: ResMut<RenderAtlasPages>) {
    let atlas = slot.active();
    pages.pages.clear();
    for i in 0..atlas.page_count() {
        if let Some(handle) = atlas.image_handle(i as u32) {
            pages.pages.push(handle.clone());
        }
    }
}
