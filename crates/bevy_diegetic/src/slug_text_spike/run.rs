use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;

use bevy::math::Vec2;
use bevy_kana::ToU16;
use parley::fontique::Blob;
use parley::fontique::FontInfoOverride;
use parley::layout::PositionedLayoutItem;
use parley::style::FontFamily;
use parley::style::StyleProperty;
use rayon::prelude::*;
use ttf_parser::Face;

use super::backend::SlugTextRequest;
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
    /// Creates a cache key for one glyph in one resolved font face.
    #[must_use]
    pub const fn new(font: SlugFontKey, glyph_id: u16) -> Self {
        Self {
            font,
            glyph_id,
            preprocess_version: 0,
        }
    }

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

/// One positioned glyph in a shaped Slug text run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlugGlyphInstance {
    key:          SlugGlyphKey,
    origin:       Vec2,
    advance:      f32,
    bounds:       SlugBounds,
    bounds_scale: Vec2,
}

impl SlugGlyphInstance {
    /// Creates a positioned glyph instance.
    #[must_use]
    pub const fn new(key: SlugGlyphKey, origin: Vec2, advance: f32, bounds: SlugBounds) -> Self {
        Self::new_scaled(key, origin, advance, bounds, 1.0)
    }

    /// Creates a positioned glyph instance with scaled design-space bounds.
    #[must_use]
    pub const fn new_scaled(
        key: SlugGlyphKey,
        origin: Vec2,
        advance: f32,
        bounds: SlugBounds,
        bounds_scale: f32,
    ) -> Self {
        Self::new_non_uniform(key, origin, advance, bounds, Vec2::splat(bounds_scale))
    }

    /// Creates a positioned glyph instance with non-uniform scaled bounds.
    #[must_use]
    pub const fn new_non_uniform(
        key: SlugGlyphKey,
        origin: Vec2,
        advance: f32,
        bounds: SlugBounds,
        bounds_scale: Vec2,
    ) -> Self {
        Self {
            key,
            origin,
            advance,
            bounds,
            bounds_scale,
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

    /// Scale from glyph design-space bounds to run layout units.
    #[must_use]
    pub const fn bounds_scale(self) -> Vec2 { self.bounds_scale }
}

/// CPU representation of one shaped Slug text run.
#[derive(Clone, Debug, PartialEq)]
pub struct SlugTextRun {
    glyphs:        Vec<SlugGlyphInstance>,
    bounds:        SlugBounds,
    advance_width: f32,
}

/// Result of shaping and packing one Slug text run in the spike path.
#[derive(Clone, Debug)]
pub struct SlugBuiltTextRun {
    /// Per-entity shaped text run.
    pub run:            SlugTextRun,
    /// First-line baseline in font design-space units.
    pub baseline:       f32,
    /// Font size in caller world units.
    pub reference_size: f32,
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

/// Builds one shaped Slug text run and glyph cache using the spike-only
/// shaping path.
pub fn build_slug_text_run(
    text: &str,
    font_data: &[u8],
    font_key: SlugFontKey,
    font_family: &str,
    world_scale: f32,
    band_count: usize,
) -> Result<SlugBuiltTextRun, SlugOutlineError> {
    let request = SlugTextRequest {
        text,
        font_data,
        font_key,
        font_family,
        world_scale,
        band_count,
        preprocess_version: 0,
    };
    let mut glyph_cache = SlugGlyphCache::default();
    build_slug_text_run_with_cache(request, &mut glyph_cache)
}

/// Builds one Slug text run after text shaping using a caller-owned glyph cache.
pub fn build_slug_text_run_with_cache(
    request: SlugTextRequest<'_>,
    glyph_cache: &mut SlugGlyphCache,
) -> Result<SlugBuiltTextRun, SlugOutlineError> {
    let face = Face::parse(request.font_data, 0).map_err(|_| SlugOutlineError::InvalidFont)?;
    let shaped_text = shape_slug_text(
        request.text,
        request.font_data,
        request.font_family,
        request.world_scale,
    )?;
    let mut visible_glyphs = Vec::with_capacity(shaped_text.glyphs.len());
    for glyph in &shaped_text.glyphs {
        let key = SlugGlyphKey::with_preprocess_version(
            request.font_key,
            glyph.glyph_id,
            request.preprocess_version,
        );
        if !geometry::glyph_id_has_visible_outline(&face, glyph.glyph_id) {
            continue;
        }
        visible_glyphs.push(VisibleSlugGlyph {
            key,
            character: glyph.character,
            origin: glyph.origin,
            advance: glyph.advance,
        });
    }

    glyph_cache.insert_missing_packed_parallel(
        &visible_glyphs,
        request.font_data,
        request.band_count,
    )?;

    let mut glyphs = Vec::with_capacity(visible_glyphs.len());
    for glyph in visible_glyphs {
        let packed_glyph = glyph_cache
            .get(glyph.key)
            .ok_or_else(|| SlugOutlineError::MissingGlyphId(glyph.key.glyph_id()))?;
        glyphs.push(SlugGlyphInstance::new(
            glyph.key,
            glyph.origin,
            glyph.advance,
            packed_glyph.bounds(),
        ));
    }
    Ok(SlugBuiltTextRun {
        run:            SlugTextRun::new(glyphs),
        baseline:       shaped_text.baseline,
        reference_size: shaped_text.reference_size,
    })
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

    fn insert_missing_packed_parallel(
        &mut self,
        glyphs: &[VisibleSlugGlyph],
        font_data: &[u8],
        band_count: usize,
    ) -> Result<(), SlugOutlineError> {
        let missing = unique_missing_glyphs(glyphs, &self.glyphs);
        let packed = missing
            .par_iter()
            .map(|glyph| {
                let outline =
                    geometry::load_glyph_by_id(font_data, glyph.key.glyph_id, glyph.character)?;
                Ok((*glyph, packing::build_packed_glyph(outline, band_count)))
            })
            .collect::<Result<Vec<_>, SlugOutlineError>>()?;

        for (glyph, packed_glyph) in packed {
            self.glyphs.entry(glyph.key).or_insert(packed_glyph);
        }
        Ok(())
    }

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
                let glyph = geometry::load_glyph_by_id(font_data, key.glyph_id, character)?;
                Ok(entry.insert(packing::build_packed_glyph(glyph, band_count)))
            },
        }
    }

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

#[derive(Clone, Copy, Debug)]
struct VisibleSlugGlyph {
    key:       SlugGlyphKey,
    character: char,
    origin:    Vec2,
    advance:   f32,
}

fn unique_missing_glyphs(
    glyphs: &[VisibleSlugGlyph],
    cache: &HashMap<SlugGlyphKey, SlugPackedGlyph>,
) -> Vec<VisibleSlugGlyph> {
    let mut seen = HashSet::new();
    let mut missing = Vec::new();
    for glyph in glyphs {
        if cache.contains_key(&glyph.key) || !seen.insert(glyph.key) {
            continue;
        }
        missing.push(*glyph);
    }
    missing
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
        min: glyph.origin + glyph.bounds.min * glyph.bounds_scale,
        max: glyph.origin + glyph.bounds.max * glyph.bounds_scale,
    }
}

