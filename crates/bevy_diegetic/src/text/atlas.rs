//! MSDF glyph atlas — packs rasterized glyphs into paged textures.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::time::Instant;

use bevy::image::Image;
use bevy::log::warn;
use bevy::math::UVec2;
use bevy::prelude::Assets;
use bevy::prelude::Handle;
use bevy::prelude::Resource;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDimension;
use bevy::render::render_resource::TextureFormat;
use bevy::render::render_resource::TextureUsages;
use bevy::tasks::TaskPool;
use bevy::tasks::TaskPoolBuilder;
use bevy_kana::ToF32;
use bevy_kana::ToI32;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;
use etagere::AtlasAllocator;
use etagere::size2;

use super::atlas_config::RasterBackend;
use super::constants::ATLAS_GUTTER;
use super::constants::BYTES_PER_PIXEL;
use super::constants::DEFAULT_ATLAS_SIZE;
use super::constants::DEFAULT_GLYPH_WORKER_THREADS;
use super::constants::GLYPH_WORKER_THREAD_NAME;
use super::font::Font;
use super::font_registry::FontId;
use super::font_registry::FontRegistry;
use super::gpu_rasterizer::AtlasGpuPipe;
use super::gpu_rasterizer::BuiltGpuRequest;
use super::gpu_rasterizer::GpuCompletionSink;
use super::gpu_rasterizer::GpuRenderJob;
use super::msdf_rasterizer::DEFAULT_CANONICAL_SIZE;
use super::msdf_rasterizer::DEFAULT_GLYPH_PADDING;
use super::msdf_rasterizer::DEFAULT_SDF_RANGE;
use super::msdf_rasterizer::DistanceField;
use super::msdf_rasterizer::MsdfRasterizer;
use super::msdf_rasterizer::RasterizedBitmap;
use super::msdf_rasterizer::Rasterizer;
use super::msdf_rasterizer::SdfRasterizer;
use crate::constants::MILLISECONDS_PER_SECOND;

/// Key for looking up a cached glyph in the atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    /// Font identifier from [`FontId`](crate::FontId).
    pub font_id:     u16,
    /// Glyph index within the font.
    pub glyph_index: u16,
}

/// Metrics for a single glyph stored in the atlas.
///
/// **Mode-agnostic**: UV rect, bearings, and pixel dimensions are
/// identical regardless of which rasterizer (MSDF vs. SDF) produced the
/// underlying bitmap. The atlas insert path computes the same allocation
/// extent and bearing offsets for both modes; only the per-pixel byte
/// payload differs. This means downstream code that builds glyph quads
/// from `GlyphMetrics` does not need to branch on the atlas's mode.
#[derive(Clone, Copy, Debug)]
pub struct GlyphMetrics {
    /// UV rectangle in the atlas texture: `[u_min, v_min, u_max, v_max]`.
    pub uv_rect:      [f32; 4],
    /// Font-defined horizontal bearing (em units, ink origin to ink
    /// left). Atlas-invariant — equal across atlases of different
    /// canonical sizes for the same font/glyph. Quad-builders combine
    /// this with `pad_x_em` to position the padded quad.
    pub bearing_x:    f32,
    /// Font-defined vertical bearing (em units, baseline to ink top).
    /// Atlas-invariant.
    pub bearing_y:    f32,
    /// Atlas-specific outward padding on the X axis (em units). The
    /// bitmap extends `pad_x_em` em-units to the left of the ink and
    /// `pad_x_em` to the right. Differs per canonical size because the
    /// bitmap dimensions are integer-rounded; quad-builders subtract
    /// it at the corner so the ink lands at the same em-coordinate
    /// regardless of which atlas served the glyph.
    pub pad_x_em:     f32,
    /// Atlas-specific outward padding on the Y axis (em units).
    pub pad_y_em:     f32,
    /// Glyph bitmap width in pixels.
    pub pixel_width:  u32,
    /// Glyph bitmap height in pixels.
    pub pixel_height: u32,
    /// Atlas page this glyph is stored on.
    pub page_index:   u32,
}

impl GlyphMetrics {
    /// Sentinel for glyphs with no visual representation (e.g. space).
    /// Zero-sized so no quad is generated, but present in the cache so
    /// the glyph isn't re-queued for async rasterization.
    pub(crate) const INVISIBLE: Self = Self {
        uv_rect:      [0.0; 4],
        bearing_x:    0.0,
        bearing_y:    0.0,
        pad_x_em:     0.0,
        pad_y_em:     0.0,
        pixel_width:  0,
        pixel_height: 0,
        page_index:   0,
    };
}

/// Page region reserved for a GPU-dispatched glyph rasterization.
///
/// Returned by [`GlyphAtlas::allocate_gpu_region`] so the dispatcher
/// knows where in the page texture to write and the completion observer
/// knows how to build [`GlyphMetrics`].
#[derive(Clone, Copy, Debug)]
pub struct GpuAtlasRegion {
    /// Atlas page index the bitmap will be written into.
    pub page_index:   u32,
    /// Top-left texel of the bitmap interior on the page.
    pub atlas_origin: UVec2,
    /// Bitmap dimensions in texels (excludes the gutter ring).
    pub bitmap_size:  UVec2,
}

/// Completed async rasterization result.
struct RasterizedGlyph {
    key:               GlyphKey,
    /// `None` for glyphs with no visual representation (e.g. space).
    bitmap:            Option<RasterizedBitmap>,
    /// End-to-end raster time on the worker.
    elapsed_ms:        f32,
    /// Worker thread id that completed the raster.
    worker:            String,
    /// Number of raster jobs concurrently active when this job started.
    start_active_jobs: usize,
}

/// Result of a glyph atlas lookup with async queueing semantics.
#[derive(Clone, Copy, Debug)]
pub enum GlyphLookup {
    /// Glyph metrics are already available in the atlas.
    Ready(GlyphMetrics),
    /// Glyph was already queued and is still being rasterized.
    Pending,
    /// Glyph was queued by this lookup and will be available later.
    Queued,
}

