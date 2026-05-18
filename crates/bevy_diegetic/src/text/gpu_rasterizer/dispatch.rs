//! Render-schedule dispatch system for GPU glyph rasterization.
//!
//! Drains the extracted [`super::request::GpuGlyphRequestQueue`],
//! groups requests by target atlas page, builds the per-page edge +
//! header storage buffers, binds the page's storage texture, and
//! encodes one compute pass per page.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use bevy::asset::Handle;
use bevy::ecs::system::Res;
use bevy::ecs::system::ResMut;
use bevy::image::Image;
use bevy::log::warn;
use bevy::prelude::Resource;
use bevy::render::extract_resource::ExtractResource;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::BindGroupEntries;
use bevy::render::render_resource::Buffer;
use bevy::render::render_resource::BufferInitDescriptor;
use bevy::render::render_resource::BufferUsages;
use bevy::render::render_resource::CachedPipelineState;
use bevy::render::render_resource::CommandEncoderDescriptor;
use bevy::render::render_resource::ComputePassDescriptor;
use bevy::render::render_resource::PipelineCache;
use bevy::render::renderer::RenderDevice;
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bytemuck::cast_slice;

use super::pipeline::GlyphHeader;
use super::pipeline::GpuRasterizerPipeline;
use super::pipeline::RasterParams;
use super::pipeline::WORKGROUP_SIZE;
use super::request::GpuGlyphRequest;
use super::request::GpuGlyphRequestQueue;

/// Default per-frame dispatch cap. The user can raise this via
/// `Res<GpuGlyphBudget>` for batch warm-up or loading screens.
pub(super) const DEFAULT_BUDGET_PER_FRAME: u32 = 16;
/// Threshold at which the dispatcher logs a queue-overflow warning.
const QUEUE_HIGH_WATER: usize = 4096;

/// User-facing cap on per-frame glyph dispatches.
#[derive(Resource, Clone, Copy, Debug, ExtractResource)]
pub struct GpuGlyphBudget {
    /// Maximum number of glyph dispatches per frame across all pages.
    pub per_frame: u32,
}

impl Default for GpuGlyphBudget {
    fn default() -> Self {
        Self {
            per_frame: DEFAULT_BUDGET_PER_FRAME,
        }
    }
}

/// Render-world resource — atlas page image handles, extracted from
/// the main-world atlas each frame.
#[derive(Resource, Default, Clone, ExtractResource)]
pub struct RenderAtlasPages {
    /// `page_index → Handle<Image>` (storage texture for that page).
    pub pages: Vec<Handle<Image>>,
}

/// Completion record emitted by the dispatcher; extracted back into
/// the main world for the [`super::request::GpuGlyphCompleted`] event.
#[derive(Clone, Copy, Debug)]
pub struct GpuGlyphCompletedRecord {
    pub key:          super::super::atlas::GlyphKey,
    pub bitmap_size:  bevy::math::UVec2,
    pub bearing:      bevy::math::Vec2,
    pub atlas_origin: bevy::math::UVec2,
    pub page_index:   u32,
}

/// Shared completion buffer used to bridge render→main.
///
/// `ExtractResourcePlugin` only copies main→render, so a normal
/// `Resource` write on the render world is invisible from the main
/// world. The `Arc<Mutex<...>>` is owned by both worlds (the
/// extract step clones the Arc, both clones point at the same inner
/// `Vec`), so the render-world dispatcher can push records and the
/// main-world drain system can read them in the same frame.
#[derive(Resource, Clone, ExtractResource)]
pub struct GpuGlyphCompletionBuffer {
    pub(crate) inner: Arc<Mutex<Vec<GpuGlyphCompletedRecord>>>,
}

impl Default for GpuGlyphCompletionBuffer {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl GpuGlyphCompletionBuffer {
    /// Appends a single completion record to the shared buffer.
    pub(crate) fn push(&self, record: GpuGlyphCompletedRecord) {
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        guard.push(record);
    }

    /// Drains every queued completion record.
    pub(crate) fn drain(&self) -> Vec<GpuGlyphCompletedRecord> {
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut *guard)
    }
}

