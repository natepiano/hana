//! GPU compute-shader glyph rasterizer (Phase 1: SDF).
//!
//! Sibling to the CPU `fdsm` path in [`super::msdf_rasterizer`]. Opt
//! in per atlas by setting [`super::atlas_config::RasterBackend::Gpu`]
//! on the [`super::atlas_config::AtlasConfig`].
//!
//! ## Wiring
//!
//! - Main world: a worker task per glyph builds the edge buffer (via [`edges::build_edge_buffer`])
//!   and pushes a [`request::GpuGlyphRequest`] into [`request::GpuGlyphRequestQueue`].
//! - Extract: the queue and the atlas page handles flow into the render world via
//!   [`bevy::render::extract_resource::ExtractResourcePlugin`].
//! - Render schedule, [`bevy::render::RenderSystems::PrepareBindGroups`]:
//!   [`dispatch::dispatch_glyph_compute`] drains the queue, groups by page, encodes one compute
//!   pass per page, writes records into [`dispatch::GpuGlyphCompletionBuffer`].
//! - Extract back: the completion buffer flows to the main world.
//!   [`extract::drain_gpu_completions`] turns each record into a [`request::GpuGlyphCompleted`]
//!   event; an atlas-side observer calls `insert_completed_gpu`.

use std::sync::Mutex;
use std::sync::mpsc;

use bevy::app::App;
use bevy::app::Plugin;
use bevy::app::PostUpdate;
use bevy::app::PreUpdate;
use bevy::app::Update;
use bevy::asset::embedded_asset;
use bevy::ecs::observer::On;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::ResMut;
use bevy::log::warn;
use bevy::render::Render;
use bevy::render::RenderApp;
use bevy::render::RenderStartup;
use bevy::render::RenderSystems;
use bevy::render::extract_resource::ExtractResourcePlugin;

mod dispatch;
mod edges;
mod extract;
#[cfg(test)]
mod parity;
mod pipeline;
mod readback;
mod request;

pub use self::dispatch::GpuGlyphBudget;
use self::dispatch::RenderAtlasPages;
pub use self::request::GpuGlyphCompleted;
pub use self::request::GpuGlyphRequestQueue;
pub use self::request::GpuGlyphRequestSender;
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
    /// Allocation failed — the glyph is larger than a single atlas
    /// page. Caller should fall back to CPU rasterization or log.
    AllocationFailed,
    /// `font_data` could not be parsed or the glyph has no outline.
    NoOutline,
}

/// Enqueues a glyph for GPU rasterization.
///
/// Public entry point used by text-shaping integrations (and by
/// tests / benchmarks). The cheap parts — bitmap-size computation and
/// page-region allocation — happen synchronously so the atlas's
/// shelf allocator stays single-threaded; the expensive part
/// (`build_edge_buffer`, ~hundreds of µs per glyph) is offloaded to
/// the atlas's existing worker pool. The completed request lands in
/// the queue on the frame after the worker finishes, via
/// [`drain_request_channel`].
///
/// Returns immediately. Use [`super::super::atlas::GlyphAtlas::is_ready`]
/// or watch for [`GpuGlyphCompleted`] events to know when the glyph
/// is actually rasterized.
pub fn enqueue_gpu_glyph(
    slot: &mut AtlasSlot,
    sender: &GpuGlyphRequestSender,
    key: GlyphKey,
    font_data: &[u8],
    canonical_size: u32,
    sdf_range: f64,
    padding: u32,
    distance_field: DistanceField,
) -> GpuEnqueueResult {
    let atlas = slot.rasterize_target_mut();
    if atlas.is_ready(key) {
        return GpuEnqueueResult::AlreadyHandled;
    }
    if atlas.in_flight_keys().any(|k| k == key) {
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
    let tx = sender.sender.clone();
    let font_data = font_data.to_vec();
    #[allow(
        clippy::cast_possible_truncation,
        reason = "sdf_range is small (~4.0); f64→f32 is exact"
    )]
    let sdf_range_f32 = sdf_range as f32;
    pool.spawn(async move {
        let msg = match edges::build_edge_buffer(
            &font_data,
            key.glyph_index,
            canonical_size,
            sdf_range,
            padding,
        ) {
            Some(body) => BuiltRequest::Built(Box::new(GpuGlyphRequest {
                key,
                body,
                sdf_range: sdf_range_f32,
                distance_field,
                atlas_origin: region.atlas_origin,
                page_index: region.page_index,
            })),
            None => BuiltRequest::Invisible(key),
        };
        let _ = tx.send(msg);
    })
    .detach();

    GpuEnqueueResult::Queued
}