/// Diagnostics from draining completed async glyph rasterizations.
#[derive(Clone, Copy, Debug, Default)]
pub struct AsyncGlyphPollStats {
    /// Number of completed async jobs drained from the channel.
    pub completed:       usize,
    /// Number of visible glyphs inserted into atlas pages.
    pub inserted:        usize,
    /// Number of invisible glyph entries cached (e.g. space).
    pub invisible:       usize,
    /// Number of atlas pages added while inserting completed glyphs.
    pub pages_added:     usize,
    /// Average worker-side raster duration for drained jobs.
    pub avg_raster_ms:   f32,
    /// Maximum worker-side raster duration for drained jobs.
    pub max_raster_ms:   f32,
    /// Maximum concurrent active raster jobs reported by drained jobs.
    pub max_active_jobs: usize,
    /// Number of distinct worker threads seen in drained jobs.
    pub worker_threads:  usize,
}

/// Whether a page's CPU pixel data has been modified since the last GPU sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtlasPageState {
    Clean,
    Dirty,
}

/// A single page of the MSDF atlas texture.
struct AtlasPage {
    /// Raw pixel data (RGBA8, row-major).
    pixels:       Vec<u8>,
    /// Rectangle allocator for packing glyphs.
    allocator:    AtlasAllocator,
    /// Handle to the GPU image (populated after upload).
    image_handle: Option<Handle<Image>>,
    /// Whether the CPU pixel data has changed since last GPU sync.
    state:        AtlasPageState,
}

impl AtlasPage {
    fn new(width: u32, height: u32) -> Self {
        let pixel_count = (width * height * BYTES_PER_PIXEL).to_usize();
        Self {
            pixels:       vec![0; pixel_count],
            allocator:    AtlasAllocator::new(size2(width.to_i32(), height.to_i32())),
            image_handle: None,
            state:        AtlasPageState::Clean,
        }
    }
}

/// Glyph atlas with automatic page overflow.
///
/// Stores RGBA textures containing rasterized glyph bitmaps and a
/// lookup table mapping [`GlyphKey`] to [`GlyphMetrics`]. Glyphs are
/// packed into pages using `etagere`'s shelf-packing algorithm. When a
/// page fills, a new page is allocated automatically.
///
/// The atlas owns an `Arc<dyn Rasterizer>` chosen at construction time
/// from the requested [`DistanceField`] — either MSDF (three channels,
/// sharp corners) or SDF (one channel, smooth curves). Both modes
/// write into the same RGBA atlas texture; the shader branches on a
/// per-material uniform.
///
/// Glyph rasterization is asynchronous — cache misses spawn tasks on
/// the atlas's worker pool and return `None`. Call
/// [`poll_async_glyphs`](Self::poll_async_glyphs) each frame to insert
/// completed results.
#[derive(Resource)]
pub struct GlyphAtlas {
    /// Atlas pages, each with its own pixel buffer and allocator.
    pages:             Vec<AtlasPage>,
    /// Cached glyph metrics, keyed by `GlyphKey`.
    glyphs:            HashMap<GlyphKey, GlyphMetrics>,
    /// Page width in pixels (all pages share the same dimensions).
    width:             u32,
    /// Page height in pixels.
    height:            u32,
    /// Canonical pixel size for glyph rasterization.
    canonical_size:    u32,
    /// Glyph keys currently being rasterized asynchronously.
    in_flight:         HashSet<GlyphKey>,
    /// Receiver for completed async rasterizations (Mutex for Sync).
    rx:                Mutex<Receiver<RasterizedGlyph>>,
    /// Sender cloned into each async task.
    tx:                Sender<RasterizedGlyph>,
    /// Number of raster jobs currently executing on worker threads.
    active_jobs:       Arc<AtomicUsize>,
    /// Peak concurrently executing raster jobs observed so far.
    peak_active_jobs:  Arc<AtomicUsize>,
    /// Worker pool for async glyph rasterization. Shared across atlas
    /// instances during a mode swap so the pending atlas reuses the
    /// active atlas's threads instead of doubling the live thread
    /// count.
    glyph_worker_pool: Arc<TaskPool>,
    /// Rasterizer chosen at construction. Cloned into each async task
    /// so workers hold a clone independent of the atlas lock.
    rasterizer:        Arc<dyn Rasterizer>,
    /// Which device rasterizes glyphs. CPU uses `rasterizer` above;
    /// GPU uses the dispatcher set via [`Self::set_gpu_dispatcher`].
    /// Defaults to [`RasterBackend::Cpu`].
    backend:           RasterBackend,
    /// Callback used to route a single glyph through the GPU path.
    /// Installed by `GpuRasterizerPlugin::build` once per atlas. When
    /// `None` (CPU-only atlas or pre-init), `get_or_insert` falls
    /// back to the CPU path even if `backend == Gpu`.
    gpu_dispatcher:    Option<Arc<dyn GpuGlyphDispatcher>>,
    /// Per-atlas worker, dispatch, and completion pipe for GPU glyphs.
    gpu_pipe:          Option<AtlasGpuPipe>,
}

/// Plug-point the atlas uses to route a glyph through the GPU
/// rasterizer without depending on the `gpu_rasterizer` module
/// directly. `GpuRasterizerPlugin` installs an implementation that
/// allocates the page region, marks the glyph in-flight, and spawns
/// the edge-build task.
pub trait GpuGlyphDispatcher: Send + Sync + 'static {
    /// Enqueues `key` for GPU rasterization. The implementation
    /// owns all GPU-specific state (sender channel, worker pool) and
    /// must be safe to call from any thread.
    ///
    /// Returns `true` if the request was accepted (queued or already
    /// handled); `false` only if the dispatcher could not enqueue
    /// (oversized glyph, unsupported state) so the caller can fall
    /// back to CPU.
    fn dispatch(
        &self,
        atlas: &mut GlyphAtlas,
        key: GlyphKey,
        font_data: &[u8],
        canonical_size: u32,
        sdf_range: f64,
        padding: u32,
        distance_field: DistanceField,
    ) -> bool;
}

impl GlyphAtlas {
    /// Creates a new atlas with the default page size, canonical size,
    /// and MSDF rasterizer.
    #[must_use]
    pub fn new() -> Self {
        Self::new_with_dimensions(
            DEFAULT_ATLAS_SIZE,
            DEFAULT_ATLAS_SIZE,
            DEFAULT_CANONICAL_SIZE,
            DEFAULT_GLYPH_WORKER_THREADS,
            DistanceField::Msdf,
            None,
        )
    }

