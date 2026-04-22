//! Shared text-shaping cache keyed by `(text, TextMeasure)`.
//!
//! Stores two parallel maps:
//! - **measurements** — `TextDimensions` used by the layout engine's `MeasureTextFn`. This is the
//!   only half the headless layout path needs.
//! - **entries** — shaped glyph runs used by the renderer to avoid repeating parley shaping after
//!   measurement has already happened.
//!
//! The cache lives in `layout/` because its key and value types (`TextMeasure`,
//! `TextDimensions`) are layout-domain types and the measurement half is
//! load-bearing for headless panel layout. The renderer populates both halves
//! whenever it shapes text, so measurement lookups hit when shaping has
//! already run.

use std::collections::HashMap;
use std::hash::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;

use bevy::prelude::Resource;
use bevy_kana::ToI32;
use bevy_kana::ToU32;

use super::font_features::FontFeatures;
use super::text_props::FontSlant;
use super::text_props::TextDimensions;
use super::text_props::TextMeasure;

/// A single shaped glyph from parley — glyph ID plus its position relative
/// to the text origin.
#[derive(Clone, Debug)]
pub struct ShapedGlyph {
    /// Glyph index within the font.
    pub glyph_id: u16,
    /// X position relative to the text origin (accumulated advance + fine
    /// offset).
    pub x:        f32,
    /// Y position relative to the text origin (baseline-relative).
    pub y:        f32,
    /// Baseline of the line this glyph belongs to.
    pub baseline: f32,
}

/// Snapshot of parley's per-line metrics, captured during text shaping.
///
/// All values are in layout units (Y-down coordinate system).
#[derive(Clone, Copy, Debug)]
pub struct LineMetricsSnapshot {
    /// Typographic ascent for this line.
    pub ascent:   f32,
    /// Typographic descent for this line.
    pub descent:  f32,
    /// Offset to the baseline from the top of the layout.
    pub baseline: f32,
    /// Top of the line box (parley `block_min_coord`).
    pub top:      f32,
    /// Bottom of the line box (parley `block_max_coord`).
    pub bottom:   f32,
}

/// Cached shaping result for a text string at a specific font configuration.
#[derive(Clone, Debug)]
pub struct ShapedTextRun {
    /// The shaped glyphs in order.
    pub glyphs:       Vec<ShapedGlyph>,
    /// Per-line metrics from parley, captured during shaping.
    pub line_metrics: Vec<LineMetricsSnapshot>,
}

/// Cache key: hash of the text string + the full `TextMeasure` identity.
#[derive(Clone, Eq, PartialEq, Hash)]
struct ShapedCacheKey {
    text_hash:                u64,
    font_id:                  u16,
    /// Size quantized to avoid floating-point hash issues (size * 100 as u32).
    size_quantized:           u32,
    weight_quantized:         u32,
    slant:                    u8,
    line_height_quantized:    u32,
    letter_spacing_quantized: i32,
    word_spacing_quantized:   i32,
    font_features:            FontFeatures,
}

impl ShapedCacheKey {
    fn new(text: &str, m: &TextMeasure) -> Self {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        Self {
            text_hash:                hasher.finish(),
            font_id:                  m.font_id,
            size_quantized:           (m.size * 100.0).to_u32(),
            weight_quantized:         (m.weight.0 * 10.0).to_u32(),
            slant:                    match m.slant {
                FontSlant::Normal => 0,
                FontSlant::Italic => 1,
                FontSlant::Oblique => 2,
            },
            line_height_quantized:    (m.line_height * 100.0).to_u32(),
            letter_spacing_quantized: (m.letter_spacing * 100.0).to_i32(),
            word_spacing_quantized:   (m.word_spacing * 100.0).to_i32(),
            font_features:            m.font_features,
        }
    }
}

/// Caches shaped text runs and measurement results to avoid redundant parley
/// shaping.
///
/// Shared between the layout engine's `MeasureTextFn` (measurement half) and
/// the renderer's text shaper (run + measurement halves) via
/// `Arc<Mutex<>>`.
#[derive(Resource, Clone, Default)]
pub struct ShapedTextCache {
    entries:      HashMap<ShapedCacheKey, ShapedTextRun>,
    measurements: HashMap<ShapedCacheKey, TextDimensions>,
}

impl ShapedTextCache {
    /// Returns cached measurement dimensions for the given text + config,
    /// or `None` if not yet cached.
    #[must_use]
    pub fn get_measurement(&self, text: &str, measure: &TextMeasure) -> Option<TextDimensions> {
        let key = ShapedCacheKey::new(text, measure);
        self.measurements.get(&key).copied()
    }

    /// Returns the cached shaped text run for the given text + config,
    /// or `None` if not yet cached.
    #[must_use]
    pub fn get_shaped(&self, text: &str, measure: &TextMeasure) -> Option<&ShapedTextRun> {
        let key = ShapedCacheKey::new(text, measure);
        self.entries.get(&key)
    }

    /// Inserts a measurement result into the cache.
    pub fn insert_measurement(&mut self, text: &str, measure: &TextMeasure, dims: TextDimensions) {
        let key = ShapedCacheKey::new(text, measure);
        self.measurements.insert(key, dims);
    }

    /// Inserts a shaped run alongside its measurement. Used by the renderer
    /// after parley shaping so subsequent layout-engine lookups hit.
    pub fn insert_shaped(
        &mut self,
        text: &str,
        measure: &TextMeasure,
        run: ShapedTextRun,
        dims: TextDimensions,
    ) {
        let key = ShapedCacheKey::new(text, measure);
        self.measurements.insert(key.clone(), dims);
        self.entries.insert(key, run);
    }
}
