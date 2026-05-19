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

/// Fields common to every GPU glyph request variant.
#[derive(Clone, Debug)]
pub(super) struct GpuGlyphRequestCommon {
    /// Lookup key the completion record echoes back.
    pub key:          GlyphKey,
    /// Edges, bitmap dims, and bearings produced by [`super::edges::build_edge_buffer`].
    pub body:         GpuGlyphRequestBody,
    /// SDF distance range in pixels.
    pub sdf_range:    f32,
    /// Top-left texel of the bitmap interior on the target page.
    pub atlas_origin: UVec2,
    /// Atlas page index the bitmap will be written into.
    pub page_index:   u32,
}

/// Built glyph request consumed by the render-world dispatch system.
///
/// The variant tag picks the compute pipeline (SDF vs MSDF). Variant
/// data carries everything needed for either kernel; on the MSDF
/// variant the `EdgeSegment::kind` bits 2–4 hold the channel mask.
#[derive(Clone, Debug)]
pub(super) enum GpuGlyphRequest {
    /// Single-channel SDF request.
    Sdf(GpuGlyphRequestCommon),
    /// Three-channel MSDF request with channel-coloured edges.
    Msdf(GpuGlyphRequestCommon),
    /// Four-channel MTSDF request. Generation reuses the MSDF kernel;
    /// the correction pass writes a signed-true-distance alpha so the
    /// fragment shader can clamp the RGB median to ±tolerance around it.
    Mtsdf(GpuGlyphRequestCommon),
}

impl GpuGlyphRequest {
    /// Returns a reference to the common fields shared by every variant.
    #[must_use]
    pub(super) const fn common(&self) -> &GpuGlyphRequestCommon {
        match self {
            Self::Sdf(c) | Self::Msdf(c) | Self::Mtsdf(c) => c,
        }
    }

    /// Returns the distance-field encoding this request targets.
    #[must_use]
    pub const fn distance_field(&self) -> DistanceField {
        match self {
            Self::Sdf(_) => DistanceField::Sdf,
            Self::Msdf(_) => DistanceField::Msdf,
            Self::Mtsdf(_) => DistanceField::Mtsdf,
        }
    }
}

/// Message sent from a spawned edge-build task back to its atlas.
#[derive(Clone, Debug)]
pub(crate) struct BuiltGpuRequest {
    kind: BuiltGpuRequestKind,
}

#[derive(Clone, Debug)]
enum BuiltGpuRequestKind {
    /// Visible glyph work ready for render-world dispatch.
    Built {
        request:     Box<GpuGlyphRequest>,
        completions: GpuCompletionSink,
    },
    /// The glyph has no outline, so the atlas should cache an invisible entry.
    Invisible { key: GlyphKey },
}

impl BuiltGpuRequest {
    #[must_use]
    pub(super) const fn built(
        request: Box<GpuGlyphRequest>,
        completions: GpuCompletionSink,
    ) -> Self {
        Self {
            kind: BuiltGpuRequestKind::Built {
                request,
                completions,
            },
        }
    }

    #[must_use]
    pub(super) const fn invisible(key: GlyphKey) -> Self {
        Self {
            kind: BuiltGpuRequestKind::Invisible { key },
        }
    }

    #[must_use]
    pub fn page_index(&self) -> Option<u32> {
        match &self.kind {
            BuiltGpuRequestKind::Built { request, .. } => Some(request.common().page_index),
            BuiltGpuRequestKind::Invisible { .. } => None,
        }
    }

    #[must_use]
    pub const fn invisible_key(&self) -> Option<GlyphKey> {
        match &self.kind {
            BuiltGpuRequestKind::Built { .. } => None,
            BuiltGpuRequestKind::Invisible { key } => Some(*key),
        }
    }

    #[must_use]
    pub fn into_render_job(self, image_handle: Handle<Image>) -> Option<GpuRenderJob> {
        match self.kind {
            BuiltGpuRequestKind::Built {
                request,
                completions,
            } => Some(GpuRenderJob {
                request: *request,
                image_handle,
                completions,
            }),
            BuiltGpuRequestKind::Invisible { .. } => None,
        }
    }
}

/// Completion record emitted by the render-world dispatcher.
#[derive(Clone, Copy, Debug)]
pub struct GpuGlyphCompletedRecord {
    pub key:          GlyphKey,
    pub bitmap_size:  UVec2,
    pub bearing:      Vec2,
    /// Atlas-specific bitmap inset in em units, used by quad-builders
    /// to position the padded quad without contaminating the
    /// font-defined bearing.
    pub pad_em:       Vec2,
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
    pub(super) request:      GpuGlyphRequest,
    /// Exact image the compute shader writes to.
    pub(super) image_handle: Handle<Image>,
    /// Per-atlas completion sink.
    pub(super) completions:  GpuCompletionSink,
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
