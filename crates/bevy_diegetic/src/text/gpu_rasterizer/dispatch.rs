//! Render-schedule dispatch system for GPU glyph rasterization.
//!
//! Drains extracted per-atlas jobs, builds edge and header storage
//! buffers, binds each job's storage texture, and encodes compute
//! passes.

use std::collections::HashMap;

use bevy::asset::Handle;
use bevy::ecs::system::Res;
use bevy::ecs::system::ResMut;
use bevy::image::Image;
use bevy::log::warn;
use bevy::math::Vec2;
use bevy::prelude::Resource;
use bevy::render::extract_resource::ExtractResource;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::BindGroup;
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
use bytemuck::cast_slice;

use super::pipeline::GlyphHeader;
use super::pipeline::GpuRasterizerPipeline;
use super::pipeline::RasterParams;
use super::pipeline::WORKGROUP_SIZE;
use super::request::GpuGlyphCompletedRecord;
use super::request::GpuRenderJob;
use super::request::GpuRenderJobExtract;
use super::request::GpuRenderJobQueue;

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

/// Shared context for dispatch work.
struct PageDispatchContext<'a> {
    render_device:    &'a RenderDevice,
    pipeline_cache:   &'a PipelineCache,
    pipeline:         &'a GpuRasterizerPipeline,
    compute_pipeline: &'a ComputePipeline,
    gpu_images:       &'a RenderAssets<GpuImage>,
}

/// Render-schedule system: drains the extracted jobs and dispatches
/// compute work.
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
    mut extract: ResMut<GpuRenderJobExtract>,
    mut queue: ResMut<GpuRenderJobQueue>,
    gpu_images: Res<RenderAssets<GpuImage>>,
) {
    queue.pending.append(&mut extract.pending);
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
    let dispatched: Vec<GpuRenderJob> = queue.pending.drain(..take).collect();
    if dispatched.is_empty() {
        return;
    }
    let by_image = partition_by_image(dispatched);

    let mut encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("gpu_rasterizer_encoder"),
    });

    let context = PageDispatchContext {
        render_device: &render_device,
        pipeline_cache: &pipeline_cache,
        pipeline: &pipeline,
        compute_pipeline,
        gpu_images: &gpu_images,
    };

    for (image_handle, jobs) in &by_image {
        encode_image(&context, &mut encoder, image_handle, jobs, &mut queue);
    }

    render_queue.submit(std::iter::once(encoder.finish()));
}

/// Groups dispatched jobs by target image. One compute pass per image
/// binds exactly one storage texture as the write target.
fn partition_by_image(
    dispatched: Vec<GpuRenderJob>,
) -> HashMap<Handle<Image>, Vec<GpuRenderJob>> {
    let mut by_image: HashMap<Handle<Image>, Vec<GpuRenderJob>> = HashMap::new();
    for job in dispatched {
        by_image
            .entry(job.image_handle.clone())
            .or_default()
            .push(job);
    }
    by_image
}

/// Encodes the compute pass for a single atlas image.
///
/// One `dispatch_workgroups` call per glyph, sized exactly to that
/// glyph's bitmap. The kernel sees `glyph_count == 1` and reads
/// `headers[0]`, so no extra workgroup can address a neighbor glyph's
/// atlas region. All glyphs on an image share a single compute pass;
/// the bind group rebinds between dispatches.
fn encode_image(
    context: &PageDispatchContext<'_>,
    encoder: &mut CommandEncoder,
    image_handle: &Handle<Image>,
    jobs: &[GpuRenderJob],
    queue: &mut GpuRenderJobQueue,
) {
    let Some(gpu_image) = context.gpu_images.get(image_handle) else {
        queue.pending.extend(jobs.iter().cloned());
        return;
    };

    let layout = context
        .pipeline_cache
        .get_bind_group_layout(&context.pipeline.layout);

    let mut per_glyph: Vec<PerGlyphDispatch> = Vec::with_capacity(jobs.len());
    for job in jobs {
        let req = &job.request;
        if req.body.edges.is_empty()
            || req.body.bitmap_size.x == 0
            || req.body.bitmap_size.y == 0
        {
            continue;
        }
        let edges_buf =
            create_storage_buffer(context.render_device, "gpu_raster_edges", &req.body.edges);
        let header = GlyphHeader {
            edge_offset:  0,
            edge_count:   req.body.edges.len().to_u32(),
            atlas_origin: [req.atlas_origin.x, req.atlas_origin.y],
            bitmap_size:  [req.body.bitmap_size.x, req.body.bitmap_size.y],
            _padding:     [0, 0],
        };
        let headers_buf =
            create_storage_buffer(context.render_device, "gpu_raster_headers", &[header]);
        let params = RasterParams {
            sdf_range:      req.sdf_range,
            padding_texels: 0,
            distance_field: u32::from(req.distance_field),
            glyph_count:    1,
        };
        let params_buf = context
            .render_device
            .create_buffer_with_data(&BufferInitDescriptor {
                label:    Some("gpu_raster_params"),
                contents: bytemuck::bytes_of(&params),
                usage:    BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            });
        let bind_group = context.render_device.create_bind_group(
            Some("gpu_rasterizer_bind_group"),
            &layout,
            &BindGroupEntries::sequential((
                edges_buf.as_entire_binding(),
                headers_buf.as_entire_binding(),
                &gpu_image.texture_view,
                params_buf.as_entire_binding(),
            )),
        );
        let gx = req.body.bitmap_size.x.div_ceil(WORKGROUP_SIZE);
        let gy = req.body.bitmap_size.y.div_ceil(WORKGROUP_SIZE);
        per_glyph.push(PerGlyphDispatch {
            bind_group,
            groups_x: gx,
            groups_y: gy,
            _edges_buf: edges_buf,
            _headers_buf: headers_buf,
            _params_buf: params_buf,
        });
    }

    if !per_glyph.is_empty() {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label:            Some("gpu_rasterizer_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(context.compute_pipeline);
        for dispatch in &per_glyph {
            pass.set_bind_group(0, &dispatch.bind_group, &[]);
            pass.dispatch_workgroups(dispatch.groups_x, dispatch.groups_y, 1);
        }
    }

    for job in jobs {
        let req = &job.request;
        job.completions.push(GpuGlyphCompletedRecord {
            key:          req.key,
            bitmap_size:  req.body.bitmap_size,
            bearing:      Vec2::new(req.body.bearing_x, req.body.bearing_y),
            atlas_origin: req.atlas_origin,
            page_index:   req.page_index,
        });
    }
}

/// Per-glyph dispatch state — bind group plus the buffers it
/// references. The buffers are held here so they outlive the compute
/// pass; wgpu requires the bind group's bound resources to remain
/// alive until the command buffer is submitted.
struct PerGlyphDispatch {
    bind_group:   BindGroup,
    groups_x:     u32,
    groups_y:     u32,
    _edges_buf:   Buffer,
    _headers_buf: Buffer,
    _params_buf:  Buffer,
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
