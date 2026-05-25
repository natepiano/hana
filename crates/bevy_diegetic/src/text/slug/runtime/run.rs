use std::collections::HashMap;
use std::collections::hash_map::Entry;

use bevy::math::Vec2;

use crate::text::slug::glyph;
use crate::text::slug::glyph::Bounds;
use crate::text::slug::glyph::OutlineError;
use crate::text::slug::glyph::PackedGlyph;

/// Stable identity for the resolved font face used by shaping.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FontKey(u64);

impl FontKey {
    /// Creates a font key from a stable caller-owned identifier.
    #[must_use]
    pub const fn new(value: u64) -> Self { Self(value) }

    /// Raw key value.
    #[must_use]
    pub const fn value(self) -> u64 { self.0 }
}

/// Cache key for one resolved font glyph.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GlyphKey {
    font:               FontKey,
    glyph_id:           u16,
    preprocess_version: u32,
}

impl GlyphKey {
    /// Creates a cache key for one preprocessing version.
    #[must_use]
    pub const fn with_preprocess_version(
        font: FontKey,
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
    pub const fn font(self) -> FontKey { self.font }

    /// Font glyph ID.
    #[must_use]
    pub const fn glyph_id(self) -> u16 { self.glyph_id }

    /// Preprocessing version.
    #[must_use]
    pub const fn preprocess_version(self) -> u32 { self.preprocess_version }
}

/// One positioned glyph in a text run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphInstance {
    key:          GlyphKey,
    origin:       Vec2,
    bounds:       Bounds,
    bounds_scale: Vec2,
}

impl GlyphInstance {
    /// Creates a positioned glyph instance with non-uniform scaled bounds.
    #[must_use]
    pub const fn new_non_uniform(
        key: GlyphKey,
        origin: Vec2,
        bounds: Bounds,
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
    pub const fn key(self) -> GlyphKey { self.key }

    /// Glyph origin in run layout units.
    #[must_use]
    pub const fn origin(self) -> Vec2 { self.origin }

    /// Glyph bounds in font design-space units.
    #[must_use]
    pub const fn bounds(self) -> Bounds { self.bounds }

    /// Scale from glyph design-space bounds to run layout units.
    #[must_use]
    pub const fn bounds_scale(self) -> Vec2 { self.bounds_scale }
}

/// CPU representation of one positioned text run.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    glyphs: Vec<GlyphInstance>,
}

/// Result of positioning and packing one text run.
#[derive(Clone, Debug)]
pub struct BuiltTextRun {
    /// Per-entity positioned text run.
    pub run: TextRun,
}

impl TextRun {
    /// Creates a text run from already-positioned glyph instances.
    #[must_use]
    pub const fn new(glyphs: Vec<GlyphInstance>) -> Self { Self { glyphs } }

    /// Ordered glyph instances in run order.
    #[must_use]
    pub fn glyphs(&self) -> &[GlyphInstance] { &self.glyphs }
}

/// Cache of reusable packed glyph data.
#[derive(Clone, Debug, Default)]
pub struct GlyphCache {
    glyphs: HashMap<GlyphKey, PackedGlyph>,
}

impl GlyphCache {
    /// Returns the cached packed glyph for `key`, if it exists.
    #[must_use]
    pub fn get(&self, key: GlyphKey) -> Option<&PackedGlyph> { self.glyphs.get(&key) }

    /// Loads, packs, and caches one glyph from a specific collection face.
    pub fn get_or_insert_packed_from_face(
        &mut self,
        key: GlyphKey,
        font_data: &[u8],
        face_index: u32,
        character: char,
        band_count: usize,
    ) -> Result<&PackedGlyph, OutlineError> {
        match self.glyphs.entry(key) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let glyph = glyph::load_glyph_by_id_from_face(
                    font_data,
                    face_index,
                    key.glyph_id,
                    character,
                )?;
                Ok(entry.insert(glyph::build_packed_glyph(glyph, band_count)))
            },
        }
    }
}
