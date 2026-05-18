//! Main-world request queue and render-to-main completion event for
//! GPU glyph rasterization.

use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::mpsc;

use bevy::math::UVec2;
use bevy::math::Vec2;
use bevy::prelude::Event;
use bevy::prelude::Resource;
use bevy::render::extract_resource::ExtractResource;

use super::super::atlas::GlyphKey;
use super::super::msdf_rasterizer::DistanceField;
use super::edges::GpuGlyphRequestBody;

/// Single queued glyph request, built on the main world and consumed
/// by the render-world dispatch system.
#[derive(Clone, Debug)]
pub(crate) struct GpuGlyphRequest {
    /// Lookup key the completion event echoes back so the observer can
    /// finalize metrics.
    pub key:            GlyphKey,
    /// Edges, bitmap dims, and bearings produced by [`super::edges::build_edge_buffer`].
    pub body:           GpuGlyphRequestBody,
    /// SDF distance range in pixels.
    pub sdf_range:      f32,
    /// SDF vs MSDF (Phase 1 only routes SDF; MSDF lands in Phase 2).
    pub distance_field: DistanceField,
    /// Top-left texel of the bitmap interior on the target page.
    pub atlas_origin:   UVec2,
    /// Atlas page index the bitmap will be written into.
    pub page_index:     u32,
}

/// Main-world resource — append-only queue of glyph requests.
///
/// Extracted into the render world each frame via
/// [`bevy::render::extract_resource::ExtractResourcePlugin`]; the
/// render-world dispatcher drains the cloned copy and submits compute
/// passes. A separate main-world system clears the original queue
/// post-extract.
#[derive(Resource, Default, Clone, ExtractResource)]
pub struct GpuGlyphRequestQueue {
    pub(crate) pending: Vec<GpuGlyphRequest>,
}

impl GpuGlyphRequestQueue {
    /// Returns the number of pending requests.
    #[must_use]
    pub fn len(&self) -> usize { self.pending.len() }

    /// Returns whether the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.pending.is_empty() }
}

/// Message sent from a spawned edge-build task back to the main world.
///
/// `Built` carries the finished request that should be queued for
/// GPU dispatch; `Invisible` reports that the glyph had no outline so
/// the atlas can record the sentinel `GlyphMetrics::INVISIBLE` and
/// stop the key from re-queueing.
#[derive(Clone, Debug)]
pub(crate) enum BuiltRequest {
    Built(Box<GpuGlyphRequest>),
    Invisible(GlyphKey),
}

/// Send half of the worker→main request channel.
///
/// Cloned into each spawned edge-build task by `enqueue_gpu_glyph`.
/// The matching [`GpuGlyphRequestReceiver`] is drained by the
/// main-world `drain_request_channel` system each frame.
#[derive(Resource, Clone)]
pub struct GpuGlyphRequestSender {
    pub(crate) sender: mpsc::Sender<BuiltRequest>,
}

/// Receive half of the worker→main request channel.
#[derive(Resource)]
pub(crate) struct GpuGlyphRequestReceiver {
    pub(crate) receiver: Mutex<mpsc::Receiver<BuiltRequest>>,
}

impl GpuGlyphRequestReceiver {
    /// Drains every message that finished since the last poll, returning
    /// them in arrival order.
    pub(crate) fn drain(&self) -> Vec<BuiltRequest> {
        let rx = self.receiver.lock().unwrap_or_else(PoisonError::into_inner);
        let mut out = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            out.push(msg);
        }
        out
    }
}

/// Event fired on the main world when a GPU compute pass finishes a
/// glyph. Observed by the atlas-side finalizer, which calls
/// [`super::super::atlas::GlyphAtlas::insert_completed_gpu`].
///
/// Carries the pre-allocated region details so the observer can
/// reconstruct the [`super::super::atlas::GlyphMetrics`] without
/// reading any render-world state.
#[derive(Event, Clone, Copy, Debug)]
pub struct GpuGlyphCompleted {
    /// Lookup key the dispatch was for.
    pub key:          GlyphKey,
    /// Bitmap dimensions in texels.
    pub bitmap_size:  UVec2,
    /// Em-units bearing reported by the edge builder.
    pub bearing:      Vec2,
    /// Top-left interior texel on the page.
    pub atlas_origin: UVec2,
    /// Atlas page index that was written.
    pub page_index:   u32,
}