    /// Creates a new atlas with a specific page size (default canonical
    /// size and MSDF rasterizer).
    #[must_use]
    pub fn with_size(width: u32, height: u32) -> Self {
        Self::new_with_dimensions(
            width,
            height,
            DEFAULT_CANONICAL_SIZE,
            DEFAULT_GLYPH_WORKER_THREADS,
            DistanceField::Msdf,
            None,
        )
    }

    /// Creates a new atlas configured from `AtlasConfig`-derived values.
    ///
    /// `shared_worker_pool`: when `Some`, the new atlas reuses the given
    /// pool instead of allocating its own. Used by the mode-swap path
    /// so the pending atlas does not double the live thread count.
    #[must_use]
    pub fn with_config(
        page_size: u32,
        canonical_size: u32,
        glyph_worker_threads: usize,
        distance_field: DistanceField,
        shared_worker_pool: Option<Arc<TaskPool>>,
    ) -> Self {
        Self::new_with_dimensions(
            page_size,
            page_size,
            canonical_size,
            glyph_worker_threads,
            distance_field,
            shared_worker_pool,
        )
    }

    /// Sets the backend the atlas dispatches to in `get_or_insert`.
    pub const fn set_backend(&mut self, backend: RasterBackend) { self.backend = backend; }

    /// Returns the backend the atlas dispatches to in `get_or_insert`.
    #[must_use]
    pub const fn backend(&self) -> RasterBackend { self.backend }

    /// Installs a GPU dispatcher so [`Self::get_or_insert`] can route
    /// glyphs through the compute-shader path when
    /// [`Self::backend()`] is [`RasterBackend::Gpu`].
    pub fn set_gpu_dispatcher(&mut self, dispatcher: Arc<dyn GpuGlyphDispatcher>) {
        self.ensure_gpu_pipe();
        self.gpu_dispatcher = Some(dispatcher);
    }

    /// Returns whether the atlas has a GPU dispatcher installed.
    #[must_use]
    pub fn has_gpu_dispatcher(&self) -> bool { self.gpu_dispatcher.is_some() }

    /// Returns a clone of the installed GPU dispatcher handle, if any.
    /// Used by the atlas-swap path so the pending atlas inherits the
    /// active atlas's dispatcher.
    #[must_use]
    pub fn gpu_dispatcher_handle(&self) -> Option<Arc<dyn GpuGlyphDispatcher>> {
        self.gpu_dispatcher.clone()
    }

    fn ensure_gpu_pipe(&mut self) -> &mut AtlasGpuPipe {
        self.gpu_pipe.get_or_insert_with(AtlasGpuPipe::new)
    }

    /// Returns worker and completion endpoints for this atlas's GPU pipe.
    pub(crate) fn gpu_pipe_handles(
        &mut self,
    ) -> (mpsc::Sender<BuiltGpuRequest>, GpuCompletionSink) {
        let pipe = self.ensure_gpu_pipe();
        (pipe.built_tx.clone(), pipe.completions.clone())
    }

    /// Moves pending render jobs into `out` for extraction.
    pub(crate) fn drain_gpu_render_jobs(&mut self, out: &mut Vec<GpuRenderJob>) {
        if let Some(pipe) = self.gpu_pipe.as_mut() {
            out.append(&mut pipe.pending_dispatch);
        }
    }

    fn new_with_dimensions(
        width: u32,
        height: u32,
        canonical_size: u32,
        glyph_worker_threads: usize,
        distance_field: DistanceField,
        shared_worker_pool: Option<Arc<TaskPool>>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let rasterizer: Arc<dyn Rasterizer> = match distance_field {
            DistanceField::Msdf => Arc::new(MsdfRasterizer::new(
                canonical_size,
                DEFAULT_SDF_RANGE,
                DEFAULT_GLYPH_PADDING,
            )),
            DistanceField::Sdf => Arc::new(SdfRasterizer::new(
                canonical_size,
                DEFAULT_SDF_RANGE,
                DEFAULT_GLYPH_PADDING,
            )),
        };
        let glyph_worker_pool = shared_worker_pool.unwrap_or_else(|| {
            Arc::new(
                TaskPoolBuilder::new()
                    .num_threads(glyph_worker_threads)
                    .thread_name(GLYPH_WORKER_THREAD_NAME.to_string())
                    .build(),
            )
        });
        Self {
            pages: vec![AtlasPage::new(width, height)],
            glyphs: HashMap::new(),
            width,
            height,
            canonical_size,
            in_flight: HashSet::new(),
            rx: Mutex::new(rx),
            tx,
            active_jobs: Arc::new(AtomicUsize::new(0)),
            peak_active_jobs: Arc::new(AtomicUsize::new(0)),
            glyph_worker_pool,
            rasterizer,
            backend: RasterBackend::Cpu,
            gpu_dispatcher: None,
            gpu_pipe: None,
        }
    }

    /// Distance-field variant this atlas was built with.
    #[must_use]
    pub fn distance_field(&self) -> DistanceField { self.rasterizer.mode() }

    /// Whether a glyph is already rasterized into the cache.
    #[must_use]
    pub fn is_ready(&self, key: GlyphKey) -> bool { self.glyphs.contains_key(&key) }

    /// Returns a clone of the shared worker pool. Used by the mode-swap
    /// path so the pending atlas reuses the active atlas's threads.
    #[must_use]
    pub fn worker_pool(&self) -> Arc<TaskPool> { Arc::clone(&self.glyph_worker_pool) }