use bevy::app::PostStartup;
use bevy::asset::AssetServer;
use bevy::ecs::system::Commands;
use bevy::ecs::system::Res;
use bevy::render::render_resource::PipelineCache;
use bevy::render::render_resource::TextureFormat;
use bevy::render::render_resource::TextureFormatFeatureFlags;
use bevy::render::renderer::RenderAdapter;
use bevy::render::renderer::RenderDevice;
use request::BuiltRequest;
use request::GpuGlyphRequest;

use self::dispatch::GpuGlyphCompletionBuffer;
use self::dispatch::dispatch_glyph_compute;
use self::extract::clear_main_request_queue;
use self::extract::drain_gpu_completions;
use self::extract::drain_request_channel;
use self::extract::sync_render_atlas_pages;
use self::pipeline::GpuRasterizerPipeline;
use self::request::GpuGlyphRequestReceiver;
use super::atlas::GpuAtlasRegion;

/// Adds the GPU glyph rasterizer.
///
/// Safe to add unconditionally: atlases that stay on
/// [`super::atlas_config::RasterBackend::Cpu`] never enqueue requests,
/// so the dispatch system runs as a no-op each frame.
pub struct GpuRasterizerPlugin;

impl Plugin for GpuRasterizerPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/sdf_gen.wgsl");

        let (tx, rx) = mpsc::channel();
        app.insert_resource(self::request::GpuGlyphRequestSender { sender: tx })
            .insert_resource(GpuGlyphRequestReceiver {
                receiver: Mutex::new(rx),
            })
            .init_resource::<GpuGlyphRequestQueue>()
            .init_resource::<GpuGlyphBudget>()
            .init_resource::<RenderAtlasPages>()
            .init_resource::<GpuGlyphCompletionBuffer>()
            .add_plugins((
                ExtractResourcePlugin::<GpuGlyphRequestQueue>::default(),
                ExtractResourcePlugin::<GpuGlyphBudget>::default(),
                ExtractResourcePlugin::<RenderAtlasPages>::default(),
                // Cloning the buffer here shares the Arc<Mutex<...>>
                // between main and render worlds — render pushes,
                // main drains.
                ExtractResourcePlugin::<GpuGlyphCompletionBuffer>::default(),
            ))
            // PostStartup so the TextPlugin's atlas has already been
            // inserted and its image_handle is populated.
            .add_systems(PostStartup, install_dispatcher_on_atlas)
            // Clear main-world queue at start of PreUpdate (after
            // Extract has run on the previous frame's contents), then
            // drain the channel into the freshly-empty queue. Order is
            // critical: if clear runs after drain (or in PostUpdate
            // before Extract), the queue is wiped before the render
            // world's Extract schedule can clone it, and
            // dispatch_glyph_compute sees nothing.
            .add_systems(
                PreUpdate,
                (clear_main_request_queue, drain_request_channel).chain(),
            )
            .add_systems(Update, sync_render_atlas_pages)
            .add_systems(PostUpdate, drain_gpu_completions)
            .add_observer(finalize_gpu_completion);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("gpu_rasterizer: RenderApp unavailable; GPU backend will be inactive");
            return;
        };
        render_app
            .add_systems(RenderStartup, init_pipeline_render_world)
            .add_systems(
                Render,
                dispatch_glyph_compute.in_set(RenderSystems::PrepareBindGroups),
            );
    }
}

