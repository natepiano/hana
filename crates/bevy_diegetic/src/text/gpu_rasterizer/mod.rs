//! GPU compute-shader glyph rasterizer.
//!
//! Sibling to the CPU `fdsm` path in [`super::msdf_rasterizer`]. Opt
//! in per atlas by setting [`super::atlas_config::RasterBackend::Gpu`]
//! on the [`super::atlas_config::AtlasConfig`].

use std::sync::Arc;

use bevy::app::App;
use bevy::app::Plugin;
use bevy::app::PostStartup;
use bevy::app::PostUpdate;
use bevy::asset::AssetServer;
use bevy::asset::Handle;
use bevy::asset::embedded_asset;
use bevy::asset::load_internal_asset;
use bevy::asset::uuid_handle;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::Commands;
use bevy::ecs::system::Res;
use bevy::ecs::system::ResMut;
use bevy::log::warn;
use bevy::render::Render;
use bevy::render::RenderApp;
use bevy::render::RenderStartup;
use bevy::render::RenderSystems;
use bevy::render::extract_resource::ExtractResourcePlugin;
use bevy::render::render_resource::PipelineCache;
use bevy::render::render_resource::TextureFormat;
use bevy::render::render_resource::TextureFormatFeatureFlags;
use bevy::render::renderer::RenderAdapter;
use bevy::render::renderer::RenderDevice;
use bevy::shader::Shader;
use bevy_kana::ToF32;

mod coloring;
mod dispatch;
mod edges;
mod extract;
#[cfg(test)]
mod msdf_parity;
#[cfg(test)]
mod parity;
mod pipeline;
mod readback;
mod request;

pub use self::dispatch::GpuGlyphBudget;
use self::dispatch::dispatch_glyph_compute;
use self::extract::collect_gpu_render_jobs;
use self::pipeline::GpuRasterizerPipeline;
pub(crate) use self::request::AtlasGpuPipe;
pub(crate) use self::request::BuiltGpuRequest;
pub(crate) use self::request::GpuCompletionSink;
use self::request::GpuGlyphRequest;
use self::request::GpuGlyphRequestCommon;
pub(crate) use self::request::GpuRenderJob;
use self::request::GpuRenderJobExtract;
use self::request::GpuRenderJobQueue;
use super::atlas::GlyphAtlas;
use super::atlas::GlyphKey;
use super::atlas::GpuGlyphDispatcher;
use super::atlas_slot::AtlasSlot;
use super::msdf_rasterizer::DistanceField;

/// Outcome of attempting to enqueue a glyph for GPU rasterization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuEnqueueResult {
    /// Request accepted and queued for the next render-schedule
    /// dispatch.
    Queued,
    /// Glyph already cached, already in-flight, or has no outline
    /// (e.g., space). Caller does not need to re-enqueue.
    AlreadyHandled,
    /// Allocation failed because the glyph is larger than a single
    /// atlas page.
    AllocationFailed,
    /// `font_data` could not be parsed or the glyph has no outline.
    NoOutline,
}

/// Enqueues a glyph for GPU rasterization.
///
/// Public entry point used by integrations and tests. Bitmap-size
/// computation and page-region allocation happen synchronously so the
/// atlas allocator stays single-threaded; edge-buffer construction is
/// offloaded to the atlas worker pool.
pub fn enqueue_gpu_glyph(
    slot: &mut AtlasSlot,
    key: GlyphKey,
    font_data: &[u8],
    canonical_size: u32,
    sdf_range: f64,
    padding: u32,
    distance_field: DistanceField,
) -> GpuEnqueueResult {
    enqueue_on_atlas(
        slot.rasterize_target_mut(),
        key,
        font_data,
        canonical_size,
        sdf_range,
        padding,
        distance_field,
    )
}

/// Adds the GPU glyph rasterizer.
///
/// Safe to add unconditionally: atlases that stay on
/// [`super::atlas_config::RasterBackend::Cpu`] never enqueue requests,
/// so the dispatch system runs as a no-op each frame.
pub struct GpuRasterizerPlugin;

/// Internal-asset handle for the shared MSDF/SDF math module imported
/// by the generation and correction kernels via
/// `#import bevy_diegetic::gpu_rasterizer::msdf_common`.
const MSDF_COMMON_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("e0b9bbb7-49c1-4eaa-9c0e-09a78dbb1b9c");

impl Plugin for GpuRasterizerPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            MSDF_COMMON_SHADER_HANDLE,
            "shaders/msdf_common.wgsl",
            Shader::from_wgsl
        );
        embedded_asset!(app, "shaders/sdf_gen.wgsl");
        embedded_asset!(app, "shaders/msdf_gen.wgsl");
        embedded_asset!(app, "shaders/msdf_correct.wgsl");

        app.init_resource::<GpuGlyphBudget>()
            .init_resource::<GpuRenderJobExtract>()
            .add_plugins((
                ExtractResourcePlugin::<GpuGlyphBudget>::default(),
                ExtractResourcePlugin::<GpuRenderJobExtract>::default(),
            ))
            .add_systems(PostStartup, install_dispatcher_on_atlas)
            .add_systems(PostUpdate, collect_gpu_render_jobs);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("gpu_rasterizer: RenderApp unavailable; GPU backend will be inactive");
            return;
        };
        render_app
            .init_resource::<GpuRenderJobQueue>()
            .add_systems(RenderStartup, init_pipeline_render_world)
            .add_systems(
                Render,
                dispatch_glyph_compute.in_set(RenderSystems::PrepareBindGroups),
            );
    }
}

