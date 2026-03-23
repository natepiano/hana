//! MSDF glyph atlas — packs rasterized glyphs into paged textures.

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

/// Default atlas page texture size in pixels.
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
    /// Atlas page this glyph is stored on.
    pub page_index:   u32,
    /// Atlas allocator ID for potential deallocation.
    _alloc_id:        AllocId,
}

/// Completed async rasterization result.
struct RasterizedGlyph {
    key:    GlyphKey,
    bitmap: MsdfBitmap,
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
    #[allow(clippy::cast_possible_wrap)]
    fn new(width: u32, height: u32) -> Self {
        let pixel_count = (width * height * BYTES_PER_PIXEL) as usize;
        Self {
            pixels:       vec![0; pixel_count],
            allocator:    AtlasAllocator::new(size2(width as i32, height as i32)),
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
    pages:          Vec<AtlasPage>,
    /// Cached glyph metrics, keyed by `GlyphKey`.
    glyphs:         HashMap<GlyphKey, GlyphMetrics>,
    /// Page width in pixels (all pages share the same dimensions).
    width:          u32,
    /// Page height in pixels.
    height:         u32,
    /// Canonical pixel size for MSDF rasterization.
    canonical_size: u32,
    /// Glyph keys currently being rasterized asynchronously.
    in_flight:      HashSet<GlyphKey>,
    /// Receiver for completed async rasterizations (Mutex for Sync).
    rx:             Mutex<mpsc::Receiver<RasterizedGlyph>>,
    /// Sender cloned into each async task.
    tx:             mpsc::Sender<RasterizedGlyph>,
}

impl MsdfAtlas {
    /// Creates a new atlas with the default page size and canonical size.
    #[must_use]
    pub fn new() -> Self { Self::with_config(DEFAULT_ATLAS_SIZE, DEFAULT_CANONICAL_SIZE) }

    /// Creates a new atlas with a specific page size (uses default canonical size).
    #[must_use]
    pub fn with_size(width: u32, height: u32) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            pages: vec![AtlasPage::new(width, height)],
            glyphs: HashMap::new(),
            width,
            height,
            canonical_size: DEFAULT_CANONICAL_SIZE,
            in_flight: HashSet::new(),
            rx: Mutex::new(rx),
            tx,
        }
    }

    /// Creates a new atlas with a specific page size and canonical rasterization size.
    #[must_use]
    pub fn with_config(page_size: u32, canonical_size: u32) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            pages: vec![AtlasPage::new(page_size, page_size)],
            glyphs: HashMap::new(),
            width: page_size,
            height: page_size,
            canonical_size,
            in_flight: HashSet::new(),
            rx: Mutex::new(rx),
            tx,
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

    /// Looks up cached metrics for a glyph without triggering rasterization.
    ///
    /// Returns `None` if the glyph hasn't been rasterized yet.
    #[must_use]
    pub fn get_metrics(&self, key: GlyphKey) -> Option<GlyphMetrics> {
        self.glyphs.get(&key).copied()
    }

    /// Returns the raw RGBA pixel data for a page.
    #[must_use]
    pub fn page_pixels(&self, page: usize) -> Option<&[u8]> {
        self.pages.get(page).map(|p| p.pixels.as_slice())
    }

