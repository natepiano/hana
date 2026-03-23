//! MSDF glyph atlas — packs rasterized glyphs into a texture.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::PoisonError;
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
    pub fn get(&self, key: GlyphKey) -> Option<&GlyphMetrics> { self.glyphs.get(&key) }

    /// Returns the SDF range used for glyph generation.
    #[must_use]
    #[allow(clippy::unused_self)]
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
            let rx = self.rx.lock().unwrap_or_else(PoisonError::into_inner);
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

    /// Inserts a rasterized bitmap into the atlas with a texel gutter.
    ///
    /// Allocates `bitmap + 2 * ATLAS_GUTTER` in the atlas, copies the bitmap
    /// into the interior, replicates border texels into the gutter, and
    /// computes UV coordinates inset by half a texel so linear filtering
    /// samples texel centers rather than edges.
    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn insert_bitmap(&mut self, key: GlyphKey, bitmap: &MsdfBitmap) -> Option<GlyphMetrics> {
        let g = ATLAS_GUTTER;
        let padded_w = bitmap.width + 2 * g;
        let padded_h = bitmap.height + 2 * g;

        let alloc = self
            .allocator
            .allocate(size2(padded_w as i32, padded_h as i32))?;

        let rect = alloc.rectangle;
        let alloc_x = rect.min.x as u32;
        let alloc_y = rect.min.y as u32;

        // Interior origin (where the actual bitmap goes).
        let x0 = alloc_x + g;
        let y0 = alloc_y + g;

        // Copy RGB bitmap data into RGBA atlas pixels (interior).
        for row in 0..bitmap.height {
            for col in 0..bitmap.width {
                let src_idx = ((row * bitmap.width + col) * 3) as usize;
                let dst_x = x0 + col;
                let dst_y = y0 + row;
                let dst_idx = ((dst_y * self.width + dst_x) * BYTES_PER_PIXEL) as usize;

                self.pixels[dst_idx] = bitmap.data[src_idx];
                self.pixels[dst_idx + 1] = bitmap.data[src_idx + 1];
                self.pixels[dst_idx + 2] = bitmap.data[src_idx + 2];
                self.pixels[dst_idx + 3] = 255;
            }
        }

        // Replicate border texels into the gutter so linear filtering
        // at the edge samples continuous distance field values.
        self.replicate_gutter(x0, y0, bitmap.width, bitmap.height, g);

        // Compute UV coordinates inset by half a texel so the sampler
        // hits texel centers, not the border between texels.
        let atlas_w = self.width as f32;
        let atlas_h = self.height as f32;
        let half_texel_u = 0.5 / atlas_w;
        let half_texel_v = 0.5 / atlas_h;
        let u_min = x0 as f32 / atlas_w + half_texel_u;
        let v_min = y0 as f32 / atlas_h + half_texel_v;
        let u_max = (x0 + bitmap.width) as f32 / atlas_w - half_texel_u;
        let v_max = (y0 + bitmap.height) as f32 / atlas_h - half_texel_v;

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

    /// Replicates the border texels of a bitmap region into the surrounding
    /// gutter. This ensures linear filtering at the edge samples the same
    /// values as the edge itself, preventing bleed from adjacent atlas entries.
    #[allow(clippy::cast_sign_loss)]
    fn replicate_gutter(
        &mut self,
        x0: u32,
        y0: u32,
        width: u32,
        height: u32,
        gutter: u32,
    ) {
        // Helper to copy a texel from (sx, sy) to (dx, dy) in the atlas.
        let w = self.width;
        let copy_texel = |pixels: &mut [u8], sx: u32, sy: u32, dx: u32, dy: u32| {
            let src = ((sy * w + sx) * BYTES_PER_PIXEL) as usize;
            let dst = ((dy * w + dx) * BYTES_PER_PIXEL) as usize;
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
                copy_texel(&mut self.pixels, x, y0, x, y0 - g);
                // Bottom gutter: replicate bottom row downward.
                copy_texel(
                    &mut self.pixels,
                    x,
                    y0 + height - 1,
                    x,
                    y0 + height - 1 + g,
                );
            }

            // Left and right edges.
            for row in 0..height {
                let y = y0 + row;
                // Left gutter: replicate left column leftward.
                copy_texel(&mut self.pixels, x0, y, x0 - g, y);
                // Right gutter: replicate right column rightward.
                copy_texel(
                    &mut self.pixels,
                    x0 + width - 1,
                    y,
                    x0 + width - 1 + g,
                    y,
                );
            }

            // Corners: replicate the corner texel diagonally.
            // Top-left.
            copy_texel(&mut self.pixels, x0, y0, x0 - g, y0 - g);
            // Top-right.
            copy_texel(
                &mut self.pixels,
                x0 + width - 1,
                y0,
                x0 + width - 1 + g,
                y0 - g,
            );
            // Bottom-left.
            copy_texel(
                &mut self.pixels,
                x0,
                y0 + height - 1,
                x0 - g,
                y0 + height - 1 + g,
            );
            // Bottom-right.
            copy_texel(
                &mut self.pixels,
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