/// Render-startup system that validates wgpu device limits and, on
/// success, builds the SDF compute pipeline.
///
/// On failure (insufficient storage-buffer size, missing
/// `STORAGE_READ_WRITE` for `Rgba8Unorm`, undersized compute
/// workgroups), logs a warning and skips pipeline insertion. The
/// dispatch system gracefully no-ops when [`GpuRasterizerPipeline`] is
/// absent — atlases configured for GPU rasterization will stay in
/// [`super::atlas_config::RasterBackend::Cpu`] semantics until a
/// device with sufficient limits is reachable.
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
    // Worst-case edge buffer: 2000 edges × 36 bytes ≈ 72 KB. Picked
    // 256 KB as a generous floor that survives shrunk mobile limits.
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
    // Edges + headers + (texture binding does not count) + uniform = 3 storage entries.
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
    // Write-only access for Rgba8Unorm is core wgpu, but documented
    // here so future MSDF (RGBA read-after-write) can add a stronger
    // check.
    let _ = TextureFormatFeatureFlags::STORAGE_READ_WRITE;
    true
}

/// Main-world observer: routes each [`GpuGlyphCompleted`] event to
/// the active atlas's `insert_completed_gpu`.
fn finalize_gpu_completion(trigger: On<GpuGlyphCompleted>, mut slot: ResMut<AtlasSlot>) {
    let event = trigger.event();
    let atlas = slot.active_mut();
    let region = GpuAtlasRegion {
        page_index:   event.page_index,
        atlas_origin: event.atlas_origin,
        bitmap_size:  event.bitmap_size,
    };
    let metrics = atlas.metrics_for_gpu_region(region, event.bearing.x, event.bearing.y);
    atlas.insert_completed_gpu(event.key, metrics);
}

/// Dispatcher implementation installed on every atlas at startup and
/// after each swap. Wraps the worker→main channel sender so the atlas
/// can branch into the GPU path inside `lookup_or_queue` without
/// importing any `gpu_rasterizer` types.
struct ChannelGpuDispatcher {
    sender: self::request::GpuGlyphRequestSender,
}

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
        if atlas.is_ready(key) {
            return true;
        }
        if atlas.in_flight_keys().any(|k| k == key) {
            return true;
        }
        let Some(bitmap_size) = edges::glyph_bitmap_size(
            font_data,
            key.glyph_index,
            canonical_size,
            sdf_range,
            padding,
        ) else {
            atlas.insert_completed_gpu_invisible(key);
            return true;
        };
        let Some(region) = atlas.allocate_gpu_region(bitmap_size) else {
            return false;
        };
        atlas.mark_in_flight(key);

        let pool = atlas.worker_pool();
        let tx = self.sender.sender.clone();
        let font_data = font_data.to_vec();
        #[allow(
            clippy::cast_possible_truncation,
            reason = "sdf_range is small (~4.0); f64→f32 is exact"
        )]
        let sdf_range_f32 = sdf_range as f32;
        pool.spawn(async move {
            let msg = if let Some(body) = edges::build_edge_buffer(
                &font_data,
                key.glyph_index,
                canonical_size,
                sdf_range,
                padding,
            ) {
                BuiltRequest::Built(Box::new(GpuGlyphRequest {
                    key,
                    body,
                    sdf_range: sdf_range_f32,
                    distance_field,
                    atlas_origin: region.atlas_origin,
                    page_index: region.page_index,
                }))
            } else {
                BuiltRequest::Invisible(key)
            };
            let _ = tx.send(msg);
        })
        .detach();
        true
    }
}

/// Installs a [`ChannelGpuDispatcher`] on the active atlas at startup
/// so `atlas.get_or_insert` routes through the GPU path when the
/// atlas is configured for [`super::atlas_config::RasterBackend::Gpu`].
fn install_dispatcher_on_atlas(
    sender: Res<self::request::GpuGlyphRequestSender>,
    mut slot: ResMut<AtlasSlot>,
) {
    let dispatcher = std::sync::Arc::new(ChannelGpuDispatcher {
        sender: sender.clone(),
    });
    slot.active_mut().set_gpu_dispatcher(dispatcher);
}
