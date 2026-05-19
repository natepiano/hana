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
use bevy::render::render_resource::BindGroupLayout;
use bevy::render::render_resource::Buffer;
use bevy::render::render_resource::BufferInitDescriptor;
use bevy::render::render_resource::BufferUsages;
use bevy::render::render_resource::CachedPipelineState;
use bevy::render::render_resource::CommandEncoder;
use bevy::render::render_resource::CommandEncoderDescriptor;
use bevy::render::render_resource::ComputePassDescriptor;
use bevy::render::render_resource::ComputePipeline;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::PipelineCache;
use bevy::render::render_resource::Texture;
use bevy::render::render_resource::TextureDescriptor;
use bevy::render::render_resource::TextureDimension;
use bevy::render::render_resource::TextureFormat;
use bevy::render::render_resource::TextureUsages;
use bevy::render::render_resource::TextureView;
use bevy::render::render_resource::TextureViewDescriptor;
use bevy::render::renderer::RenderDevice;
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy_kana::ToU32;
use bytemuck::cast_slice;

use super::edges::CornerPoint;
use super::pipeline::GlyphHeader;
use super::pipeline::GpuRasterizerPipeline;
use super::pipeline::RasterParams;
use super::pipeline::WORKGROUP_SIZE;
use super::request::GpuGlyphCompletedRecord;
use super::request::GpuGlyphRequest;
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
    render_device:                 &'a RenderDevice,
    pipeline_cache:                &'a PipelineCache,
    pipeline:                      &'a GpuRasterizerPipeline,
    sdf_compute_pipeline:          &'a ComputePipeline,
    msdf_compute_pipeline:         Option<&'a ComputePipeline>,
    msdf_correct_compute_pipeline: Option<&'a ComputePipeline>,
    gpu_images:                    &'a RenderAssets<GpuImage>,
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

    let Some(sdf_compute_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.sdf_pipeline)
    else {
        if let CachedPipelineState::Err(err) =
            pipeline_cache.get_compute_pipeline_state(pipeline.sdf_pipeline)
        {
            warn!("gpu_rasterizer: SDF pipeline failed to build: {err:?}");
        }
        return;
    };
    let msdf_compute_pipeline = pipeline_cache.get_compute_pipeline(pipeline.msdf_pipeline);
    if msdf_compute_pipeline.is_none()
        && let CachedPipelineState::Err(err) =
            pipeline_cache.get_compute_pipeline_state(pipeline.msdf_pipeline)
    {
        warn!("gpu_rasterizer: MSDF pipeline failed to build: {err:?}");
    }
    let msdf_correct_compute_pipeline =
        pipeline_cache.get_compute_pipeline(pipeline.msdf_correct_pipeline);
    if msdf_correct_compute_pipeline.is_none()
        && let CachedPipelineState::Err(err) =
            pipeline_cache.get_compute_pipeline_state(pipeline.msdf_correct_pipeline)
    {
        warn!("gpu_rasterizer: MSDF correction pipeline failed to build: {err:?}");
    }

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
        sdf_compute_pipeline,
        msdf_compute_pipeline,
        msdf_correct_compute_pipeline,
        gpu_images: &gpu_images,
    };

    for (image_handle, jobs) in &by_image {
        encode_image(&context, &mut encoder, image_handle, jobs, &mut queue);
    }

    render_queue.submit(std::iter::once(encoder.finish()));
}