const fn merge_bounds(left: SlugBounds, right: SlugBounds) -> SlugBounds {
    SlugBounds {
        min: Vec2::new(left.min.x.min(right.min.x), left.min.y.min(right.min.y)),
        max: Vec2::new(left.max.x.max(right.max.x), left.max.y.max(right.max.y)),
    }
}

#[derive(Clone, Copy, Debug)]
struct ShapedSlugGlyph {
    character: char,
    glyph_id:  u16,
    origin:    Vec2,
    advance:   f32,
}

#[derive(Clone, Debug)]
struct ShapedSlugText {
    glyphs:         Vec<ShapedSlugGlyph>,
    baseline:       f32,
    reference_size: f32,
}

fn shape_slug_text(
    text: &str,
    font_data: &[u8],
    font_family: &str,
    world_scale: f32,
) -> Result<ShapedSlugText, SlugOutlineError> {
    let face = Face::parse(font_data, 0).map_err(|_| SlugOutlineError::InvalidFont)?;
    reject_missing_exact_font_glyphs(&face, text)?;
    let shape_size = f32::from(face.units_per_em());

    let mut font_context = parley::FontContext::default();
    font_context.collection.register_fonts(
        Blob::from(font_data.to_vec()),
        Some(FontInfoOverride {
            family_name: Some(font_family),
            ..Default::default()
        }),
    );
    let mut layout_context = parley::LayoutContext::<()>::default();
    let mut layout = parley::Layout::<()>::new();

    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(shape_size));
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(font_family)));
    builder.build_into(&mut layout, text);
    layout.break_all_lines(None);

    let mut characters = text.chars();
    let mut shaped_glyphs = Vec::new();
    let mut baseline = 0.0;
    for line in layout.lines() {
        baseline = line.metrics().baseline;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let mut advance_x = 0.0_f32;
            for cluster in run.run().clusters() {
                for glyph in cluster.glyphs() {
                    let Some(character) = characters.next() else {
                        continue;
                    };
                    shaped_glyphs.push(ShapedSlugGlyph {
                        character,
                        glyph_id: glyph.id.to_u16(),
                        origin: Vec2::new(run.offset() + advance_x + glyph.x, glyph.y),
                        advance: glyph.advance,
                    });
                    advance_x += glyph.advance;
                }
            }
        }
    }
    Ok(ShapedSlugText {
        glyphs: shaped_glyphs,
        baseline,
        reference_size: shape_size * world_scale,
    })
}

fn reject_missing_exact_font_glyphs(face: &Face<'_>, text: &str) -> Result<(), SlugOutlineError> {
    for character in text.chars() {
        if face.glyph_index(character).is_none() {
            return Err(SlugOutlineError::MissingGlyph(character));
        }
    }
    Ok(())
}
