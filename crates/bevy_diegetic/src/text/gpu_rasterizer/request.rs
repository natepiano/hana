//! Per-atlas request and completion plumbing for GPU glyph rasterization.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;

use bevy::image::Image;
use bevy::math::UVec2;
use bevy::math::Vec2;
use bevy::prelude::Handle;
use bevy::prelude::Resource;
use bevy::render::extract_resource::ExtractResource;

use super::edges::GpuGlyphRequestBody;
use crate::text::atlas::GlyphKey;
use crate::text::msdf_rasterizer::DistanceField;

/// Single built glyph request consumed by the render-world dispatch system.
#[derive(Clone, Debug)]
pub(crate) struct GpuGlyphRequest {
    /// Lookup key the completion record echoes back.
    pub key:            GlyphKey,
    /// Edges, bitmap dims, and bearings produced by [`super::edges::build_edge_buffer`].
    pub body:           GpuGlyphRequestBody,
    /// SDF distance range in pixels.
    pub sdf_range:      f32,
    /// SDF vs MSDF.
    pub distance_field: DistanceField,
    /// Top-left texel of the bitmap interior on the target page.
    pub atlas_origin:   UVec2,
    /// Atlas page index the bitmap will be written into.
    pub page_index:     u32,
}

/// Message sent from a spawned edge-build task back to its atlas.
#[derive(Clone, Debug)]
pub(crate) enum BuiltGpuRequest {
    /// Visible glyph work ready for render-world dispatch.
    Built {
        request:     Box<GpuGlyphRequest>,
        completions: GpuCompletionSink,
    },
    /// The glyph has no outline, so the atlas should cache an invisible entry.
    Invisible { key: GlyphKey },
}

/// Completion record emitted by the render-world dispatcher.
#[derive(Clone, Copy, Debug)]
pub struct GpuGlyphCompletedRecord {
    pub key:          GlyphKey,
    pub bitmap_size:  UVec2,
    pub bearing:      Vec2,
    pub atlas_origin: UVec2,
    pub page_index:   u32,
}

/// Cloneable render-to-main completion sink owned by one atlas.
#[derive(Clone, Debug)]
pub(crate) struct GpuCompletionSink {
    inner: Arc<Mutex<Vec<GpuGlyphCompletedRecord>>>,
}

impl GpuCompletionSink {
    /// Appends a single completion record.
    pub fn push(&self, record: GpuGlyphCompletedRecord) {
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        guard.push(record);
    }

    /// Drains every queued completion record.
    #[must_use]
    pub fn drain(&self) -> Vec<GpuGlyphCompletedRecord> {
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut *guard)
    }
}

/// Main-owned GPU pipe attached to a single atlas.
pub(crate) struct AtlasGpuPipe {
    /// Worker-to-main edge-build results.
    pub built_tx:         Sender<BuiltGpuRequest>,
    /// Main-side receiver drained by atlas polling.
    pub built_rx:         Mutex<Receiver<BuiltGpuRequest>>,
    /// Render-to-main completion sink.
    pub completions:      GpuCompletionSink,
    /// Jobs waiting for the extract pass.
    pub pending_dispatch: Vec<GpuRenderJob>,
}

impl AtlasGpuPipe {
    /// Creates an empty per-atlas GPU pipe.
    #[must_use]
    pub fn new() -> Self {
        let (built_tx, built_rx) = mpsc::channel();
        Self {
            built_tx,
            built_rx: Mutex::new(built_rx),
            completions: GpuCompletionSink {
                inner: Arc::new(Mutex::new(Vec::new())),
            },
            pending_dispatch: Vec::new(),
        }
    }

    /// Drains built worker results into a temporary vector.
    #[must_use]
    pub fn drain_built(&self) -> Vec<BuiltGpuRequest> {
        let rx = self.built_rx.lock().unwrap_or_else(PoisonError::into_inner);
        let mut out = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            out.push(msg);
        }
        out
    }
}

/// Render-world job for one glyph.
#[derive(Clone, Debug)]
pub(crate) struct GpuRenderJob {
    /// Built edge data and atlas target coordinates.
    pub request:      GpuGlyphRequest,
    /// Exact image the compute shader writes to.
    pub image_handle: Handle<Image>,
    /// Per-atlas completion sink.
    pub completions:  GpuCompletionSink,
}

/// Main-to-render extract payload for GPU jobs.
#[derive(Resource, Default, Clone, ExtractResource)]
pub(super) struct GpuRenderJobExtract {
    pub pending: Vec<GpuRenderJob>,
}

/// Persistent render-world queue.
#[derive(Resource, Default)]
pub(super) struct GpuRenderJobQueue {
    pub pending: Vec<GpuRenderJob>,
}
