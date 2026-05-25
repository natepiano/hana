use std::collections::HashMap;
use std::collections::hash_map::Entry;

use bevy::math::Vec2;

use super::geometry;
use super::geometry::SlugBounds;
use super::geometry::SlugOutlineError;
use super::packing;
use super::packing::SlugPackedGlyph;

/// Stable identity for the resolved font face used by Slug shaping.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SlugFontKey(u64);

impl SlugFontKey {
    /// Creates a font key from a stable caller-owned identifier.
    #[must_use]
    pub const fn new(value: u64) -> Self { Self(value) }

    /// Raw key value.
    #[must_use]
    pub const fn value(self) -> u64 { self.0 }
}

/// Cache key for one resolved font glyph.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SlugGlyphKey {
    font:               SlugFontKey,
    glyph_id:           u16,
    preprocess_version: u32,
}

impl SlugGlyphKey {
    /// Creates a cache key for one preprocessing version.
    #[must_use]
    pub const fn with_preprocess_version(
        font: SlugFontKey,
        glyph_id: u16,
        preprocess_version: u32,
    ) -> Self {
        Self {
            font,
            glyph_id,
            preprocess_version,
        }
    }

    /// Resolved font face identity.
    #[must_use]
    pub const fn font(self) -> SlugFontKey { self.font }

    /// Font glyph ID.
    #[must_use]
    pub const fn glyph_id(self) -> u16 { self.glyph_id }

    /// Slug preprocessing version.
    #[must_use]
    pub const fn preprocess_version(self) -> u32 { self.preprocess_version }
}

/// One positioned glyph in a Slug text run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlugGlyphInstance {
    key:          SlugGlyphKey,
    origin:       Vec2,
    bounds:       SlugBounds,
    bounds_scale: Vec2,
}

impl SlugGlyphInstance {
    /// Creates a positioned glyph instance with non-uniform scaled bounds.
    #[must_use]
    pub const fn new_non_uniform(
        key: SlugGlyphKey,
        origin: Vec2,
        bounds: SlugBounds,
        bounds_scale: Vec2,
    ) -> Self {
        Self {
            key,
            origin,
            bounds,
            bounds_scale,
        }
    }

    /// Key for reusable packed glyph data.
    #[must_use]
    pub const fn key(self) -> SlugGlyphKey { self.key }

    /// Glyph origin in run layout units.
    #[must_use]
    pub const fn origin(self) -> Vec2 { self.origin }

    /// Glyph bounds in font design-space units.
    #[must_use]
    pub const fn bounds(self) -> SlugBounds { self.bounds }

    /// Scale from glyph design-space bounds to run layout units.
    #[must_use]
    pub const fn bounds_scale(self) -> Vec2 { self.bounds_scale }
}

/// CPU representation of one positioned Slug text run.
#[derive(Clone, Debug, PartialEq)]
pub struct SlugTextRun {
    glyphs: Vec<SlugGlyphInstance>,
}

/// Result of positioning and packing one Slug text run.
#[derive(Clone, Debug)]
pub struct SlugBuiltTextRun {
    /// Per-entity positioned text run.
    pub run: SlugTextRun,
}

impl SlugTextRun {
    /// Creates a text run from already-positioned glyph instances.
    #[must_use]
    pub const fn new(glyphs: Vec<SlugGlyphInstance>) -> Self { Self { glyphs } }

    /// Ordered glyph instances in run order.
    #[must_use]
    pub fn glyphs(&self) -> &[SlugGlyphInstance] { &self.glyphs }
}

/// Cache of reusable packed Slug glyph data.
#[derive(Clone, Debug, Default)]
pub struct SlugGlyphCache {
    glyphs: HashMap<SlugGlyphKey, SlugPackedGlyph>,
}

impl SlugGlyphCache {
    /// Returns the cached packed glyph for `key`, if it exists.
    #[must_use]
    pub fn get(&self, key: SlugGlyphKey) -> Option<&SlugPackedGlyph> { self.glyphs.get(&key) }

    /// Loads, packs, and caches one glyph from a specific collection face.
    pub fn get_or_insert_packed_from_face(
        &mut self,
        key: SlugGlyphKey,
        font_data: &[u8],
        face_index: u32,
        character: char,
        band_count: usize,
    ) -> Result<&SlugPackedGlyph, SlugOutlineError> {
        match self.glyphs.entry(key) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let glyph = geometry::load_glyph_by_id_from_face(
                    font_data,
                    face_index,
                    key.glyph_id,
                    character,
                )?;
                Ok(entry.insert(packing::build_packed_glyph(glyph, band_count)))
            },
        }
    }
}
