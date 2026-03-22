//! MSDF glyph atlas — packs rasterized glyphs into a texture.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::mpsc;

use bevy::image::Image;
use bevy::prelude::Assets;
use bevy::prelude::Handle;
use bevy::prelude::Resource;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDimension;
use bevy::render::render_resource::TextureFormat;
use bevy::tasks::AsyncComputeTaskPool;
use etagere::AllocId;
use etagere::AtlasAllocator;
use etagere::size2;

use super::msdf_rasterizer::DEFAULT_CANONICAL_SIZE;
use super::msdf_rasterizer::DEFAULT_GLYPH_PADDING;
use super::msdf_rasterizer::DEFAULT_SDF_RANGE;
use super::msdf_rasterizer::MsdfBitmap;
use super::msdf_rasterizer::rasterize_glyph;

/// Default atlas texture size in pixels.
const DEFAULT_ATLAS_SIZE: u32 = 1024;

/// Number of bytes per pixel (RGBA).
const BYTES_PER_PIXEL: u32 = 4;

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
    /// Atlas allocator ID for potential deallocation.
    _alloc_id:        AllocId,
}

/// Completed async rasterization result.
struct RasterizedGlyph {
    key:    GlyphKey,
    bitmap: MsdfBitmap,
}

/// MSDF glyph atlas.
///
/// Stores an RGBA texture containing MSDF glyph bitmaps and a lookup table
/// mapping [`GlyphKey`] to [`GlyphMetrics`]. The atlas packs glyphs on demand
/// using `etagere`'s shelf-packing algorithm.
///
/// Glyph rasterization is asynchronous — cache misses spawn tasks on
/// [`AsyncComputeTaskPool`] and return `None`. Call
/// [`poll_async_glyphs`](Self::poll_async_glyphs) each frame to insert
/// completed results.
#[derive(Resource)]
pub struct MsdfAtlas {
    /// Raw pixel data (RGBA8, row-major).
    pixels:       Vec<u8>,
    /// Rectangle allocator for packing glyphs.
    allocator:    AtlasAllocator,
    /// Cached glyph metrics, keyed by `GlyphKey`.
    glyphs:       HashMap<GlyphKey, GlyphMetrics>,
    /// Atlas width in pixels.
    width:        u32,
    /// Atlas height in pixels.
    height:       u32,
    /// Handle to the GPU image (populated after `upload_to_gpu`).
    image_handle: Option<Handle<Image>>,
    /// Whether the CPU pixel data has changed since last GPU upload.
    dirty:        bool,
    /// Glyph keys currently being rasterized asynchronously.
    in_flight:    HashSet<GlyphKey>,
    /// Receiver for completed async rasterizations (Mutex for Sync).
    rx:           Mutex<mpsc::Receiver<RasterizedGlyph>>,
    /// Sender cloned into each async task.
    tx:           mpsc::Sender<RasterizedGlyph>,
}

impl MsdfAtlas {
    /// Creates a new empty atlas with the default size.
    #[must_use]
    pub fn new() -> Self { Self::with_size(DEFAULT_ATLAS_SIZE, DEFAULT_ATLAS_SIZE) }

    /// Creates a new empty atlas with a specific size.
    #[must_use]
    #[allow(clippy::cast_possible_wrap)]
    pub fn with_size(width: u32, height: u32) -> Self {
        let pixel_count = (width * height * BYTES_PER_PIXEL) as usize;
        let (tx, rx) = mpsc::channel();
        Self {
            pixels: vec![0; pixel_count],
            allocator: AtlasAllocator::new(size2(width as i32, height as i32)),
            glyphs: HashMap::new(),
            width,
            height,
            image_handle: None,
            dirty: false,
            in_flight: HashSet::new(),
            rx: Mutex::new(rx),
            tx,
        }
    }

    /// Returns the atlas width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 { self.width }

    /// Returns the atlas height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 { self.height }

    /// Returns the raw RGBA pixel data. Test-only.
    #[cfg(test)]
    #[must_use]
    pub fn pixels(&self) -> &[u8] { &self.pixels }

    /// Returns the number of cached glyphs. Test-only.
    #[cfg(test)]
    #[must_use]
    pub fn glyph_count(&self) -> usize { self.glyphs.len() }

    /// Looks up cached metrics for a glyph. Test-only.
    #[cfg(test)]
    #[must_use]
    pub fn get(&self, key: &GlyphKey) -> Option<&GlyphMetrics> { self.glyphs.get(key) }

    /// Returns the SDF range used for glyph generation.
    #[must_use]
    pub const fn sdf_range(&self) -> f64 { DEFAULT_SDF_RANGE }

    /// Returns the GPU image handle.
    ///
    /// Returns `None` if [`upload_to_gpu`](Self::upload_to_gpu) has not been called.
    #[must_use]
    pub const fn image_handle(&self) -> Option<&Handle<Image>> { self.image_handle.as_ref() }