/// Render-startup system that validates wgpu device limits and, on
/// success, builds the SDF compute pipeline.
fn init_pipeline_render_world(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    render_adapter: Res<RenderAdapter>,
    pipeline_cache: Res<PipelineCache>,
    asset_server: Res<AssetServer>,
) {
    if !device_supports_gpu_rasterization(&render_device, &render_adapter) {
        return;
    }
    let pipeline = GpuRasterizerPipeline::create(&pipeline_cache, &asset_server);
    commands.insert_resource(pipeline);
}

/// Returns whether the active wgpu device meets the GPU rasterizer's
/// minimum requirements. Logs the failing check on rejection.
fn device_supports_gpu_rasterization(
    render_device: &RenderDevice,
    render_adapter: &RenderAdapter,
) -> bool {
    const EDGE_BUFFER_FLOOR_BYTES: u32 = 256 * 1024;

    let limits = render_device.limits();
    if limits.max_storage_buffer_binding_size < EDGE_BUFFER_FLOOR_BYTES {
        warn!(
            "gpu_rasterizer: max_storage_buffer_binding_size={} < {} required; GPU \
             rasterization disabled",
            limits.max_storage_buffer_binding_size, EDGE_BUFFER_FLOOR_BYTES
        );
        return false;
    }
    if limits.max_storage_buffers_per_shader_stage < 2 {
        warn!(
            "gpu_rasterizer: max_storage_buffers_per_shader_stage={} < 2 required; GPU \
             rasterization disabled",
            limits.max_storage_buffers_per_shader_stage
        );
        return false;
    }
    if limits.max_compute_workgroup_size_x < 8 || limits.max_compute_workgroup_size_y < 8 {
        warn!(
            "gpu_rasterizer: compute workgroup size {}x{} < 8x8 required; GPU \
             rasterization disabled",
            limits.max_compute_workgroup_size_x, limits.max_compute_workgroup_size_y
        );
        return false;
    }
    let rgba8_features = render_adapter.get_texture_format_features(TextureFormat::Rgba8Unorm);
    if !rgba8_features
        .allowed_usages
        .contains(bevy::render::render_resource::TextureUsages::STORAGE_BINDING)
    {
        warn!(
            "gpu_rasterizer: Rgba8Unorm lacks STORAGE_BINDING usage on this adapter; GPU \
             rasterization disabled"
        );
        return false;
    }
    let _ = TextureFormatFeatureFlags::STORAGE_READ_WRITE;
    true
}

/// Dispatcher implementation installed on every atlas at startup and
/// inherited by pending atlases during swaps.
struct ChannelGpuDispatcher;

impl GpuGlyphDispatcher for ChannelGpuDispatcher {
    fn dispatch(
        &self,
        atlas: &mut GlyphAtlas,
        key: GlyphKey,
        font_data: &[u8],
        canonical_size: u32,
        sdf_range: f64,
        padding: u32,
        distance_field: DistanceField,
    ) -> bool {
        !matches!(
            enqueue_on_atlas(
                atlas,
                key,
                font_data,
                canonical_size,
                sdf_range,
                padding,
                distance_field,
            ),
            GpuEnqueueResult::AllocationFailed
        )
    }
}

fn enqueue_on_atlas(
    atlas: &mut GlyphAtlas,
    key: GlyphKey,
    font_data: &[u8],
    canonical_size: u32,
    sdf_range: f64,
    padding: u32,
    distance_field: DistanceField,
) -> GpuEnqueueResult {
    if atlas.is_ready(key) || atlas.in_flight_keys().any(|k| k == key) {
        return GpuEnqueueResult::AlreadyHandled;
    }
    let Some(bitmap_size) = edges::glyph_bitmap_size(
        font_data,
        key.glyph_index,
        canonical_size,
        sdf_range,
        padding,
    ) else {
        atlas.insert_completed_gpu_invisible(key);
        return GpuEnqueueResult::NoOutline;
    };
    let Some(region) = atlas.allocate_gpu_region(bitmap_size) else {
        return GpuEnqueueResult::AllocationFailed;
    };
    atlas.mark_in_flight(key);

    let pool = atlas.worker_pool();
    let (built_tx, completions) = atlas.gpu_pipe_handles();
    let font_data = font_data.to_vec();
    let sdf_range_f32 = sdf_range.to_f32();
    pool.spawn(async move {
        let msg = if let Some(body) = edges::build_edge_buffer(
            &font_data,
            key.glyph_index,
            canonical_size,
            sdf_range,
            padding,
            distance_field,
        ) {
            let common = GpuGlyphRequestCommon {
                key,
                body,
                sdf_range: sdf_range_f32,
                atlas_origin: region.atlas_origin,
                page_index: region.page_index,
            };
            let request = match distance_field {
                DistanceField::Sdf => GpuGlyphRequest::Sdf(common),
                DistanceField::Msdf => GpuGlyphRequest::Msdf(common),
                DistanceField::Mtsdf => GpuGlyphRequest::Mtsdf(common),
            };
            BuiltGpuRequest::built(Box::new(request), completions)
        } else {
            BuiltGpuRequest::invisible(key)
        };
        let _ = built_tx.send(msg);
    })
    .detach();

    GpuEnqueueResult::Queued
}

/// Installs a [`ChannelGpuDispatcher`] on the active atlas at startup
/// so `atlas.get_or_insert` routes through the GPU path when the
/// atlas is configured for [`super::atlas_config::RasterBackend::Gpu`].
fn install_dispatcher_on_atlas(mut slot: ResMut<AtlasSlot>) {
    slot.active_mut()
        .set_gpu_dispatcher(Arc::new(ChannelGpuDispatcher));
}
