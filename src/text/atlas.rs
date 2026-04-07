//! MSDF glyph atlas — packs rasterized glyphs into paged textures.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Instant;

use bevy::image::Image;
use bevy::prelude::Assets;
use bevy::prelude::Handle;
use bevy::prelude::Resource;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDimension;
use bevy::render::render_resource::TextureFormat;
use bevy::tasks::TaskPool;
use bevy::tasks::TaskPoolBuilder;
use bevy_kana::ToF32;
use bevy_kana::ToI32;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;
use etagere::AtlasAllocator;
use etagere::size2;

use super::font::Font;
use super::font_registry::FontId;
use super::font_registry::FontRegistry;
use super::msdf_rasterizer;
use super::msdf_rasterizer::DEFAULT_CANONICAL_SIZE;
use super::msdf_rasterizer::DEFAULT_GLYPH_PADDING;
use super::msdf_rasterizer::DEFAULT_SDF_RANGE;
use super::msdf_rasterizer::MsdfBitmap;

/// Default atlas page texture size in pixels.
const DEFAULT_ATLAS_SIZE: u32 = 1024;

/// Default number of worker threads used by the atlas when no override is provided.
const DEFAULT_GLYPH_WORKER_THREADS: usize = 6;

/// Number of bytes per pixel (RGBA).
const BYTES_PER_PIXEL: u32 = 4;

/// Texel gutter around each glyph in the atlas.
///
/// Prevents linear filtering from sampling into adjacent glyph regions,
/// which causes the MSDF median-of-three decode to produce faint vertical
/// line artifacts at glyph boundaries. Border texels are replicated into
/// the gutter so the distance field is continuous at the edge, and UV
/// coordinates are inset by half a texel so the sampler hits texel centers.
///
/// This is one of two fixes for MSDF seam artifacts — see the module docs
/// in [`glyph_quad`](crate::render::glyph_quad) for the full explanation.
/// The other fix is [`clip_overlapping_quads`](crate::render::glyph_quad::clip_overlapping_quads)
/// which handles overlapping geometry.
const ATLAS_GUTTER: u32 = 1;

/// Key for looking up a cached glyph in the atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    /// Font identifier from [`FontId`](crate::FontId).
    pub font_id:     u16,
    /// Glyph index within the font.
    pub glyph_index: u16,
}

/// Metrics for a single glyph stored in the atlas.
#[derive(Clone, Copy, Debug)]
pub struct GlyphMetrics {
    /// UV rectangle in the atlas texture: `[u_min, v_min, u_max, v_max]`.
    pub uv_rect:      [f32; 4],
    /// Horizontal bearing offset in em units (glyph origin to bitmap left).
    pub bearing_x:    f32,
    /// Vertical bearing offset in em units (glyph origin to bitmap top).
    pub bearing_y:    f32,
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
    const INVISIBLE: Self = Self {
        uv_rect:      [0.0; 4],
        bearing_x:    0.0,
        bearing_y:    0.0,
        pixel_width:  0,
        pixel_height: 0,
        page_index:   0,
    };
}

