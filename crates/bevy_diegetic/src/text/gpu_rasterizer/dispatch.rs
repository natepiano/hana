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
use bevy::math::UVec2;
use bevy::math::Vec2;
use bevy::prelude::Resource;
use bevy::render::extract_resource::ExtractResource;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::BindGroupEntries;
use bevy::render::render_resource::Buffer;
use bevy::render::render_resource::BufferInitDescriptor;
use bevy::render::render_resource::BufferUsages;
use bevy::render::render_resource::CachedPipelineState;
use bevy::render::render_resource::CommandEncoder;
use bevy::render::render_resource::CommandEncoderDescriptor;
use bevy::render::render_resource::ComputePassDescriptor;
use bevy::render::render_resource::ComputePipeline;
use bevy::render::render_resource::PipelineCache;
use bevy::render::renderer::RenderDevice;
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;
use bytemuck::cast_slice;

use super::pipeline::GlyphHeader;
use super::pipeline::GpuRasterizerPipeline;
use super::pipeline::RasterParams;
use super::pipeline::WORKGROUP_SIZE;
use super::request::GpuGlyphRequest;
use super::request::GpuGlyphRequestQueue;
use crate::text::atlas::GlyphKey;

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
    pub key:          GlyphKey,
    pub bitmap_size:  UVec2,
    pub bearing:      Vec2,
    pub atlas_origin: UVec2,
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
    inner: Arc<Mutex<Vec<GpuGlyphCompletedRecord>>>,
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
    fn push(&self, record: GpuGlyphCompletedRecord) {
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        guard.push(record);
    }

    /// Drains every queued completion record.
    pub(super) fn drain(&self) -> Vec<GpuGlyphCompletedRecord> {
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut *guard)
    }
}

/// Shared context for per-page dispatch work.
struct PageDispatchContext<'a> {
    render_device:    &'a RenderDevice,
    pipeline_cache:   &'a PipelineCache,
    pipeline:         &'a GpuRasterizerPipeline,
    compute_pipeline: &'a ComputePipeline,
    atlas_pages:      &'a RenderAtlasPages,
    gpu_images:       &'a RenderAssets<GpuImage>,
    completions:      &'a GpuGlyphCompletionBuffer,
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
        if let CachedPipelineState::Err(err) =
            pipeline_cache.get_compute_pipeline_state(pipeline.sdf_pipeline)
        {
            warn!("gpu_rasterizer: SDF pipeline failed to build: {err:?}");
        }
        return;
    };

    let take = (budget.per_frame as usize).min(queue.pending.len());
    let dispatched: Vec<GpuGlyphRequest> = queue.pending.drain(..take).collect();
    let by_page = partition_by_page(dispatched);

    let mut encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("gpu_rasterizer_encoder"),
    });

    let context = PageDispatchContext {
        render_device: &render_device,
        pipeline_cache: &pipeline_cache,
        pipeline: &pipeline,
        compute_pipeline,
        atlas_pages: &atlas_pages,
        gpu_images: &gpu_images,
        completions: &completions,
    };

    for (page_index, requests) in &by_page {
        encode_page(&context, &mut encoder, *page_index, requests, &mut queue);
    }

    render_queue.submit(std::iter::once(encoder.finish()));
}

/// Groups dispatched requests by atlas page index. One compute pass
/// per page binds exactly one storage texture as the write target.
fn partition_by_page(dispatched: Vec<GpuGlyphRequest>) -> HashMap<u32, Vec<GpuGlyphRequest>> {
    let mut by_page: HashMap<u32, Vec<GpuGlyphRequest>> = HashMap::new();
    for req in dispatched {
        by_page.entry(req.page_index).or_default().push(req);
    }
    by_page
}

