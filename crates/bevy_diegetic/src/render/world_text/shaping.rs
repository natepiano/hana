use bevy::prelude::*;
use bevy_kana::ToF32;
use ttf_parser::GlyphId;

#[cfg(feature = "typography_overlay")]
use super::ComputedGlyphMetrics;
use crate::layout::ShapedTextCache;
use crate::layout::ShapedTextRun;
use crate::layout::Unit;
use crate::layout::WorldTextStyle;
use crate::render::constants;
use crate::render::text_shaping;
use crate::render::text_shaping::TextBuildStats;
use crate::render::text_shaping::TextShapingContext;
use crate::text::DEFAULT_BAND_COUNT;
use crate::text::FontRegistry;
use crate::text::GlyphCache;
use crate::text::PositionedGlyph;
use crate::text::PreparedTextRun;

/// Result of building text run data for a [`WorldText`](super::WorldText) entity.
pub(super) struct ShapedWorldTextRun {
    /// Prepared text run.
    pub(super) prepared: Option<PreparedTextRun>,
    /// `Anchor` offset Y in layout units.
    pub(super) anchor_y: f32,
    /// Per-glyph ink bounding boxes `[x, y, width, height]` in world units.
    #[cfg(feature = "typography_overlay")]
    pub(super) glyphs:   Vec<ComputedGlyphMetrics>,
    /// Timing and queue diagnostics from the build.
    pub(super) stats:    TextBuildStats,
}

impl ShapedWorldTextRun {
    const fn empty(stats: TextBuildStats) -> Self {
        Self {
            prepared: None,
            anchor_y: 0.0,
            #[cfg(feature = "typography_overlay")]
            glyphs: Vec::new(),
            stats,
        }
    }
}

/// Builds text run data in entity-local coordinates after text shaping.
pub(super) fn build_world_text_run(
    text: &str,
    style: &WorldTextStyle,
    font_registry: &FontRegistry,
    backend: &mut GlyphCache,
    shaping_cx: &TextShapingContext,
    cache: &mut ShapedTextCache,
    scale: f32,
) -> ShapedWorldTextRun {
    let points_to_world = Unit::Points.meters_per_unit();
    let boost = if points_to_world > 0.0 {
        1.0 / points_to_world
    } else {
        1.0
    };
    let config = style.as_layout_config().scaled(boost);

    let mut stats = TextBuildStats {
        texts: 1,
        ..Default::default()
    };
    let shape_start = std::time::Instant::now();
    let layout_run =
        text_shaping::shape_text_cached(text, &config, font_registry, shaping_cx, cache);
    stats.shape_ms =
        shape_start.elapsed().as_secs_f32() * crate::constants::MILLISECONDS_PER_SECOND;
    stats.glyphs = layout_run.glyphs.len();
    let positioned_glyphs =
        text_shaping::positioned_glyphs(&layout_run.glyphs, font_registry, &mut stats);

    if stats.failed_glyphs > 0 {
        return ShapedWorldTextRun::empty(stats);
    }

    let boosted_size = config.size();
    let (anchor_x, anchor_y) = measure_anchor_offset(
        &layout_run,
        &positioned_glyphs,
        &config.as_standalone().with_anchor(style.anchor()),
        boosted_size,
    );
    let world_scale = scale * points_to_world;

    let run_start = std::time::Instant::now();
    let prepared = match backend.prepare_positioned_run(
        &positioned_glyphs,
        Vec2::new(anchor_x, anchor_y),
        boosted_size,
        world_scale,
        DEFAULT_BAND_COUNT,
    ) {
        Ok(prepared) => prepared,
        Err(err) => {
            bevy::log::warn!("world text unsupported: {err}");
            stats.failed_glyphs += positioned_glyphs.len().max(1);
            stats.atlas_ms =
                run_start.elapsed().as_secs_f32() * crate::constants::MILLISECONDS_PER_SECOND;
            return ShapedWorldTextRun::empty(stats);
        },
    };

    stats.ready_glyphs = positioned_glyphs.len();
    stats.emitted_quads = prepared.glyph_count();
    stats.atlas_ms = run_start.elapsed().as_secs_f32() * crate::constants::MILLISECONDS_PER_SECOND;

    #[cfg(feature = "typography_overlay")]
    let glyphs = overlay_glyph_metrics(
        &positioned_glyphs,
        boosted_size,
        Vec2::new(anchor_x, anchor_y),
        world_scale,
    );

    ShapedWorldTextRun {
        prepared: Some(prepared),
        anchor_y: anchor_y * points_to_world,
        #[cfg(feature = "typography_overlay")]
        glyphs,
        stats,
    }
}