/// Completed async rasterization result.
struct RasterizedGlyph {
    key:               GlyphKey,
    /// `None` for glyphs with no visual representation (e.g. space).
    bitmap:            Option<MsdfBitmap>,
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

/// A single page of the MSDF atlas texture.
struct AtlasPage {
    /// Raw pixel data (RGBA8, row-major).
    pixels:       Vec<u8>,
    /// Rectangle allocator for packing glyphs.
    allocator:    AtlasAllocator,
    /// Handle to the GPU image (populated after upload).
    image_handle: Option<Handle<Image>>,
    /// Whether the CPU pixel data has changed since last GPU sync.
    dirty:        bool,
}

impl AtlasPage {
    fn new(width: u32, height: u32) -> Self {
        let pixel_count = (width * height * BYTES_PER_PIXEL).to_usize();
        Self {
            pixels:       vec![0; pixel_count],
            allocator:    AtlasAllocator::new(size2(width.to_i32(), height.to_i32())),
            image_handle: None,
            dirty:        false,
        }
    }
}

/// MSDF glyph atlas with automatic page overflow.
///
/// Stores RGBA textures containing MSDF glyph bitmaps and a lookup table
/// mapping [`GlyphKey`] to [`GlyphMetrics`]. Glyphs are packed into pages
/// using `etagere`'s shelf-packing algorithm. When a page fills, a new
/// page is allocated automatically.
///
/// Glyph rasterization is asynchronous — cache misses spawn tasks on
/// [`AsyncComputeTaskPool`] and return `None`. Call
/// [`poll_async_glyphs`](Self::poll_async_glyphs) each frame to insert
/// completed results.
#[derive(Resource)]
pub struct MsdfAtlas {
    /// Atlas pages, each with its own pixel buffer and allocator.
    pages:             Vec<AtlasPage>,
    /// Cached glyph metrics, keyed by `GlyphKey`.
    glyphs:            HashMap<GlyphKey, GlyphMetrics>,
    /// Page width in pixels (all pages share the same dimensions).
    width:             u32,
    /// Page height in pixels.
    height:            u32,
    /// Canonical pixel size for MSDF rasterization.
    canonical_size:    u32,
    /// Glyph keys currently being rasterized asynchronously.
    in_flight:         HashSet<GlyphKey>,
    /// Receiver for completed async rasterizations (Mutex for Sync).
    rx:                Mutex<mpsc::Receiver<RasterizedGlyph>>,
    /// Sender cloned into each async task.
    tx:                mpsc::Sender<RasterizedGlyph>,
    /// Number of raster jobs currently executing on worker threads.
    active_jobs:       Arc<AtomicUsize>,
    /// Peak concurrently executing raster jobs observed so far.
    peak_active_jobs:  Arc<AtomicUsize>,
    /// Dedicated worker pool for async glyph rasterization.
    glyph_worker_pool: Arc<TaskPool>,
}

impl MsdfAtlas {
    /// Creates a new atlas with the default page size and canonical size.
    #[must_use]
    pub fn new() -> Self {
        Self::new_with_dimensions(
            DEFAULT_ATLAS_SIZE,
            DEFAULT_ATLAS_SIZE,
            DEFAULT_CANONICAL_SIZE,
            DEFAULT_GLYPH_WORKER_THREADS,
        )
    }

    /// Creates a new atlas with a specific page size (uses default canonical size).
    #[must_use]
    pub fn with_size(width: u32, height: u32) -> Self {
        Self::new_with_dimensions(
            width,
            height,
            DEFAULT_CANONICAL_SIZE,
            DEFAULT_GLYPH_WORKER_THREADS,
        )
    }

    /// Creates a new atlas with a specific page size and canonical rasterization size.
    #[must_use]
    pub fn with_config(page_size: u32, canonical_size: u32, glyph_worker_threads: usize) -> Self {
        Self::new_with_dimensions(page_size, page_size, canonical_size, glyph_worker_threads)
    }

    fn new_with_dimensions(
        width: u32,
        height: u32,
        canonical_size: u32,
        glyph_worker_threads: usize,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
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
            glyph_worker_pool: Arc::new(
                TaskPoolBuilder::new()
                    .num_threads(glyph_worker_threads)
                    .thread_name("Diegetic Glyph Raster".to_string())
                    .build(),
            ),
        }
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
    pub fn dirty_page_count(&self) -> usize { self.pages.iter().filter(|p| p.dirty).count() }

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
    /// necessary for tooling that needs to inspect shaped glyph storage without
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

    /// Returns the GPU image handle for a specific atlas page.
    ///
    /// Returns `None` if the page doesn't exist or hasn't been uploaded yet.
    #[must_use]
    pub fn image_handle(&self, page: u32) -> Option<&Handle<Image>> {
        self.pages
            .get(page as usize)
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
            let image = Image::new(
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
            page.image_handle = Some(images.add(image));
            page.dirty = false;
        }
    }

