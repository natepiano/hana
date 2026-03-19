//! MSDF glyph atlas — packs rasterized glyphs into a texture.

use std::collections::HashMap;

use bevy::image::Image;
use bevy::prelude::Assets;
use bevy::prelude::Handle;
use bevy::prelude::Resource;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDimension;
use bevy::render::render_resource::TextureFormat;
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

/// MSDF glyph atlas.
///
/// Stores an RGBA texture containing MSDF glyph bitmaps and a lookup table
/// mapping [`GlyphKey`] to [`GlyphMetrics`]. The atlas packs glyphs on demand
/// using `etagere`'s shelf-packing algorithm.
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
        Self {
            pixels: vec![0; pixel_count],
            allocator: AtlasAllocator::new(size2(width as i32, height as i32)),
            glyphs: HashMap::new(),
            width,
            height,
            image_handle: None,
            dirty: false,
        }
    }

    /// Returns the atlas width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 { self.width }

    /// Returns the atlas height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 { self.height }

    /// Returns the raw RGBA pixel data.
    #[must_use]
    pub fn pixels(&self) -> &[u8] { &self.pixels }

    /// Returns the number of cached glyphs.
    #[must_use]
    pub fn glyph_count(&self) -> usize { self.glyphs.len() }

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
    /// Call once during plugin initialization after prepopulating glyphs.
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

    /// Syncs CPU pixel data to the GPU image if the atlas has changed.
    ///
    /// Call each frame (or when needed) to push new glyphs to the GPU.
    pub fn sync_to_gpu(&mut self, images: &mut Assets<Image>) {
        if !self.dirty {
            return;
        }
        if let Some(handle) = &self.image_handle
            && let Some(image) = images.get_mut(handle)
        {
            image.data = Some(self.pixels.clone());
            self.dirty = false;
        }
    }

    /// Looks up cached metrics for a glyph.
    #[must_use]
    pub fn get(&self, key: &GlyphKey) -> Option<&GlyphMetrics> { self.glyphs.get(key) }

    /// Looks up or rasterizes a glyph, returning its metrics.
    ///
    /// If the glyph is not cached, uses `fdsm` to generate the MSDF bitmap,
    /// packs it into the atlas via `etagere`, and copies the pixel data.
    ///
    /// Returns `None` if the glyph has no outline (e.g., space) or if the
    /// atlas is full.
    pub fn get_or_insert(&mut self, key: GlyphKey, font_data: &[u8]) -> Option<GlyphMetrics> {
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

    /// Pre-populates the atlas with glyphs for the given character set.
    ///
    /// Call during startup for known text (e.g., ASCII) to avoid runtime
    /// rasterization stalls. Glyphs that fail to rasterize (no outline)
    /// are silently skipped.
    pub fn prepopulate(&mut self, font_id: u16, font_data: &[u8], chars: &str) {
        let Some(face) = ttf_parser::Face::parse(font_data, 0).ok() else {
            return;
        };

        for ch in chars.chars() {
            let Some(glyph_id) = face.glyph_index(ch) else {
                continue;
            };

            let key = GlyphKey {
                font_id,
                glyph_index: glyph_id.0,
            };

            // Skip if already cached.
            if self.glyphs.contains_key(&key) {
                continue;
            }

            let Some(bitmap) = rasterize_glyph(
                font_data,
                glyph_id.0,
                DEFAULT_CANONICAL_SIZE,
                DEFAULT_SDF_RANGE,
                DEFAULT_GLYPH_PADDING,
            ) else {
                continue;
            };

            // Stop if atlas is full.
            if self.insert_bitmap(key, &bitmap).is_none() {
                break;
            }
        }
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