/// Encodes the compute pass for a single atlas page. Re-queues
/// requests if the page's image is not yet uploaded.
fn encode_page(
    context: &PageDispatchContext<'_>,
    encoder: &mut CommandEncoder,
    page_index: u32,
    requests: &[GpuGlyphRequest],
    queue: &mut GpuGlyphRequestQueue,
) {
    let Some(handle) = context.atlas_pages.pages.get(page_index.to_usize()) else {
        warn!(
            "gpu_rasterizer: page {page_index} not in RenderAtlasPages; skipping {} glyph(s)",
            requests.len()
        );
        return;
    };
    let Some(gpu_image) = context.gpu_images.get(handle) else {
        queue.pending.extend(requests.iter().cloned());
        return;
    };

    let Some(buffers) = build_page_buffers(context.render_device, requests) else {
        return;
    };

    let layout = context
        .pipeline_cache
        .get_bind_group_layout(&context.pipeline.layout);
    let bind_group = context.render_device.create_bind_group(
        Some("gpu_rasterizer_bind_group"),
        &layout,
        &BindGroupEntries::sequential((
            buffers.edges_buf.as_entire_binding(),
            buffers.headers_buf.as_entire_binding(),
            &gpu_image.texture_view,
            buffers.params_buf.as_entire_binding(),
        )),
    );

    let (max_groups_x, max_groups_y) = max_workgroup_grid(requests);
    if max_groups_x == 0 || max_groups_y == 0 {
        return;
    }

    {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label:            Some("gpu_rasterizer_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(context.compute_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(max_groups_x, max_groups_y, buffers.header_count);
    }

    for req in requests {
        context.completions.push(GpuGlyphCompletedRecord {
            key:          req.key,
            bitmap_size:  req.body.bitmap_size,
            bearing:      Vec2::new(req.body.bearing_x, req.body.bearing_y),
            atlas_origin: req.atlas_origin,
            page_index:   req.page_index,
        });
    }
}

/// Storage buffers prepared for one compute pass.
struct PageBuffers {
    edges_buf:    Buffer,
    headers_buf:  Buffer,
    params_buf:   Buffer,
    header_count: u32,
}

/// Concatenates edge data and builds header / params buffers for a
/// single page's requests. Returns `None` if there is nothing to
/// dispatch (empty edges or no headers).
fn build_page_buffers(
    render_device: &RenderDevice,
    requests: &[GpuGlyphRequest],
) -> Option<PageBuffers> {
    let mut edges_combined = Vec::new();
    let mut headers = Vec::<GlyphHeader>::with_capacity(requests.len());
    for req in requests {
        let edge_offset = edges_combined.len().to_u32();
        edges_combined.extend_from_slice(&req.body.edges);
        headers.push(GlyphHeader {
            edge_offset,
            edge_count: req.body.edges.len().to_u32(),
            atlas_origin: [req.atlas_origin.x, req.atlas_origin.y],
            bitmap_size: [req.body.bitmap_size.x, req.body.bitmap_size.y],
            _padding: [0, 0],
        });
    }
    if edges_combined.is_empty() || headers.is_empty() {
        return None;
    }

    let edges_buf = create_storage_buffer(render_device, "gpu_raster_edges", &edges_combined);
    let headers_buf = create_storage_buffer(render_device, "gpu_raster_headers", &headers);

    // All requests on a page share the atlas-config-derived
    // sdf_range / distance_field, so take params from the first.
    let first = &requests[0];
    let header_count = headers.len().to_u32();
    let params = RasterParams {
        sdf_range:      first.sdf_range,
        padding_texels: 0,
        distance_field: u32::from(first.distance_field),
        glyph_count:    header_count,
    };
    let params_buf = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label:    Some("gpu_raster_params"),
        contents: bytemuck::bytes_of(&params),
        usage:    BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });

    Some(PageBuffers {
        edges_buf,
        headers_buf,
        params_buf,
        header_count,
    })
}

/// Computes the max workgroup grid across all glyphs in a pass.
/// Over-dispatch is bounded by the per-glyph bounds check in the
/// shader.
fn max_workgroup_grid(requests: &[GpuGlyphRequest]) -> (u32, u32) {
    let mut max_groups_x = 0_u32;
    let mut max_groups_y = 0_u32;
    for req in requests {
        let gx = req.body.bitmap_size.x.div_ceil(WORKGROUP_SIZE);
        let gy = req.body.bitmap_size.y.div_ceil(WORKGROUP_SIZE);
        max_groups_x = max_groups_x.max(gx);
        max_groups_y = max_groups_y.max(gy);
    }
    (max_groups_x, max_groups_y)
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