/// Groups dispatched jobs by target image. One compute pass per image
/// binds exactly one storage texture as the write target.
fn partition_by_image(dispatched: Vec<GpuRenderJob>) -> HashMap<Handle<Image>, Vec<GpuRenderJob>> {
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
/// One `dispatch_workgroups` call per glyph for the SDF and MSDF
/// generation kernels, sized exactly to that glyph's bitmap. The
/// kernel sees `glyph_count == 1` and reads `headers[0]`, so no extra
/// workgroup can address a neighbor glyph's atlas region.
///
/// MSDF requests use a two-pass ping-pong: the MSDF generation kernel
/// writes into a per-page scratch texture; the MSDF correction kernel
/// then reads the scratch and writes the final corrected texels to the
/// atlas page. Both kernels run in the same `ComputePass`, so wgpu
/// inserts the read-after-write barriers automatically when bind
/// groups are swapped.
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
    let correction_layout = context
        .pipeline_cache
        .get_bind_group_layout(&context.pipeline.correction_layout);
    let msdf_pipelines_ready = msdf_pipelines_ready(context);
    let scratch = create_msdf_scratch(context, gpu_image, jobs, msdf_pipelines_ready);
    let dispatches = collect_glyph_dispatches(
        context,
        &layout,
        &correction_layout,
        gpu_image,
        scratch.as_ref(),
        jobs,
        msdf_pipelines_ready,
    );

    encode_glyph_dispatches(context, encoder, &dispatches);
    record_completed_jobs(queue, jobs, dispatches.msdf_skipped, msdf_pipelines_ready);
}

const fn msdf_pipelines_ready(context: &PageDispatchContext<'_>) -> bool {
    context.msdf_compute_pipeline.is_some() && context.msdf_correct_compute_pipeline.is_some()
}

fn create_msdf_scratch(
    context: &PageDispatchContext<'_>,
    gpu_image: &GpuImage,
    jobs: &[GpuRenderJob],
    msdf_pipelines_ready: bool,
) -> Option<ScratchTexture> {
    if msdf_pipelines_ready
        && jobs
            .iter()
            .any(|j| matches!(j.request, GpuGlyphRequest::Msdf(_)))
    {
        Some(create_scratch_texture(context.render_device, gpu_image))
    } else {
        None
    }
}

fn collect_glyph_dispatches(
    context: &PageDispatchContext<'_>,
    layout: &BindGroupLayout,
    correction_layout: &BindGroupLayout,
    gpu_image: &GpuImage,
    scratch: Option<&ScratchTexture>,
    jobs: &[GpuRenderJob],
    msdf_pipelines_ready: bool,
) -> GlyphDispatches {
    let mut dispatches = GlyphDispatches::default();
    for job in jobs {
        let request = &job.request;
        if has_empty_dispatch(request) {
            continue;
        }
        match request {
            GpuGlyphRequest::Sdf(_) => {
                let dispatch = build_per_glyph_dispatch(context, layout, gpu_image, request);
                dispatches.sdf_per_glyph.push(dispatch);
            },
            GpuGlyphRequest::Msdf(_) => {
                let Some(scratch_view) = scratch.map(|s| &s.view) else {
                    dispatches.msdf_skipped.push(job.clone());
                    continue;
                };
                if !msdf_pipelines_ready {
                    dispatches.msdf_skipped.push(job.clone());
                    continue;
                }
                let dispatch = build_msdf_glyph_dispatch(
                    context,
                    layout,
                    correction_layout,
                    gpu_image,
                    scratch_view,
                    request,
                );
                dispatches.msdf_per_glyph.push(dispatch);
            },
        }
    }
    dispatches
}

const fn has_empty_dispatch(request: &GpuGlyphRequest) -> bool {
    let common = request.common();
    common.body.edges.is_empty() || common.body.bitmap_size.x == 0 || common.body.bitmap_size.y == 0
}

fn encode_glyph_dispatches(
    context: &PageDispatchContext<'_>,
    encoder: &mut CommandEncoder,
    dispatches: &GlyphDispatches,
) {
    if dispatches.is_empty() {
        return;
    }
    let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
        label:            Some("gpu_rasterizer_pass"),
        timestamp_writes: None,
    });
    if !dispatches.sdf_per_glyph.is_empty() {
        pass.set_pipeline(context.sdf_compute_pipeline);
        for dispatch in &dispatches.sdf_per_glyph {
            pass.set_bind_group(0, &dispatch.bind_group, &[]);
            pass.dispatch_workgroups(dispatch.groups_x, dispatch.groups_y, 1);
        }
    }
    if let (Some(msdf_pipeline), Some(correct_pipeline)) = (
        context.msdf_compute_pipeline,
        context.msdf_correct_compute_pipeline,
    ) {
        for dispatch in &dispatches.msdf_per_glyph {
            pass.set_pipeline(msdf_pipeline);
            pass.set_bind_group(0, &dispatch.gen_bind_group, &[]);
            pass.dispatch_workgroups(dispatch.groups_x, dispatch.groups_y, 1);
            pass.set_pipeline(correct_pipeline);
            pass.set_bind_group(0, &dispatch.correct_bind_group, &[]);
            pass.dispatch_workgroups(dispatch.groups_x, dispatch.groups_y, 1);
        }
    }
}

