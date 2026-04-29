//! Shared text shaping support used by panel and world text rendering.

use std::sync::Mutex;
use std::sync::PoisonError;

use bevy::prelude::*;
use bevy_kana::ToU16;

use crate::layout::LayoutTextStyle;
use crate::layout::LineMetricsSnapshot;
use crate::layout::ShapedGlyph;
use crate::layout::ShapedTextCache;
use crate::layout::ShapedTextRun;
use crate::layout::TextDimensions;
use crate::text::FontId;
use crate::text::FontRegistry;

/// Reusable parley shaping buffers.
///
/// Avoids reallocating `LayoutContext` and `Layout` on every
/// shaping call. Wrapped in `Mutex` for `Send + Sync`.
#[derive(Resource)]
pub(super) struct TextShapingContext {
    layout_cx: Mutex<parley::LayoutContext<()>>,
    layout:    Mutex<parley::Layout<()>>,
}

impl Default for TextShapingContext {
    fn default() -> Self {
        Self {
            layout_cx: Mutex::new(parley::LayoutContext::default()),
            layout:    Mutex::new(parley::Layout::new()),
        }
    }
}

/// Timing and queue diagnostics gathered while building text quads.
#[derive(Clone, Debug, Default)]
pub(super) struct TextBuildStats {
    pub texts:          usize,
    pub glyphs:         usize,
    pub ready_glyphs:   usize,
    pub queued_glyphs:  usize,
    pub pending_glyphs: usize,
    pub emitted_quads:  usize,
    pub shape_ms:       f32,
    pub atlas_ms:       f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GlyphReadiness {
    Idle,
    Pending,
    Ready,
}

impl From<&TextBuildStats> for GlyphReadiness {
    fn from(stats: &TextBuildStats) -> Self {
        if stats.glyphs > 0 && stats.ready_glyphs == stats.glyphs {
            Self::Ready
        } else if stats.pending_glyphs > 0 || stats.queued_glyphs > 0 {
            Self::Pending
        } else {
            Self::Idle
        }
    }
}

impl TextBuildStats {
    pub(super) fn accumulate(&mut self, other: &Self) {
        self.texts += other.texts;
        self.glyphs += other.glyphs;
        self.ready_glyphs += other.ready_glyphs;
        self.queued_glyphs += other.queued_glyphs;
        self.pending_glyphs += other.pending_glyphs;
        self.emitted_quads += other.emitted_quads;
        self.shape_ms += other.shape_ms;
        self.atlas_ms += other.atlas_ms;
    }
}

/// Shapes text via parley, using the cache when possible.
pub(super) fn shape_text_cached(
    text: &str,
    config: &LayoutTextStyle,
    font_registry: &FontRegistry,
    shaping_cx: &TextShapingContext,
    cache: &mut ShapedTextCache,
) -> ShapedTextRun {
    let measure = config.as_measure();

    if let Some(cached) = cache.get_shaped(text, &measure) {
        return cached.clone();
    }

    let font_context = font_registry.font_context();
    let mut font_context = font_context.lock().unwrap_or_else(PoisonError::into_inner);
    let mut layout_context = shaping_cx
        .layout_cx
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let mut layout = shaping_cx
        .layout
        .lock()
        .unwrap_or_else(PoisonError::into_inner);

    let family_name = font_registry
        .family_name(FontId(config.font_id()))
        .unwrap_or("JetBrains Mono");

    let mut builder = layout_context.ranged_builder(&mut font_context, text, 1.0, true);
    builder.push_default(parley::style::StyleProperty::FontSize(config.size()));
    builder.push_default(parley::style::StyleProperty::FontFamily(
        parley::style::FontFamily::named(family_name),
    ));
    if config.line_height_raw() > 0.0 {
        builder.push_default(parley::style::StyleProperty::LineHeight(
            parley::style::LineHeight::Absolute(config.line_height_raw()),
        ));
    }

    let font_features = config.font_features();
    if !font_features.is_default() {
        let parley_features: Vec<parley::style::FontFeature> = font_features
            .to_parley_settings()
            .into_iter()
            .map(|(tag, value)| parley::FontFeature {
                tag: parley::setting::Tag::from_bytes(tag),
                value,
            })
            .collect();
        builder.push_default(parley::style::StyleProperty::FontFeatures(
            parley::style::FontFeatures::List(std::borrow::Cow::Owned(parley_features)),
        ));
    }

    builder.build_into(&mut layout, text);
    layout.break_all_lines(None);

    drop(font_context);
    drop(layout_context);

    let mut glyphs = Vec::new();
    let mut line_metrics = Vec::new();
    for line in layout.lines() {
        let line_metrics_snapshot = line.metrics();
        line_metrics.push(LineMetricsSnapshot {
            ascent:   line_metrics_snapshot.ascent,
            descent:  line_metrics_snapshot.descent,
            baseline: line_metrics_snapshot.baseline,
            top:      line_metrics_snapshot.block_min_coord,
            bottom:   line_metrics_snapshot.block_max_coord,
        });
        for item in line.items() {
            let parley::layout::PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let glyph_run = run.run();
            let mut advance_x = 0.0_f32;
            for cluster in glyph_run.clusters() {
                for glyph in cluster.glyphs() {
                    glyphs.push(ShapedGlyph {
                        glyph_id: glyph.id.to_u16(),
                        x:        run.offset() + advance_x + glyph.x,
                        y:        glyph.y,
                        baseline: run.baseline(),
                    });
                    advance_x += glyph.advance;
                }
            }
        }
    }

    let dimensions = TextDimensions {
        width:       layout.full_width(),
        height:      layout.height(),
        line_height: layout
            .lines()
            .next()
            .map_or_else(|| config.size(), |line| line.metrics().line_height),
    };
    drop(layout);
    let shaped_text_run = ShapedTextRun {
        glyphs,
        line_metrics,
    };
    cache.insert_shaped(text, &measure, shaped_text_run.clone(), dimensions);
    shaped_text_run
}