/// Render-schedule system: drains the extracted request queue and
/// dispatches one compute pass per target page.
#[allow(
    clippy::too_many_arguments,
    reason = "single render-schedule system that fans out into all wgpu resources it needs"
)]
pub(super) fn dispatch_glyph_compute(
    pipeline: Res<GpuRasterizerPipeline>,
    pipeline_cache: Res<PipelineCache>,
    budget: Res<GpuGlyphBudget>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut queue: ResMut<GpuGlyphRequestQueue>,
    atlas_pages: Res<RenderAtlasPages>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    completions: Res<GpuGlyphCompletionBuffer>,
) {
    if queue.pending.is_empty() {
        return;
    }
    if queue.pending.len() > QUEUE_HIGH_WATER {
        warn!(
            "gpu_rasterizer: request queue at {} entries (>{} high-water); raise \
             GpuGlyphBudget.per_frame or pre-warm at load time",
            queue.pending.len(),
            QUEUE_HIGH_WATER
        );
    }

    let Some(compute_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.sdf_pipeline) else {
        // Pipeline still building; defer until next frame. Requests
        // stay in the queue so nothing is lost.
        if matches!(
            pipeline_cache.get_compute_pipeline_state(pipeline.sdf_pipeline),
            CachedPipelineState::Err(_)
        ) {
            warn!("gpu_rasterizer: SDF pipeline failed to build; will retry next frame");
        }
        return;
    };

    let take = (budget.per_frame as usize).min(queue.pending.len());
    let dispatched: Vec<GpuGlyphRequest> = queue.pending.drain(..take).collect();

    // Partition by page; one compute pass per page so each pass binds
    // exactly one storage texture as the write target.
    let mut by_page: HashMap<u32, Vec<GpuGlyphRequest>> = HashMap::new();
    for req in dispatched {
        by_page.entry(req.page_index).or_default().push(req);
    }

    let mut encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("gpu_rasterizer_encoder"),
    });

    for (page_index, requests) in &by_page {
        let Some(handle) = atlas_pages.pages.get(*page_index as usize) else {
            warn!(
                "gpu_rasterizer: page {page_index} not in RenderAtlasPages; skipping {} glyph(s)",
                requests.len()
            );
            continue;
        };
        let Some(gpu_image) = gpu_images.get(handle) else {
            // Image not yet uploaded — re-queue for next frame.
            queue.pending.extend(requests.iter().cloned());
            continue;
        };

        // Build concatenated edge buffer and header buffer.
        let mut edges_combined = Vec::new();
        let mut headers = Vec::<GlyphHeader>::with_capacity(requests.len());
        for req in requests {
            let edge_offset = edges_combined.len() as u32;
            edges_combined.extend_from_slice(&req.body.edges);
            headers.push(GlyphHeader {
                edge_offset,
                edge_count: req.body.edges.len() as u32,
                atlas_origin: [req.atlas_origin.x, req.atlas_origin.y],
                bitmap_size: [req.body.bitmap_size.x, req.body.bitmap_size.y],
                _padding: [0, 0],
            });
        }
        if edges_combined.is_empty() || headers.is_empty() {
            continue;
        }

        let edges_buf = create_storage_buffer(&render_device, "gpu_raster_edges", &edges_combined);
        let headers_buf = create_storage_buffer(&render_device, "gpu_raster_headers", &headers);

        // Take params from the first request — the dispatch pass
        // serves one page at a time, and all requests on a page share
        // the atlas-config-derived sdf_range / distance_field.
        let first = &requests[0];
        let params = RasterParams {
            sdf_range:      first.sdf_range,
            padding_texels: 0,
            distance_field: u32::from(first.distance_field),
            glyph_count:    headers.len() as u32,
        };
        let params_buf = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label:    Some("gpu_raster_params"),
            contents: bytemuck::bytes_of(&params),
            usage:    BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let layout = pipeline_cache.get_bind_group_layout(&pipeline.layout);
        let bind_group = render_device.create_bind_group(
            Some("gpu_rasterizer_bind_group"),
            &layout,
            &BindGroupEntries::sequential((
                edges_buf.as_entire_binding(),
                headers_buf.as_entire_binding(),
                &gpu_image.texture_view,
                params_buf.as_entire_binding(),
            )),
        );

        // Compute the max workgroup grid across all glyphs in this
        // pass; over-dispatch is bounded by the per-glyph bounds check
        // inside the shader.
        let mut max_groups_x = 0_u32;
        let mut max_groups_y = 0_u32;
        for req in requests {
            let gx = req.body.bitmap_size.x.div_ceil(WORKGROUP_SIZE);
            let gy = req.body.bitmap_size.y.div_ceil(WORKGROUP_SIZE);
            max_groups_x = max_groups_x.max(gx);
            max_groups_y = max_groups_y.max(gy);
        }
        if max_groups_x == 0 || max_groups_y == 0 {
            continue;
        }

        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label:            Some("gpu_rasterizer_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(compute_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(max_groups_x, max_groups_y, headers.len() as u32);
        }

        for req in requests {
            completions.push(GpuGlyphCompletedRecord {
                key:          req.key,
                bitmap_size:  req.body.bitmap_size,
                bearing:      bevy::math::Vec2::new(req.body.bearing_x, req.body.bearing_y),
                atlas_origin: req.atlas_origin,
                page_index:   req.page_index,
            });
        }
    }

    render_queue.submit(std::iter::once(encoder.finish()));
}

fn create_storage_buffer<T: bytemuck::Pod>(
    device: &RenderDevice,
    label: &'static str,
    data: &[T],
) -> Buffer {
    device.create_buffer_with_data(&BufferInitDescriptor {
        label:    Some(label),
        contents: cast_slice(data),
        usage:    BufferUsages::STORAGE | BufferUsages::COPY_DST,
    })
}