    /// Scans a glyph's atlas region using bilinear-filtered UV sampling to
    /// find the tight bounding box of visible pixels. This replicates the
    /// GPU's `textureSample` (bilinear) behavior so the result matches the
    /// fragment shader exactly.
    ///
    /// Uses the same edge-scanning algorithm as the compute shader: scan
    /// inward from each edge at half-texel steps, check `compute_alpha`
    /// at each sample point. Returns the tight UV bounds directly.
    ///
    /// `screen_px_range` controls the SDF-to-alpha conversion — higher
    /// values produce sharper edges (and tighter bounds).
    ///
    /// Returns `(ink_uv_min, ink_uv_max)` in atlas UV coordinates, or
    /// `None` if the glyph is not cached or has no visible pixels.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::similar_names
    )]
    pub fn scan_ink_bounds_uv(
        &self,
        key: GlyphKey,
        screen_px_range: f32,
    ) -> Option<([f32; 2], [f32; 2])> {
        const THRESHOLD: f32 = 0.02;

        let metrics = self.glyphs.get(&key)?;
        let page = self.pages.get(metrics.page_index as usize)?;
        let pixels = &page.pixels;

        let atlas_w = self.width as f32;
        let atlas_h = self.height as f32;

        let [u_start, v_start, u_end, v_end] = metrics.uv_rect;

        // Half-texel step size — same as the compute shader.
        let du = 0.5 / atlas_w;
        let dv = 0.5 / atlas_h;

        // Integer step counts to avoid float-comparison while conditions.
        let cols = ((u_end - u_start) / du) as u32 + 1;
        let rows = ((v_end - v_start) / dv) as u32 + 1;

        // Bilinear sample at a UV coordinate, returning the median SDF alpha.
        let sample_alpha = |sample_u: f32, sample_v: f32| -> f32 {
            let med = bilinear_sample_median(pixels, self.width, self.height, sample_u, sample_v);
            let sd = med - 0.5;
            screen_px_range.mul_add(sd, 0.5).clamp(0.0, 1.0)
        };

        // Left edge: scan columns left-to-right.
        let mut ink_u_min = u_end;
        'left: for col in 0..cols {
            let su = du.mul_add(col as f32, u_start);
            for row in 0..rows {
                let sv = dv.mul_add(row as f32, v_start);
                if sample_alpha(su, sv) >= THRESHOLD {
                    ink_u_min = su;
                    break 'left;
                }
            }
        }

        // Right edge: scan columns right-to-left.
        let mut ink_u_max = u_start;
        'right: for col in (0..cols).rev() {
            let su = du.mul_add(col as f32, u_start);
            for row in 0..rows {
                let sv = dv.mul_add(row as f32, v_start);
                if sample_alpha(su, sv) >= THRESHOLD {
                    ink_u_max = su;
                    break 'right;
                }
            }
        }

        // Top edge: scan rows top-to-bottom.
        let mut ink_v_min = v_end;
        'top: for row in 0..rows {
            let sv = dv.mul_add(row as f32, v_start);
            for col in 0..cols {
                let su = du.mul_add(col as f32, u_start);
                if sample_alpha(su, sv) >= THRESHOLD {
                    ink_v_min = sv;
                    break 'top;
                }
            }
        }

        // Bottom edge: scan rows bottom-to-top.
        let mut ink_v_max = v_start;
        'bottom: for row in (0..rows).rev() {
            let sv = dv.mul_add(row as f32, v_start);
            for col in 0..cols {
                let su = du.mul_add(col as f32, u_start);
                if sample_alpha(su, sv) >= THRESHOLD {
                    ink_v_max = sv;
                    break 'bottom;
                }
            }
        }

        // No visible pixels found.
        if ink_u_max < ink_u_min {
            return None;
        }

        // Adjust max edges — the scan finds the last visible sample center;
        // the visible edge extends ~0.75 steps past it.
        ink_u_max += du * 0.75;
        ink_v_max += dv * 0.75;

        Some(([ink_u_min, ink_v_min], [ink_u_max, ink_v_max]))
    }

    /// Looks up cached metrics for a glyph. Test-only.
    #[cfg(test)]
    #[must_use]
    pub fn get(&self, key: GlyphKey) -> Option<&GlyphMetrics> { self.glyphs.get(&key) }

    /// Returns the SDF range used for glyph generation.
    #[must_use]
    #[allow(clippy::unused_self)]
    pub const fn sdf_range(&self) -> f64 { DEFAULT_SDF_RANGE }

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
        let canonical = self.canonical_size;
        AsyncComputeTaskPool::get()
            .spawn(async move {
                if let Some(bitmap) = rasterize_glyph(
                    &font_data,
                    glyph_index,
                    canonical,
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
    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn insert_bitmap(&mut self, key: GlyphKey, bitmap: &MsdfBitmap) -> Option<GlyphMetrics> {
        let g = ATLAS_GUTTER;
        let padded_w = bitmap.width + 2 * g;
        let padded_h = bitmap.height + 2 * g;
        let alloc_size = size2(padded_w as i32, padded_h as i32);

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
        let alloc_x = rect.min.x as u32;
        let alloc_y = rect.min.y as u32;

        // Interior origin (where the actual bitmap goes, inside the gutter).
        let x0 = alloc_x + g;
        let y0 = alloc_y + g;

        // Copy RGB bitmap data into RGBA page pixels (interior).
        for row in 0..bitmap.height {
            for col in 0..bitmap.width {
                let src_idx = ((row * bitmap.width + col) * 3) as usize;
                let dst_x = x0 + col;
                let dst_y = y0 + row;
                let dst_idx = ((dst_y * self.width + dst_x) * BYTES_PER_PIXEL) as usize;

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
            page_index:   page_index as u32,
            _alloc_id:    alloc.id,
        };

        self.glyphs.insert(key, metrics);
        page.dirty = true;
        Some(metrics)
    }

    /// Replicates the border texels of a bitmap region into the surrounding
    /// gutter. This ensures linear filtering at the edge samples the same
    /// values as the edge itself, preventing bleed from adjacent atlas entries.
    #[allow(clippy::cast_sign_loss)]
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

/// Bilinear-filtered median sample at a UV coordinate in the atlas.
///
/// Replicates the GPU's `textureSample` / `textureSampleLevel` behavior:
/// maps UV to fractional pixel coordinates, fetches the four surrounding
/// texels, and bilinearly interpolates the median-of-three SDF value.
///
/// Returns the interpolated median in `[0.0, 1.0]`.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
fn bilinear_sample_median(
    pixels: &[u8],
    atlas_width: u32,
    atlas_height: u32,
    tex_u: f32,
    tex_v: f32,
) -> f32 {
    let width_f = atlas_width as f32;
    let height_f = atlas_height as f32;

    // Map UV to texel-center coordinates (GPU convention: UV 0.0 maps to
    // the left edge of texel 0, so texel center 0 is at UV 0.5/w).
    let tx = tex_u.mul_add(width_f, -0.5);
    let ty = tex_v.mul_add(height_f, -0.5);

    let x0 = tx.floor() as i32;
    let y0 = ty.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;

    let frac_x = tx - tx.floor();
    let frac_y = ty - ty.floor();

    let max_x = atlas_width as i32 - 1;
    let max_y = atlas_height as i32 - 1;
    let read_median = |px: i32, py: i32| -> f32 {
        let cx = px.clamp(0, max_x) as u32;
        let cy = py.clamp(0, max_y) as u32;
        let idx = ((cy * atlas_width + cx) * BYTES_PER_PIXEL) as usize;
        let red = f32::from(pixels[idx]) / 255.0;
        let green = f32::from(pixels[idx + 1]) / 255.0;
        let blue = f32::from(pixels[idx + 2]) / 255.0;
        // median(r, g, b) = max(min(r, g), min(max(r, g), b))
        red.max(green).min(blue).max(red.min(green))
    };

    let m00 = read_median(x0, y0);
    let m10 = read_median(x1, y0);
    let m01 = read_median(x0, y1);
    let m11 = read_median(x1, y1);

    // Bilinear interpolation.
    let top = (1.0 - frac_x).mul_add(m00, frac_x * m10);
    let bot = (1.0 - frac_x).mul_add(m01, frac_x * m11);
    (1.0 - frac_y).mul_add(top, frac_y * bot)
}