    /// Iterates over keys currently being rasterized asynchronously.
    pub fn in_flight_keys(&self) -> impl Iterator<Item = GlyphKey> + '_ {
        self.in_flight.iter().copied()
    }

    /// Returns the page width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 { self.width }

    /// Returns the page height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 { self.height }

    /// Returns the canonical rasterization size in pixels.
    #[must_use]
    pub const fn canonical_size(&self) -> u32 { self.canonical_size }

    /// Returns the number of atlas pages currently allocated.
    #[must_use]
    pub const fn page_count(&self) -> usize { self.pages.len() }

    /// Returns the total number of cached glyphs across all pages.
    #[must_use]
    pub fn glyph_count(&self) -> usize { self.glyphs.len() }

    /// Returns the number of glyphs currently being rasterized asynchronously.
    #[must_use]
    pub fn in_flight_count(&self) -> usize { self.in_flight.len() }

    /// Returns the number of raster jobs currently executing on worker threads.
    #[must_use]
    pub fn active_job_count(&self) -> usize { self.active_jobs.load(Ordering::Relaxed) }

    /// Returns the peak worker-side raster concurrency observed so far.
    #[must_use]
    pub fn peak_active_job_count(&self) -> usize { self.peak_active_jobs.load(Ordering::Relaxed) }

    /// Returns the number of atlas pages with unsynced CPU-side changes.
    #[must_use]
    pub fn dirty_page_count(&self) -> usize {
        self.pages
            .iter()
            .filter(|p| matches!(p.state, AtlasPageState::Dirty))
            .count()
    }

    /// Looks up cached metrics for a glyph without triggering rasterization.
    ///
    /// Returns `None` if the glyph hasn't been rasterized yet.
    #[must_use]
    pub fn get_metrics(&self, key: GlyphKey) -> Option<GlyphMetrics> {
        self.glyphs.get(&key).copied()
    }

    /// Iterates over all glyphs currently stored in the atlas.
    ///
    /// This exposes the atlas's canonical glyph keys and metrics, which is
    /// necessary for tooling that needs to inspect glyph storage without
    /// reconstructing keys from source characters.
    pub fn iter_glyphs(&self) -> impl Iterator<Item = (&GlyphKey, &GlyphMetrics)> {
        self.glyphs.iter()
    }

    /// Returns the raw RGBA pixel data for a page.
    #[must_use]
    pub fn page_pixels(&self, page: usize) -> Option<&[u8]> {
        self.pages.get(page).map(|p| p.pixels.as_slice())
    }

    /// Looks up cached metrics for a glyph. Test-only.
    #[cfg(test)]
    #[must_use]
    pub fn get(&self, key: GlyphKey) -> Option<&GlyphMetrics> { self.glyphs.get(&key) }

    /// Returns the SDF range used for glyph generation.
    #[must_use]
    pub const fn sdf_range() -> f64 { DEFAULT_SDF_RANGE }

    /// Per-side padding baked into every rasterized glyph bitmap, in texels.
    /// Sum of `DEFAULT_GLYPH_PADDING` and `DEFAULT_SDF_RANGE` — the same
    /// `total_pad` used by `rasterize_glyph` to size the bitmap.
    #[must_use]
    pub fn glyph_padding_texels() -> f32 {
        DEFAULT_GLYPH_PADDING.to_f32() + DEFAULT_SDF_RANGE.to_f32()
    }

    /// Returns the GPU image handle for a specific atlas page.
    ///
    /// Returns `None` if the page doesn't exist or hasn't been uploaded yet.
    #[must_use]
    pub fn image_handle(&self, page: u32) -> Option<&Handle<Image>> {
        self.pages
            .get(page.to_usize())
            .and_then(|p| p.image_handle.as_ref())
    }

    /// Creates Bevy `Image` assets for all pages and stores their handles.
    ///
    /// Call once during plugin initialization. Subsequent changes are synced
    /// via [`sync_to_gpu`](Self::sync_to_gpu).
    pub fn upload_to_gpu(&mut self, images: &mut Assets<Image>) {
        for page in &mut self.pages {
            if page.image_handle.is_some() {
                continue;
            }
            let mut image = Image::new(
                Extent3d {
                    width:                 self.width,
                    height:                self.height,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                page.pixels.clone(),
                TextureFormat::Rgba8Unorm,
                bevy::asset::RenderAssetUsages::default(),
            );
            // Atlas pages must allow STORAGE_BINDING so the GPU
            // rasterizer's compute shader can write distance-field
            // texels directly into them. Pages also stay COPY_DST so
            // the existing dirty-page sync_to_gpu path keeps working,
            // and TEXTURE_BINDING so the text fragment shader can
            // sample them.
            image.texture_descriptor.usage |= TextureUsages::STORAGE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::TEXTURE_BINDING;
            page.image_handle = Some(images.add(image));
            page.state = AtlasPageState::Clean;
        }
    }

    /// Syncs changed CPU pixel data to the existing GPU `Image` assets.
    ///
    /// Call after new glyphs are inserted. Handles both dirty existing pages
    /// and newly created pages (allocates GPU Images for them). Bevy's asset
    /// change detection handles the actual GPU upload.
    pub fn sync_to_gpu(&mut self, images: &mut Assets<Image>) {
        for page in &mut self.pages {
            if matches!(page.state, AtlasPageState::Clean) {
                continue;
            }
            if let Some(handle) = page.image_handle.as_ref() {
                // Existing page — update pixel data.
                if let Some(image) = images.get_mut(handle) {
                    image.data = Some(page.pixels.clone());
                }
            } else {
                // New page created at runtime — allocate GPU Image.
                let mut image = Image::new(
                    Extent3d {
                        width:                 self.width,
                        height:                self.height,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    page.pixels.clone(),
                    TextureFormat::Rgba8Unorm,
                    bevy::asset::RenderAssetUsages::default(),
                );
                image.texture_descriptor.usage |= TextureUsages::STORAGE_BINDING
                    | TextureUsages::COPY_DST
                    | TextureUsages::TEXTURE_BINDING;
                page.image_handle = Some(images.add(image));
            }
            page.state = AtlasPageState::Clean;
        }
    }

    /// Returns cached metrics for a glyph, or `None` if not yet available.
    ///
    /// On cache miss, queues async rasterization on
    /// [`AsyncComputeTaskPool`]. The glyph will be available after
    /// [`poll_async_glyphs`](Self::poll_async_glyphs) picks up the
    /// completed result.
    pub fn get_or_insert(&mut self, key: GlyphKey, font_data: &[u8]) -> Option<GlyphMetrics> {
        match self.lookup_or_queue(key, font_data) {
            GlyphLookup::Ready(metrics) => Some(metrics),
            GlyphLookup::Pending | GlyphLookup::Queued => None,
        }
    }

    /// Looks up a glyph and reports whether it was ready, already pending,
    /// or newly queued for async rasterization.
    pub fn lookup_or_queue(&mut self, key: GlyphKey, font_data: &[u8]) -> GlyphLookup {
        if let Some(metrics) = self.glyphs.get(&key) {
            return GlyphLookup::Ready(*metrics);
        }

        // Already queued — don't spawn a duplicate task.
        if self.in_flight.contains(&key) {
            return GlyphLookup::Pending;
        }

        // GPU branch: route through the installed dispatcher when the
        // atlas is configured for GPU rasterization. A `false` return
        // means the dispatcher could not enqueue (oversized glyph,
        // missing pipeline); fall through to the CPU path in that
        // case so the glyph still renders.
        if matches!(self.backend, RasterBackend::Gpu)
            && let Some(disp) = self.gpu_dispatcher.clone()
        {
            let canonical = self.canonical_size;
            let mode = self.rasterizer.mode();
            if disp.dispatch(
                self,
                key,
                font_data,
                canonical,
                DEFAULT_SDF_RANGE,
                DEFAULT_GLYPH_PADDING,
                mode,
            ) {
                return GlyphLookup::Queued;
            }
        }

        // Queue async rasterization.
        self.in_flight.insert(key);
        let tx = self.tx.clone();
        let active_jobs = Arc::clone(&self.active_jobs);
        let peak_active_jobs = Arc::clone(&self.peak_active_jobs);
        let glyph_index = key.glyph_index;
        let font_data = font_data.to_vec();
        let rasterizer = Arc::clone(&self.rasterizer);
        self.glyph_worker_pool
            .spawn(async move {
                let active_now = active_jobs.fetch_add(1, Ordering::Relaxed) + 1;
                peak_active_jobs.fetch_max(active_now, Ordering::Relaxed);
                let start = Instant::now();
                let bitmap = rasterizer.rasterize(&font_data, glyph_index);
                let elapsed_ms = start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
                let worker = format!("{:?}", std::thread::current().id());
                let _ = tx.send(RasterizedGlyph {
                    key,
                    bitmap,
                    elapsed_ms,
                    worker,
                    start_active_jobs: active_now,
                });
                active_jobs.fetch_sub(1, Ordering::Relaxed);
            })
            .detach();

        GlyphLookup::Queued
    }

    /// Kicks off async rasterization for every glyph in `text`.
    ///
    /// This does **not** block — glyphs are queued on
    /// [`AsyncComputeTaskPool`] and inserted into the atlas when
    /// [`poll_async_glyphs`](Self::poll_async_glyphs) runs. Preloading
    /// front-loads the async work so glyphs are more likely to be cached
    /// by the time the text is rendered.
    ///
    /// Call this in a [`FontRegistered`](crate::FontRegistered) observer
    /// to warm the atlas for a known character set:
    ///
    /// ```ignore
    /// app.add_observer(|trigger: On<FontRegistered>,
    ///                    mut atlas: ResMut<GlyphAtlas>,
    ///                    font_registry: Res<FontRegistry>| {
    ///     atlas.preload("ABCDEF...", trigger.id, &font_registry);
    /// });
    /// ```
    pub fn preload(&mut self, text: &str, font_id: FontId, font_registry: &FontRegistry) {
        let Some(font_data) = font_registry.font(font_id).map(Font::data) else {
            return;
        };
        let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
            return;
        };
        for ch in text.chars() {
            if let Some(glyph_id) = face.glyph_index(ch) {
                let key = GlyphKey {
                    font_id:     font_id.0,
                    glyph_index: glyph_id.0,
                };
                self.get_or_insert(key, font_data);
            }
        }
    }

    /// Drains completed async rasterizations and inserts them into the atlas.
    ///
    /// Returns `true` if any new glyphs were inserted (callers should
    /// trigger text mesh rebuilds).
    pub fn poll_async_glyphs(&mut self) -> bool {
        let stats = self.poll_async_glyphs_stats();
        stats.inserted > 0 || stats.invisible > 0
    }

    /// Drains completed async rasterizations and returns detailed diagnostics.
    pub fn poll_async_glyphs_stats(&mut self) -> AsyncGlyphPollStats {
        let completed: Vec<_> = {
            let rx = self.rx.lock().unwrap_or_else(PoisonError::into_inner);
            rx.try_iter().collect()
        };
        let pages_before = self.pages.len();
        let mut stats = AsyncGlyphPollStats {
            completed: completed.len(),
            ..Default::default()
        };
        let mut total_raster_ms = 0.0_f32;
        let mut workers = HashSet::new();
        for result in completed {
            total_raster_ms += result.elapsed_ms;
            stats.max_raster_ms = stats.max_raster_ms.max(result.elapsed_ms);
            stats.max_active_jobs = stats.max_active_jobs.max(result.start_active_jobs);
            workers.insert(result.worker);
            self.in_flight.remove(&result.key);
            if self.glyphs.contains_key(&result.key) {
                continue;
            }
            let Some(bitmap) = result.bitmap else {
                // Glyph has no visual representation (e.g. space).
                // Insert zero-sized metrics so future lookups hit the
                // cache instead of re-queuing async rasterization.
                self.glyphs.insert(result.key, GlyphMetrics::INVISIBLE);
                stats.invisible += 1;
                continue;
            };
            if self.insert_bitmap(result.key, &bitmap).is_some() {
                stats.inserted += 1;
            }
        }
        stats.pages_added = self.pages.len().saturating_sub(pages_before);
        if stats.completed > 0 {
            stats.avg_raster_ms = total_raster_ms / stats.completed.to_f32();
        }
        stats.worker_threads = workers.len();
        let gpu_stats = self.poll_gpu();
        stats.completed += gpu_stats.completed;
        stats.inserted += gpu_stats.inserted;
        stats.invisible += gpu_stats.invisible;
        stats
    }

    fn poll_gpu(&mut self) -> AsyncGlyphPollStats {
        let mut stats = AsyncGlyphPollStats::default();
        let (built, completion_records) = {
            let Some(pipe) = self.gpu_pipe.as_ref() else {
                return stats;
            };
            (pipe.drain_built(), pipe.completions.drain())
        };

        let mut new_jobs: Vec<GpuRenderJob> = Vec::with_capacity(built.len());
        let mut invisible_keys: Vec<GlyphKey> = Vec::new();
        for msg in built {
            match msg {
                BuiltGpuRequest::Built {
                    request,
                    completions,
                } => {
                    let page_index = request.page_index.to_usize();
                    let Some(image_handle) = self
                        .pages
                        .get(page_index)
                        .and_then(|page| page.image_handle.clone())
                    else {
                        warn!(
                            "gpu_rasterizer: page {page_index} has no image handle yet; dropping \
                             built request"
                        );
                        continue;
                    };
                    new_jobs.push(GpuRenderJob {
                        request: *request,
                        image_handle,
                        completions,
                    });
                },
                BuiltGpuRequest::Invisible { key } => {
                    stats.completed += 1;
                    if !self.glyphs.contains_key(&key) {
                        stats.invisible += 1;
                    }
                    invisible_keys.push(key);
                },
            }
        }

        for key in invisible_keys {
            self.insert_completed_gpu_invisible(key);
        }
        if !new_jobs.is_empty() {
            self.ensure_gpu_pipe().pending_dispatch.extend(new_jobs);
        }

        for record in completion_records {
            stats.completed += 1;
            let was_cached = self.glyphs.contains_key(&record.key);
            let region = GpuAtlasRegion {
                page_index:   record.page_index,
                atlas_origin: record.atlas_origin,
                bitmap_size:  record.bitmap_size,
            };
            let metrics = self.metrics_for_gpu_region(
                region,
                record.bearing.x,
                record.bearing.y,
                record.pad_em.x,
                record.pad_em.y,
            );
            self.insert_completed_gpu(record.key, metrics);
            if !was_cached {
                stats.inserted += 1;
            }
        }
        stats
    }

    /// Marks a glyph as in-flight without spawning CPU rasterization.
    ///
    /// Used by the GPU dispatch path: after [`Self::allocate_gpu_region`]
    /// reserves a page slot, the caller marks the key in-flight so
    /// subsequent lookups during the GPU dispatch report `Pending`
    /// instead of re-queueing.
    pub fn mark_in_flight(&mut self, key: GlyphKey) { self.in_flight.insert(key); }

    /// Reserves a page region for a GPU-dispatched glyph.
    ///
    /// Allocates `bitmap_size + 2 * ATLAS_GUTTER` on a page (creating a
    /// new page if every existing one is full), returns the interior
    /// origin where the GPU shader should write. Returns `None` only if
    /// the requested bitmap is larger than a single page.
    pub fn allocate_gpu_region(&mut self, bitmap_size: UVec2) -> Option<GpuAtlasRegion> {
        let width = bitmap_size.x;
        let height = bitmap_size.y;
        let g = ATLAS_GUTTER;
        let padded_width = width + 2 * g;
        let padded_height = height + 2 * g;
        let alloc_size = size2(padded_width.to_i32(), padded_height.to_i32());

        let mut found = None;
        for (i, page) in self.pages.iter_mut().enumerate() {
            if let Some(alloc) = page.allocator.allocate(alloc_size) {
                found = Some((i, alloc));
                break;
            }
        }
        let (page_index, alloc) = if let Some(hit) = found {
            hit
        } else {
            let mut new_page = AtlasPage::new(self.width, self.height);
            let alloc = new_page.allocator.allocate(alloc_size)?;
            self.pages.push(new_page);
            (self.pages.len() - 1, alloc)
        };

        let rect = alloc.rectangle;
        let alloc_x = rect.min.x.to_u32();
        let alloc_y = rect.min.y.to_u32();
        let x0 = alloc_x + g;
        let y0 = alloc_y + g;
        // Only mark Dirty when the page is brand new (no image_handle
        // yet) so sync_to_gpu allocates the underlying Image. For pages
        // that already have a handle, sync_to_gpu would blit the empty
        // CPU mirror over the GPU storage texture and wipe previously
        // dispatched GPU texels.
        if self.pages[page_index].image_handle.is_none() {
            self.pages[page_index].state = AtlasPageState::Dirty;
        }

        Some(GpuAtlasRegion {
            page_index: page_index.to_u32(),
            atlas_origin: UVec2::new(x0, y0),
            bitmap_size,
        })
    }

    /// Builds [`GlyphMetrics`] from the GPU dispatch's pre-computed
    /// region and the edge builder's reported bearings and padding.
    #[must_use]
    pub fn metrics_for_gpu_region(
        &self,
        region: GpuAtlasRegion,
        bearing_x: f32,
        bearing_y: f32,
        horizontal_padding_em: f32,
        vertical_padding_em: f32,
    ) -> GlyphMetrics {
        let atlas_width = self.width.to_f32();
        let atlas_height = self.height.to_f32();
        let x0 = region.atlas_origin.x;
        let y0 = region.atlas_origin.y;
        let w = region.bitmap_size.x;
        let h = region.bitmap_size.y;
        let u_min = x0.to_f32() / atlas_width;
        let v_min = y0.to_f32() / atlas_height;
        let u_max = (x0 + w).to_f32() / atlas_width;
        let v_max = (y0 + h).to_f32() / atlas_height;
        GlyphMetrics {
            uv_rect: [u_min, v_min, u_max, v_max],
            bearing_x,
            bearing_y,
            pad_x_em: horizontal_padding_em,
            pad_y_em: vertical_padding_em,
            pixel_width: w,
            pixel_height: h,
            page_index: region.page_index,
        }
    }

    /// Registers a GPU-dispatched glyph as ready.
    ///
    /// Sibling to the CPU-side `insert_bitmap` path: removes `key`
    /// from the in-flight set and records the pre-computed metrics so
    /// future lookups hit the cache. The GPU compute pass has already
    /// written the texels into the page texture; nothing CPU-side
    /// needs to move.
    pub fn insert_completed_gpu(&mut self, key: GlyphKey, metrics: GlyphMetrics) {
        self.in_flight.remove(&key);
        self.glyphs.insert(key, metrics);
    }

    /// Marks an in-flight GPU glyph as invisible (no outline / oversized).
    pub fn insert_completed_gpu_invisible(&mut self, key: GlyphKey) {
        self.in_flight.remove(&key);
        self.glyphs.insert(key, GlyphMetrics::INVISIBLE);
    }

    /// Synchronously rasterizes and inserts a glyph. Used in tests and
    /// startup prepopulation where blocking is acceptable.
    #[cfg(test)]
    pub fn get_or_insert_sync(&mut self, key: GlyphKey, font_data: &[u8]) -> Option<GlyphMetrics> {
        if let Some(metrics) = self.glyphs.get(&key) {
            return Some(*metrics);
        }
        let bitmap = self.rasterizer.rasterize(font_data, key.glyph_index)?;
        self.insert_bitmap(key, &bitmap)
    }

    /// Inserts a rasterized bitmap into the atlas with a texel gutter,
    /// trying existing pages first and allocating a new page if all are full.
    ///
    /// Allocates `bitmap + 2 * ATLAS_GUTTER` per page, copies the bitmap
    /// into the interior, replicates border texels into the gutter, and
    /// computes UV coordinates. The atlas gutter (replicated border
    /// texels) handles edge sampling — no half-texel inset needed.
    ///
    /// Per-pixel layout depends on the [`RasterizedBitmap`] variant:
    /// MSDF writes 3 bytes (R, G, B) and A=255; SDF writes 1 byte into
    /// R, with G=B=0 and A=255 (the shader reads only R in SDF mode).
    fn insert_bitmap(&mut self, key: GlyphKey, bitmap: &RasterizedBitmap) -> Option<GlyphMetrics> {
        let (width, height, bearing_x, bearing_y, horizontal_padding_em, vertical_padding_em) =
            match bitmap {
                RasterizedBitmap::Msdf(b) => (
                    b.width,
                    b.height,
                    b.bearing_x,
                    b.bearing_y,
                    b.pad_x_em,
                    b.pad_y_em,
                ),
                RasterizedBitmap::Sdf(b) => (
                    b.width,
                    b.height,
                    b.bearing_x,
                    b.bearing_y,
                    b.pad_x_em,
                    b.pad_y_em,
                ),
            };
        let g = ATLAS_GUTTER;
        let padded_width = width + 2 * g;
        let padded_height = height + 2 * g;
        let alloc_size = size2(padded_width.to_i32(), padded_height.to_i32());

        // Try each existing page.
        let mut page_idx = None;
        for (i, page) in self.pages.iter_mut().enumerate() {
            if let Some(alloc) = page.allocator.allocate(alloc_size) {
                page_idx = Some((i, alloc));
                break;
            }
        }

        // All pages full — create a new one.
        let (page_index, alloc) = if let Some((i, alloc)) = page_idx {
            (i, alloc)
        } else {
            let mut new_page = AtlasPage::new(self.width, self.height);
            let alloc = new_page.allocator.allocate(alloc_size)?;
            self.pages.push(new_page);
            (self.pages.len() - 1, alloc)
        };

        let page = &mut self.pages[page_index];
        let rect = alloc.rectangle;
        let alloc_x = rect.min.x.to_u32();
        let alloc_y = rect.min.y.to_u32();

        // Interior origin (where the actual bitmap goes, inside the gutter).
        let x0 = alloc_x + g;
        let y0 = alloc_y + g;

        // Copy bitmap data into RGBA page pixels (interior). Each
        // variant decides how its per-pixel bytes map onto the RGBA
        // texel.
        match bitmap {
            RasterizedBitmap::Msdf(b) => {
                for row in 0..b.height {
                    for col in 0..b.width {
                        let src_idx = ((row * b.width + col) * 3).to_usize();
                        let dst_x = x0 + col;
                        let dst_y = y0 + row;
                        let dst_idx = ((dst_y * self.width + dst_x) * BYTES_PER_PIXEL).to_usize();
                        page.pixels[dst_idx] = b.data[src_idx];
                        page.pixels[dst_idx + 1] = b.data[src_idx + 1];
                        page.pixels[dst_idx + 2] = b.data[src_idx + 2];
                        page.pixels[dst_idx + 3] = 255;
                    }
                }
            },
            RasterizedBitmap::Sdf(b) => {
                for row in 0..b.height {
                    for col in 0..b.width {
                        let src_idx = (row * b.width + col).to_usize();
                        let dst_x = x0 + col;
                        let dst_y = y0 + row;
                        let dst_idx = ((dst_y * self.width + dst_x) * BYTES_PER_PIXEL).to_usize();
                        page.pixels[dst_idx] = b.data[src_idx];
                        page.pixels[dst_idx + 1] = 0;
                        page.pixels[dst_idx + 2] = 0;
                        page.pixels[dst_idx + 3] = 255;
                    }
                }
            },
        }

        // Replicate border texels into the gutter so linear filtering
        // at the edge samples continuous distance field values.
        Self::replicate_gutter(&mut page.pixels, self.width, x0, y0, width, height, g);

        let atlas_width = self.width.to_f32();
        let atlas_height = self.height.to_f32();
        let u_min = x0.to_f32() / atlas_width;
        let v_min = y0.to_f32() / atlas_height;
        let u_max = (x0 + width).to_f32() / atlas_width;
        let v_max = (y0 + height).to_f32() / atlas_height;

        let metrics = GlyphMetrics {
            uv_rect:      [u_min, v_min, u_max, v_max],
            bearing_x:    bearing_x.to_f32(),
            bearing_y:    bearing_y.to_f32(),
            pad_x_em:     horizontal_padding_em.to_f32(),
            pad_y_em:     vertical_padding_em.to_f32(),
            pixel_width:  width,
            pixel_height: height,
            page_index:   page_index.to_u32(),
        };

        self.glyphs.insert(key, metrics);
        page.state = AtlasPageState::Dirty;
        Some(metrics)
    }

    /// Replicates the border texels of a bitmap region into the surrounding
    /// gutter. This ensures linear filtering at the edge samples the same
    /// values as the edge itself, preventing bleed from adjacent atlas entries.
    fn replicate_gutter(
        pixels: &mut [u8],
        atlas_width: u32,
        x0: u32,
        y0: u32,
        width: u32,
        height: u32,
        gutter: u32,
    ) {
        // Helper to copy a texel from (sx, sy) to (dx, dy) in the page.
        let copy_texel = |pixels: &mut [u8], sx: u32, sy: u32, dx: u32, dy: u32| {
            let src = ((sy * atlas_width + sx) * BYTES_PER_PIXEL).to_usize();
            let dst = ((dy * atlas_width + dx) * BYTES_PER_PIXEL).to_usize();
            pixels[dst] = pixels[src];
            pixels[dst + 1] = pixels[src + 1];
            pixels[dst + 2] = pixels[src + 2];
            pixels[dst + 3] = pixels[src + 3];
        };

        for g in 1..=gutter {
            // Top and bottom edges.
            for col in 0..width {
                let x = x0 + col;
                // Top gutter: replicate top row upward.
                copy_texel(pixels, x, y0, x, y0 - g);
                // Bottom gutter: replicate bottom row downward.
                copy_texel(pixels, x, y0 + height - 1, x, y0 + height - 1 + g);
            }

            // Left and right edges.
            for row in 0..height {
                let y = y0 + row;
                // Left gutter: replicate left column leftward.
                copy_texel(pixels, x0, y, x0 - g, y);
                // Right gutter: replicate right column rightward.
                copy_texel(pixels, x0 + width - 1, y, x0 + width - 1 + g, y);
            }

            // Corners: replicate the corner texel diagonally.
            // Top-left.
            copy_texel(pixels, x0, y0, x0 - g, y0 - g);
            // Top-right.
            copy_texel(pixels, x0 + width - 1, y0, x0 + width - 1 + g, y0 - g);
            // Bottom-left.
            copy_texel(pixels, x0, y0 + height - 1, x0 - g, y0 + height - 1 + g);
            // Bottom-right.
            copy_texel(
                pixels,
                x0 + width - 1,
                y0 + height - 1,
                x0 + width - 1 + g,
                y0 + height - 1 + g,
            );
        }
    }
}