    /// Syncs changed CPU pixel data to the existing GPU `Image` assets.
    ///
    /// Call after new glyphs are inserted. Handles both dirty existing pages
    /// and newly created pages (allocates GPU Images for them). Bevy's asset
    /// change detection handles the actual GPU upload.
    pub fn sync_to_gpu(&mut self, images: &mut Assets<Image>) {
        for page in &mut self.pages {
            if !page.dirty {
                continue;
            }
            if let Some(handle) = page.image_handle.as_ref() {
                // Existing page — update pixel data.
                if let Some(image) = images.get_mut(handle) {
                    image.data = Some(page.pixels.clone());
                }
            } else {
                // New page created at runtime — allocate GPU Image.
                let image = Image::new(
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
                page.image_handle = Some(images.add(image));
            }
            page.dirty = false;
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

        // Queue async rasterization.
        self.in_flight.insert(key);
        let tx = self.tx.clone();
        let active_jobs = Arc::clone(&self.active_jobs);
        let peak_active_jobs = Arc::clone(&self.peak_active_jobs);
        let glyph_index = key.glyph_index;
        let font_data = font_data.to_vec();
        let canonical = self.canonical_size;
        self.glyph_worker_pool
            .spawn(async move {
                let active_now = active_jobs.fetch_add(1, Ordering::Relaxed) + 1;
                peak_active_jobs.fetch_max(active_now, Ordering::Relaxed);
                let start = Instant::now();
                let bitmap = msdf_rasterizer::rasterize_glyph(
                    &font_data,
                    glyph_index,
                    canonical,
                    DEFAULT_SDF_RANGE,
                    DEFAULT_GLYPH_PADDING,
                );
                let elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;
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
    ///                    mut atlas: ResMut<MsdfAtlas>,
    ///                    registry: Res<FontRegistry>| {
    ///     atlas.preload("ABCDEF...", trigger.id, &registry);
    /// });
    /// ```
    pub fn preload(&mut self, text: &str, font_id: FontId, registry: &FontRegistry) {
        let Some(font_data) = registry.font(font_id).map(Font::data) else {
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
        stats
    }

    /// Synchronously rasterizes and inserts a glyph. Used in tests and
    /// startup prepopulation where blocking is acceptable.
    #[cfg(test)]
    pub fn get_or_insert_sync(&mut self, key: GlyphKey, font_data: &[u8]) -> Option<GlyphMetrics> {
        if let Some(metrics) = self.glyphs.get(&key) {
            return Some(*metrics);
        }
        let bitmap = msdf_rasterizer::rasterize_glyph(
            font_data,
            key.glyph_index,
            self.canonical_size,
            DEFAULT_SDF_RANGE,
            DEFAULT_GLYPH_PADDING,
        )?;
        self.insert_bitmap(key, &bitmap)
    }

    /// Inserts a rasterized bitmap into the atlas with a texel gutter,
    /// trying existing pages first and allocating a new page if all are full.
    ///
    /// Allocates `bitmap + 2 * ATLAS_GUTTER` per page, copies the bitmap
    /// into the interior, replicates border texels into the gutter, and
    /// computes UV coordinates inset by half a texel so linear filtering
    /// samples texel centers rather than edges.
    fn insert_bitmap(&mut self, key: GlyphKey, bitmap: &MsdfBitmap) -> Option<GlyphMetrics> {
        let g = ATLAS_GUTTER;
        let padded_w = bitmap.width + 2 * g;
        let padded_h = bitmap.height + 2 * g;
        let alloc_size = size2(padded_w.to_i32(), padded_h.to_i32());

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

        // Copy RGB bitmap data into RGBA page pixels (interior).
        for row in 0..bitmap.height {
            for col in 0..bitmap.width {
                let src_idx = ((row * bitmap.width + col) * 3).to_usize();
                let dst_x = x0 + col;
                let dst_y = y0 + row;
                let dst_idx = ((dst_y * self.width + dst_x) * BYTES_PER_PIXEL).to_usize();

                page.pixels[dst_idx] = bitmap.data[src_idx];
                page.pixels[dst_idx + 1] = bitmap.data[src_idx + 1];
                page.pixels[dst_idx + 2] = bitmap.data[src_idx + 2];
                page.pixels[dst_idx + 3] = 255;
            }
        }

        // Replicate border texels into the gutter so linear filtering
        // at the edge samples continuous distance field values.
        Self::replicate_gutter(
            &mut page.pixels,
            self.width,
            x0,
            y0,
            bitmap.width,
            bitmap.height,
            g,
        );

        // UV coordinates map the full bitmap extent. The atlas gutter
        // (replicated border texels) handles edge sampling — no half-texel
        // inset needed. Insetting would stretch the MSDF across the quad
        // and push the visible contour beyond the font metrics.
        let atlas_w = self.width.to_f32();
        let atlas_h = self.height.to_f32();
        let u_min = x0.to_f32() / atlas_w;
        let v_min = y0.to_f32() / atlas_h;
        let u_max = (x0 + bitmap.width).to_f32() / atlas_w;
        let v_max = (y0 + bitmap.height).to_f32() / atlas_h;

        let metrics = GlyphMetrics {
            uv_rect:      [u_min, v_min, u_max, v_max],
            bearing_x:    bitmap.bearing_x.to_f32(),
            bearing_y:    bitmap.bearing_y.to_f32(),
            pixel_width:  bitmap.width,
            pixel_height: bitmap.height,
            page_index:   page_index.to_u32(),
        };

        self.glyphs.insert(key, metrics);
        page.dirty = true;
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
        let w = atlas_width;
        let copy_texel = |pixels: &mut [u8], sx: u32, sy: u32, dx: u32, dy: u32| {
            let src = ((sy * w + sx) * BYTES_PER_PIXEL).to_usize();
            let dst = ((dy * w + dx) * BYTES_PER_PIXEL).to_usize();
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

impl Default for MsdfAtlas {
    fn default() -> Self { Self::new() }
}