fn record_completed_jobs(
    queue: &mut GpuRenderJobQueue,
    jobs: &[GpuRenderJob],
    msdf_skipped: Vec<GpuRenderJob>,
    msdf_pipelines_ready: bool,
) {
    queue.pending.extend(msdf_skipped);
    for job in jobs {
        let request = &job.request;
        if matches!(request, GpuGlyphRequest::Msdf(_)) && !msdf_pipelines_ready {
            continue;
        }
        let common = request.common();
        job.completions.push(GpuGlyphCompletedRecord {
            key:          common.key,
            bitmap_size:  common.body.bitmap_size,
            bearing:      Vec2::new(common.body.bearing_x, common.body.bearing_y),
            pad_em:       Vec2::new(common.body.pad_x_em, common.body.pad_y_em),
            atlas_origin: common.atlas_origin,
            page_index:   common.page_index,
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

#[derive(Default)]
struct GlyphDispatches {
    sdf_per_glyph:  Vec<PerGlyphDispatch>,
    msdf_per_glyph: Vec<MsdfPerGlyphDispatch>,
    msdf_skipped:   Vec<GpuRenderJob>,
}

impl GlyphDispatches {
    const fn is_empty(&self) -> bool {
        self.sdf_per_glyph.is_empty() && self.msdf_per_glyph.is_empty()
    }
}

/// MSDF per-glyph dispatch state — two bind groups (generation pass
/// writes to scratch, correction pass reads scratch + writes page) and
/// all GPU resources they reference.
struct MsdfPerGlyphDispatch {
    gen_bind_group:     BindGroup,
    correct_bind_group: BindGroup,
    groups_x:           u32,
    groups_y:           u32,
    _edges_buf:         Buffer,
    _headers_buf:       Buffer,
    _params_buf:        Buffer,
    _corners_buf:       Buffer,
}

/// Per-page scratch texture used as the intermediate for MSDF
/// generation before the correction pass reads it back. Created once
/// per `encode_image` call; dropped at the end of the function so the
/// memory is released as soon as the command buffer references it via
/// the bind group resources held by the encoder.
struct ScratchTexture {
    _texture: Texture,
    view:     TextureView,
}

fn create_scratch_texture(device: &RenderDevice, gpu_image: &GpuImage) -> ScratchTexture {
    let extent = Extent3d {
        width:                 gpu_image.size.width,
        height:                gpu_image.size.height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&TextureDescriptor {
        label:           Some("gpu_rasterizer_msdf_scratch"),
        size:            extent,
        mip_level_count: 1,
        sample_count:    1,
        dimension:       TextureDimension::D2,
        format:          TextureFormat::Rgba8Unorm,
        usage:           TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
        view_formats:    &[],
    });
    let view = texture.create_view(&TextureViewDescriptor {
        label: Some("gpu_rasterizer_msdf_scratch_view"),
        ..Default::default()
    });
    ScratchTexture {
        _texture: texture,
        view,
    }
}

fn build_per_glyph_dispatch(
    context: &PageDispatchContext<'_>,
    layout: &BindGroupLayout,
    gpu_image: &GpuImage,
    request: &GpuGlyphRequest,
) -> PerGlyphDispatch {
    let common = request.common();
    let edges_buf = create_storage_buffer(
        context.render_device,
        "gpu_raster_edges",
        &common.body.edges,
    );
    let header = GlyphHeader {
        edge_offset:   0,
        edge_count:    common.body.edges.len().to_u32(),
        atlas_origin:  [common.atlas_origin.x, common.atlas_origin.y],
        bitmap_size:   [common.body.bitmap_size.x, common.body.bitmap_size.y],
        corner_offset: 0,
        corner_count:  common.body.corners.len().to_u32(),
    };
    let headers_buf = create_storage_buffer(context.render_device, "gpu_raster_headers", &[header]);
    let params = RasterParams {
        sdf_range:      common.sdf_range,
        padding_texels: 0,
        distance_field: u32::from(request.distance_field()),
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
        layout,
        &BindGroupEntries::sequential((
            edges_buf.as_entire_binding(),
            headers_buf.as_entire_binding(),
            &gpu_image.texture_view,
            params_buf.as_entire_binding(),
        )),
    );
    let groups_x = common.body.bitmap_size.x.div_ceil(WORKGROUP_SIZE);
    let groups_y = common.body.bitmap_size.y.div_ceil(WORKGROUP_SIZE);
    PerGlyphDispatch {
        bind_group,
        groups_x,
        groups_y,
        _edges_buf: edges_buf,
        _headers_buf: headers_buf,
        _params_buf: params_buf,
    }
}

fn build_msdf_glyph_dispatch(
    context: &PageDispatchContext<'_>,
    layout: &BindGroupLayout,
    correction_layout: &BindGroupLayout,
    gpu_image: &GpuImage,
    scratch_view: &TextureView,
    request: &GpuGlyphRequest,
) -> MsdfPerGlyphDispatch {
    let common = request.common();
    let edges_buf = create_storage_buffer(
        context.render_device,
        "gpu_raster_edges",
        &common.body.edges,
    );
    // Corner buffer must be non-empty so wgpu can bind it as a storage
    // buffer; for glyphs with zero corners we ship a single zeroed
    // sentinel and the kernel ignores it via `corner_count == 0`.
    let corners_data: Vec<CornerPoint> = if common.body.corners.is_empty() {
        vec![CornerPoint::default()]
    } else {
        common.body.corners.clone()
    };
    let corners_buf =
        create_storage_buffer(context.render_device, "gpu_raster_corners", &corners_data);
    let header = GlyphHeader {
        edge_offset:   0,
        edge_count:    common.body.edges.len().to_u32(),
        atlas_origin:  [common.atlas_origin.x, common.atlas_origin.y],
        bitmap_size:   [common.body.bitmap_size.x, common.body.bitmap_size.y],
        corner_offset: 0,
        corner_count:  common.body.corners.len().to_u32(),
    };
    let headers_buf = create_storage_buffer(context.render_device, "gpu_raster_headers", &[header]);
    let params = RasterParams {
        sdf_range:      common.sdf_range,
        padding_texels: 0,
        distance_field: u32::from(request.distance_field()),
        glyph_count:    1,
    };
    let params_buf = context
        .render_device
        .create_buffer_with_data(&BufferInitDescriptor {
            label:    Some("gpu_raster_params"),
            contents: bytemuck::bytes_of(&params),
            usage:    BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
    let gen_bind_group = context.render_device.create_bind_group(
        Some("gpu_rasterizer_msdf_gen_bind_group"),
        layout,
        &BindGroupEntries::sequential((
            edges_buf.as_entire_binding(),
            headers_buf.as_entire_binding(),
            scratch_view,
            params_buf.as_entire_binding(),
        )),
    );
    let correct_bind_group = context.render_device.create_bind_group(
        Some("gpu_rasterizer_msdf_correct_bind_group"),
        correction_layout,
        &BindGroupEntries::sequential((
            edges_buf.as_entire_binding(),
            headers_buf.as_entire_binding(),
            scratch_view,
            &gpu_image.texture_view,
            params_buf.as_entire_binding(),
            corners_buf.as_entire_binding(),
        )),
    );
    let groups_x = common.body.bitmap_size.x.div_ceil(WORKGROUP_SIZE);
    let groups_y = common.body.bitmap_size.y.div_ceil(WORKGROUP_SIZE);
    MsdfPerGlyphDispatch {
        gen_bind_group,
        correct_bind_group,
        groups_x,
        groups_y,
        _edges_buf: edges_buf,
        _headers_buf: headers_buf,
        _params_buf: params_buf,
        _corners_buf: corners_buf,
    }
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