impl Default for GlyphAtlas {
    /// Returns an empty placeholder atlas — no pages, no glyphs, no
    /// cached rasterizations. Used internally only as the transient
    /// value left behind by `std::mem::take` during a mode swap; the
    /// `AtlasSlot::complete_swap` step overwrites it with `pending`
    /// immediately afterward. No public API surface ever observes this
    /// state.
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let rasterizer: Arc<dyn Rasterizer> = Arc::new(MsdfRasterizer::new(
            DEFAULT_CANONICAL_SIZE,
            DEFAULT_SDF_RANGE,
            DEFAULT_GLYPH_PADDING,
        ));
        let glyph_worker_pool = Arc::new(
            TaskPoolBuilder::new()
                .num_threads(1)
                .thread_name(GLYPH_WORKER_THREAD_NAME.to_string())
                .build(),
        );
        Self {
            pages: Vec::new(),
            glyphs: HashMap::new(),
            width: 0,
            height: 0,
            canonical_size: DEFAULT_CANONICAL_SIZE,
            in_flight: HashSet::new(),
            rx: Mutex::new(rx),
            tx,
            active_jobs: Arc::new(AtomicUsize::new(0)),
            peak_active_jobs: Arc::new(AtomicUsize::new(0)),
            glyph_worker_pool,
            rasterizer,
            backend: RasterBackend::Cpu,
            gpu_dispatcher: None,
            gpu_pipe: None,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::panic,
        clippy::unwrap_used,
        reason = "tests use panic/unwrap for clearer failure messages"
    )]

    use super::*;

    const FONT_DATA: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");

    fn glyph_index(ch: char) -> u16 {
        let face = ttf_parser::Face::parse(FONT_DATA, 0).unwrap_or_else(|e| panic!("parse: {e}"));
        face.glyph_index(ch)
            .unwrap_or_else(|| panic!("no glyph for '{ch}'"))
            .0
    }

    #[test]
    fn glyph_metrics_match_across_msdf_and_sdf() {
        let mut msdf_atlas = GlyphAtlas::with_config(
            DEFAULT_ATLAS_SIZE,
            DEFAULT_CANONICAL_SIZE,
            DEFAULT_GLYPH_WORKER_THREADS,
            DistanceField::Msdf,
            None,
        );
        let mut sdf_atlas = GlyphAtlas::with_config(
            DEFAULT_ATLAS_SIZE,
            DEFAULT_CANONICAL_SIZE,
            DEFAULT_GLYPH_WORKER_THREADS,
            DistanceField::Sdf,
            None,
        );
        for ch in ['A', 'g', 'W', 'O'] {
            let key = GlyphKey {
                font_id:     0,
                glyph_index: glyph_index(ch),
            };
            let m = msdf_atlas
                .get_or_insert_sync(key, FONT_DATA)
                .unwrap_or_else(|| panic!("msdf insert '{ch}'"));
            let s = sdf_atlas
                .get_or_insert_sync(key, FONT_DATA)
                .unwrap_or_else(|| panic!("sdf insert '{ch}'"));
            assert_eq!(
                (m.pixel_width, m.pixel_height),
                (s.pixel_width, s.pixel_height),
                "pixel dims differ for '{ch}'",
            );
            assert_eq!(
                m.bearing_x.to_bits(),
                s.bearing_x.to_bits(),
                "bearing_x differs for '{ch}'"
            );
            assert_eq!(
                m.bearing_y.to_bits(),
                s.bearing_y.to_bits(),
                "bearing_y differs for '{ch}'"
            );
            assert_eq!(
                m.uv_rect.map(f32::to_bits),
                s.uv_rect.map(f32::to_bits),
                "uv_rect differs for '{ch}'"
            );
        }
        assert_eq!(msdf_atlas.distance_field(), DistanceField::Msdf);
        assert_eq!(sdf_atlas.distance_field(), DistanceField::Sdf);
    }

    #[test]
    fn is_ready_reflects_cache_state() {
        let mut atlas = GlyphAtlas::new();
        let key = GlyphKey {
            font_id:     0,
            glyph_index: glyph_index('A'),
        };
        assert!(!atlas.is_ready(key), "uninserted glyph not ready");
        atlas.get_or_insert_sync(key, FONT_DATA);
        assert!(atlas.is_ready(key), "inserted glyph ready");
    }
}
