use std::collections::HashMap;
use std::collections::hash_map::Entry;

use bevy::math::Vec2;

use super::geometry::SlugBounds;
use super::geometry::SlugOutlineError;
use super::geometry::load_glyph_by_id;
use super::packing::SlugPackedGlyph;
use super::packing::build_packed_glyph;

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
    font:     SlugFontKey,
    glyph_id: u16,
}

impl SlugGlyphKey {
    /// Creates a cache key for one glyph in one resolved font face.
    #[must_use]
    pub const fn new(font: SlugFontKey, glyph_id: u16) -> Self { Self { font, glyph_id } }

    /// Resolved font face identity.
    #[must_use]
    pub const fn font(self) -> SlugFontKey { self.font }

    /// Font glyph ID.
    #[must_use]
    pub const fn glyph_id(self) -> u16 { self.glyph_id }
}

/// One positioned glyph in a shaped Slug text run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlugGlyphInstance {
    key:     SlugGlyphKey,
    origin:  Vec2,
    advance: f32,
    bounds:  SlugBounds,
}

impl SlugGlyphInstance {
    /// Creates a positioned glyph instance.
    #[must_use]
    pub const fn new(key: SlugGlyphKey, origin: Vec2, advance: f32, bounds: SlugBounds) -> Self {
        Self {
            key,
            origin,
            advance,
            bounds,
        }
    }

    /// Key for reusable packed glyph data.
    #[must_use]
    pub const fn key(self) -> SlugGlyphKey { self.key }

    /// Glyph origin in shaped-run design-space units.
    #[must_use]
    pub const fn origin(self) -> Vec2 { self.origin }

    /// Shaped advance in design-space units.
    #[must_use]
    pub const fn advance(self) -> f32 { self.advance }

    /// Glyph bounds in font design-space units.
    #[must_use]
    pub const fn bounds(self) -> SlugBounds { self.bounds }
}

/// CPU representation of one shaped Slug text run.
#[derive(Clone, Debug, PartialEq)]
pub struct SlugTextRun {
    glyphs:        Vec<SlugGlyphInstance>,
    bounds:        SlugBounds,
    advance_width: f32,
}

impl SlugTextRun {
    /// Creates a text run from already-shaped glyph instances.
    #[must_use]
    pub fn new(glyphs: Vec<SlugGlyphInstance>) -> Self {
        let advance_width = glyphs
            .iter()
            .map(|glyph| glyph.origin.x + glyph.advance)
            .fold(0.0_f32, f32::max);
        let bounds = run_bounds(&glyphs);
        Self {
            glyphs,
            bounds,
            advance_width,
        }
    }

    /// Ordered glyph instances in shaping order.
    #[must_use]
    pub fn glyphs(&self) -> &[SlugGlyphInstance] { &self.glyphs }

    /// Bounds of all glyph ink in shaped-run design-space units.
    #[must_use]
    pub const fn bounds(&self) -> SlugBounds { self.bounds }

    /// Total shaped advance width in design-space units.
    #[must_use]
    pub const fn advance_width(&self) -> f32 { self.advance_width }
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

    /// Number of unique packed glyphs in the cache.
    #[must_use]
    pub fn len(&self) -> usize { self.glyphs.len() }

    /// Returns whether the cache has no packed glyphs.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.glyphs.is_empty() }

    /// Loads, packs, and caches one glyph if it is not already present.
    pub fn get_or_insert_packed(
        &mut self,
        key: SlugGlyphKey,
        font_data: &[u8],
        character: char,
        band_count: usize,
    ) -> Result<&SlugPackedGlyph, SlugOutlineError> {
        match self.glyphs.entry(key) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let glyph = load_glyph_by_id(font_data, key.glyph_id, character)?;
                Ok(entry.insert(build_packed_glyph(glyph, band_count)))
            },
        }
    }
}

fn run_bounds(glyphs: &[SlugGlyphInstance]) -> SlugBounds {
    let Some(first) = glyphs.first() else {
        return SlugBounds {
            min: Vec2::ZERO,
            max: Vec2::ZERO,
        };
    };
    let first_bounds = shifted_bounds(*first);
    glyphs
        .iter()
        .skip(1)
        .map(|glyph| shifted_bounds(*glyph))
        .fold(first_bounds, merge_bounds)
}

fn shifted_bounds(glyph: SlugGlyphInstance) -> SlugBounds {
    SlugBounds {
        min: glyph.origin + glyph.bounds.min,
        max: glyph.origin + glyph.bounds.max,
    }
}

fn merge_bounds(left: SlugBounds, right: SlugBounds) -> SlugBounds {
    SlugBounds {
        min: Vec2::new(left.min.x.min(right.min.x), left.min.y.min(right.min.y)),
        max: Vec2::new(left.max.x.max(right.max.x), left.max.y.max(right.max.y)),
    }
}