fn measure_anchor_offset(
    layout_run: &ShapedTextRun,
    positioned_glyphs: &[PositionedGlyph<'_>],
    style: &WorldTextStyle,
    font_size: f32,
) -> (f32, f32) {
    let mut max_x = 0.0_f32;
    for positioned_glyph in positioned_glyphs {
        let layout_glyph = positioned_glyph.glyph;
        if let Some(ink_right) = native_ink_right(positioned_glyph, font_size) {
            max_x = max_x.max(layout_glyph.x + ink_right);
        }
    }
    let max_y = if style.line_height_raw() > 0.0 {
        let mut baselines: Vec<f32> = layout_run
            .glyphs
            .iter()
            .map(|glyph| glyph.baseline)
            .collect();
        baselines
            .dedup_by(|current, next| (*current - *next).abs() < constants::BASELINE_DEDUP_EPSILON);
        style.line_height_raw() * baselines.len().max(1).to_f32()
    } else {
        layout_run
            .line_metrics
            .iter()
            .map(|line| line.bottom)
            .reduce(f32::max)
            .unwrap_or_else(|| style.size())
    };
    style.anchor().offset(max_x, max_y)
}

fn native_ink_right(positioned_glyph: &PositionedGlyph<'_>, font_size: f32) -> Option<f32> {
    let face = ttf_parser::Face::parse(
        positioned_glyph.font.data(),
        positioned_glyph.collection_index,
    )
    .ok()?;
    let bbox = face.glyph_bounding_box(GlyphId(positioned_glyph.glyph.id))?;
    let upm = f32::from(face.units_per_em());
    Some(f32::from(bbox.x_max) * font_size / upm)
}

#[cfg(feature = "typography_overlay")]
fn overlay_glyph_metrics(
    positioned_glyphs: &[PositionedGlyph<'_>],
    font_size: f32,
    anchor: Vec2,
    scale: f32,
) -> Vec<ComputedGlyphMetrics> {
    let mut glyphs = Vec::with_capacity(positioned_glyphs.len());
    for positioned_glyph in positioned_glyphs {
        let shaped_glyph = positioned_glyph.glyph;
        if let Some(rect) = ink_rect(
            positioned_glyph.font.data(),
            positioned_glyph.collection_index,
            shaped_glyph.id,
            font_size,
            shaped_glyph.x,
            shaped_glyph.baseline + shaped_glyph.y,
            anchor,
            scale,
        ) {
            let origin_x = (shaped_glyph.x - anchor.x) * scale;
            glyphs.push(ComputedGlyphMetrics {
                rect,
                origin_x: origin_x.min(rect[0]),
                origin_y: -(shaped_glyph.baseline + shaped_glyph.y - anchor.y) * scale,
                advance_x: shaped_glyph.advance * scale,
            });
        }
    }
    glyphs
}

/// Computes the ink bounding box for a single glyph, returned as
/// `[x, y, width, height]`
/// in world units, or `None` if the font face or glyph bbox is unavailable.
#[cfg(feature = "typography_overlay")]
fn ink_rect(
    font_data: &[u8],
    collection_index: u32,
    glyph_id: u16,
    font_size: f32,
    glyph_x: f32,
    baseline_offset: f32,
    anchor: Vec2,
    scale: f32,
) -> Option<[f32; 4]> {
    let face = ttf_parser::Face::parse(font_data, collection_index).ok()?;
    let bbox = face.glyph_bounding_box(GlyphId(glyph_id))?;
    let upm = f32::from(face.units_per_em());
    let font_scale = font_size / upm;

    let ink_width = f32::from(bbox.x_max - bbox.x_min) * font_scale;
    let ink_height = f32::from(bbox.y_max - bbox.y_min) * font_scale;
    let ink_x = f32::from(bbox.x_min).mul_add(font_scale, glyph_x) - anchor.x;
    let ink_top = f32::from(bbox.y_max).mul_add(-font_scale, baseline_offset) - anchor.y;

    Some([
        ink_x * scale,
        -ink_top * scale,
        ink_width * scale,
        ink_height * scale,
    ])
}
