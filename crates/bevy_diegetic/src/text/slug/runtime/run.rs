use std::collections::HashMap;
use std::collections::hash_map::Entry;

use bevy::math::Vec2;
use bevy_kana::ToU32;

use crate::text::slug::glyph;
use crate::text::slug::glyph::BandRecord;
use crate::text::slug::glyph::Bounds;
use crate::text::slug::glyph::CurveRecord;
use crate::text::slug::glyph::GlyphOutline;
use crate::text::slug::glyph::GlyphRecord;
use crate::text::slug::glyph::OutlineError;

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

/// Cache of reusable packed glyph data, plus the shared glyph atlas every text
/// run indexes into.
///
/// `glyphs` holds each glyph's CPU outline keyed for reuse. The atlas fields
/// (`curves` / `bands` / `glyph_records`) are the append-only GPU tables: the
/// first time a glyph is packed, its records are appended here with global
/// offsets and its slot recorded in `record_indices`, so every run that draws
/// the glyph stores that one global index in its mesh instead of copying the
/// glyph's curves per run. `revision` bumps on every append so the GPU upload
/// knows when the tables grew. The atlas is append-only — glyphs are never
/// evicted.
#[derive(Clone, Debug, Default)]
pub struct GlyphOutlineCache {
    glyphs:         HashMap<GlyphKey, GlyphOutline>,
    record_indices: HashMap<GlyphKey, u32>,
    curves:         Vec<CurveRecord>,
    bands:          Vec<BandRecord>,
    glyph_records:  Vec<GlyphRecord>,
    revision:       u32,
}

impl GlyphOutlineCache {
    /// Global atlas slot for `key`, if the glyph has been packed.
    #[must_use]
    pub fn global_index(&self, key: GlyphKey) -> Option<u32> {
        self.record_indices.get(&key).copied()
    }

    /// Shared curve table for every packed glyph.
    #[must_use]
    pub fn atlas_curves(&self) -> &[CurveRecord] { &self.curves }

    /// Shared band table for every packed glyph.
    #[must_use]
    pub fn atlas_bands(&self) -> &[BandRecord] { &self.bands }

    /// Shared glyph-record table indexed by each run's mesh.
    #[must_use]
    pub fn atlas_glyph_records(&self) -> &[GlyphRecord] { &self.glyph_records }

    /// Append counter; bumps whenever a new glyph grows the atlas tables.
    #[must_use]
    pub const fn atlas_revision(&self) -> u32 { self.revision }

    /// Loads, packs, and caches one glyph from a specific collection face,
    /// appending its records to the shared atlas the first time it is seen.
    pub fn get_or_insert_packed_from_face(
        &mut self,
        key: GlyphKey,
        font_data: &[u8],
        face_index: u32,
        character: char,
        band_count: usize,
    ) -> Result<&GlyphOutline, OutlineError> {
        match self.glyphs.entry(key) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let glyph = glyph::load_glyph_by_id_from_face(
                    font_data,
                    face_index,
                    key.glyph_id,
                    character,
                )?;
                let outline = glyph::build_packed_glyph(glyph, band_count);

                // First sighting of this glyph: append its packed records to the
                // shared atlas with global offsets and record its slot, so runs
                // index in by one number rather than copying curves per run.
                let record_index = self.glyph_records.len().to_u32();
                let curve_start = self.curves.len().to_u32();
                let band_start = self.bands.len().to_u32();
                let axis_band_count = outline.bands().len().to_u32() / 2;
                self.curves.extend_from_slice(outline.curves());
                self.bands
                    .extend(outline.bands().iter().map(|band| BandRecord {
                        start: band.start + curve_start,
                        ..*band
                    }));
                self.glyph_records.push(GlyphRecord::new(
                    outline.bounds(),
                    band_start,
                    axis_band_count,
                    band_start + axis_band_count,
                    axis_band_count,
                ));
                self.record_indices.insert(key, record_index);
                self.revision = self.revision.wrapping_add(1);

                Ok(entry.insert(outline))
            },
        }
    }
}