    /// Creates a Bevy `Image` from the atlas pixel data and stores the handle.
    ///
    /// Call once during plugin initialization. Subsequent changes are synced
    /// via [`sync_to_gpu`](Self::sync_to_gpu).
    pub fn upload_to_gpu(&mut self, images: &mut Assets<Image>) {
        let image = Image::new(
            Extent3d {
                width:                 self.width,
                height:                self.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            self.pixels.clone(),
            TextureFormat::Rgba8Unorm,
            bevy::asset::RenderAssetUsages::default(),
        );
        self.image_handle = Some(images.add(image));
        self.dirty = false;
    }

    /// Syncs changed CPU pixel data to the existing GPU `Image` asset.
    ///
    /// Call after new glyphs are inserted. Only copies data when the atlas
    /// is dirty. Bevy's asset change detection handles the actual GPU upload.
    pub fn sync_to_gpu(&mut self, images: &mut Assets<Image>) {
        if !self.dirty {
            return;
        }
        let Some(handle) = self.image_handle.as_ref() else {
            return;
        };
        let Some(image) = images.get_mut(handle) else {
            return;
        };
        image.data = Some(self.pixels.clone());
        self.dirty = false;
    }

    /// Returns cached metrics for a glyph, or `None` if not yet available.
    ///
    /// On cache miss, queues async rasterization on
    /// [`AsyncComputeTaskPool`]. The glyph will be available after
    /// [`poll_async_glyphs`](Self::poll_async_glyphs) picks up the
    /// completed result.
    pub fn get_or_insert(&mut self, key: GlyphKey, font_data: &[u8]) -> Option<GlyphMetrics> {
        if let Some(metrics) = self.glyphs.get(&key) {
            return Some(*metrics);
        }

        // Already queued — don't spawn a duplicate task.
        if self.in_flight.contains(&key) {
            return None;
        }

        // Queue async rasterization.
        self.in_flight.insert(key);
        let tx = self.tx.clone();
        let glyph_index = key.glyph_index;
        let font_data = font_data.to_vec();
        AsyncComputeTaskPool::get()
            .spawn(async move {
                if let Some(bitmap) = rasterize_glyph(
                    &font_data,
                    glyph_index,
                    DEFAULT_CANONICAL_SIZE,
                    DEFAULT_SDF_RANGE,
                    DEFAULT_GLYPH_PADDING,
                ) {
                    let _ = tx.send(RasterizedGlyph { key, bitmap });
                }
            })
            .detach();

        None
    }

    /// Drains completed async rasterizations and inserts them into the atlas.
    ///
    /// Returns `true` if any new glyphs were inserted (callers should
    /// trigger text mesh rebuilds).
    pub fn poll_async_glyphs(&mut self) -> bool {
        let completed: Vec<_> = {
            let rx = self.rx.lock().unwrap_or_else(|e| e.into_inner());
            rx.try_iter().collect()
        };
        let mut any_inserted = false;
        for result in completed {
            self.in_flight.remove(&result.key);
            if self.glyphs.contains_key(&result.key) {
                continue;
            }
            if self.insert_bitmap(result.key, &result.bitmap).is_some() {
                any_inserted = true;
            }
        }
        any_inserted
    }

    /// Synchronously rasterizes and inserts a glyph. Used in tests and
    /// startup prepopulation where blocking is acceptable.
    #[cfg(test)]
    pub fn get_or_insert_sync(&mut self, key: GlyphKey, font_data: &[u8]) -> Option<GlyphMetrics> {
        if let Some(metrics) = self.glyphs.get(&key) {
            return Some(*metrics);
        }
        let bitmap = rasterize_glyph(
            font_data,
            key.glyph_index,
            DEFAULT_CANONICAL_SIZE,
            DEFAULT_SDF_RANGE,
            DEFAULT_GLYPH_PADDING,
        )?;
        self.insert_bitmap(key, &bitmap)
    }

    /// Inserts a rasterized bitmap into the atlas, returns metrics.
    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn insert_bitmap(&mut self, key: GlyphKey, bitmap: &MsdfBitmap) -> Option<GlyphMetrics> {
        let alloc = self
            .allocator
            .allocate(size2(bitmap.width as i32, bitmap.height as i32))?;

        let rect = alloc.rectangle;
        let x0 = rect.min.x as u32;
        let y0 = rect.min.y as u32;

        // Copy RGB bitmap data into RGBA atlas pixels.
        for row in 0..bitmap.height {
            for col in 0..bitmap.width {
                let src_idx = ((row * bitmap.width + col) * 3) as usize;
                let dst_x = x0 + col;
                let dst_y = y0 + row;
                let dst_idx = ((dst_y * self.width + dst_x) * BYTES_PER_PIXEL) as usize;

                self.pixels[dst_idx] = bitmap.data[src_idx];
                self.pixels[dst_idx + 1] = bitmap.data[src_idx + 1];
                self.pixels[dst_idx + 2] = bitmap.data[src_idx + 2];
                self.pixels[dst_idx + 3] = 255; // Alpha = 1.0
            }
        }

        // Compute UV coordinates (normalized 0..1).
        let atlas_w = self.width as f32;
        let atlas_h = self.height as f32;
        let u_min = x0 as f32 / atlas_w;
        let v_min = y0 as f32 / atlas_h;
        let u_max = (x0 + bitmap.width) as f32 / atlas_w;
        let v_max = (y0 + bitmap.height) as f32 / atlas_h;

        #[allow(clippy::cast_possible_truncation)]
        let metrics = GlyphMetrics {
            uv_rect:      [u_min, v_min, u_max, v_max],
            bearing_x:    bitmap.bearing_x as f32,
            bearing_y:    bitmap.bearing_y as f32,
            pixel_width:  bitmap.width,
            pixel_height: bitmap.height,
            _alloc_id:    alloc.id,
        };

        self.glyphs.insert(key, metrics);
        self.dirty = true;
        Some(metrics)
    }
}

impl Default for MsdfAtlas {
    fn default() -> Self { Self::new() }
}
