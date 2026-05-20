use bevy::prelude::*;
use bevy_kana::ToF32;
use ttf_parser::GlyphId;

#[cfg(feature = "typography_overlay")]
use super::ComputedGlyphMetrics;
use crate::layout::GlyphLoadingPolicy;
use crate::layout::ShapedGlyph;
use crate::layout::ShapedTextCache;
use crate::layout::Unit;
use crate::layout::WorldTextStyle;
use crate::render::constants;
use crate::render::glyph_quad;
use crate::render::glyph_quad::GlyphQuadData;
use crate::render::text_shaping;
use crate::render::text_shaping::TextBuildStats;
use crate::render::text_shaping::TextShapingContext;
use crate::text::Font;
use crate::text::FontId;
use crate::text::FontRegistry;
use crate::text::GlyphAtlas;
use crate::text::GlyphKey;
use crate::text::GlyphLookup;

/// Result of shaping and building glyph quads for a [`WorldText`](super::WorldText) entity.
pub(super) struct ShapedWorldText {
    /// Per-glyph quads keyed by atlas page index.
    pub(super) quads:    Vec<(u32, GlyphQuadData)>,
    /// `Anchor` offset Y in layout units.
    pub(super) anchor_y: f32,
    /// Per-glyph ink bounding boxes `[x, y, width, height]` in world units.
    #[cfg(feature = "typography_overlay")]
    pub(super) glyphs:   Vec<ComputedGlyphMetrics>,
    /// Timing and queue diagnostics from the build.
    pub(super) stats:    TextBuildStats,
}

impl ShapedWorldText {
    const fn empty(stats: TextBuildStats) -> Self {
        Self {
            quads: Vec::new(),
            anchor_y: 0.0,
            #[cfg(feature = "typography_overlay")]
            glyphs: Vec::new(),
            stats,
        }
    }
}

struct BuiltGlyphQuads {
    quads:  Vec<(u32, GlyphQuadData)>,
    #[cfg(feature = "typography_overlay")]
    glyphs: Vec<ComputedGlyphMetrics>,
}

/// Shapes text and produces glyph quads in entity-local coordinates.
///
/// Unlike panel text, standalone text has no layout bounds or panel scale.
/// Glyphs are positioned relative to the origin, offset by the anchor point.
/// The `scale` parameter converts layout units to world units.
pub(super) fn shape_world_text(
    text: &str,
    style: &WorldTextStyle,
    font_registry: &FontRegistry,
    atlas: &mut GlyphAtlas,
    shaping_cx: &TextShapingContext,
    cache: &mut ShapedTextCache,
    scale: f32,
) -> ShapedWorldText {
    // Pre-scale font size to points for shaping. Parley's quantize mode
    // rounds baselines to integers, which destroys metrics when the font
    // size is below 1.0 (e.g., 0.10 meters). We shape at the equivalent
    // point size and scale the output back down.
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
    let shaped = text_shaping::shape_text_cached(text, &config, font_registry, shaping_cx, cache);
    stats.shape_ms =
        shape_start.elapsed().as_secs_f32() * crate::constants::MILLISECONDS_PER_SECOND;
    stats.glyphs = shaped.glyphs.len();

    let font_data = font_registry
        .font(FontId(style.font_id()))
        .map_or(crate::text::EMBEDDED_FONT, Font::data);

    let atlas_start = std::time::Instant::now();
    if style.loading_policy() == GlyphLoadingPolicy::WhenReady
        && !ensure_all_glyphs_ready(&shaped.glyphs, atlas, font_data, &mut stats)
    {
        stats.atlas_ms =
            atlas_start.elapsed().as_secs_f32() * crate::constants::MILLISECONDS_PER_SECOND;
        return ShapedWorldText::empty(stats);
    }

    let linear: LinearRgba = style.color().into();
    let color_arr = [linear.red, linear.green, linear.blue, linear.alpha];

    // em_scale uses the boosted config size (in points) for atlas lookup,
    // then the final quad positions are multiplied by `scale` (which already
    // accounts for meters_per_unit). The boost cancels out:
    //   quad_world = (glyph_pts * em_scale_pts) * scale_meters
    // where em_scale_pts = config.size() / canonical and scale includes
    // the 1/boost factor to convert back from points to the original unit.
    let em_scale = config.size() / atlas.canonical_size().to_f32();

    // The boosted config is `ForLayout` (no anchor field). Convert to
    // standalone and restore the *original* style's anchor so the offset
    // computation uses the user's intended anchor, not the default Center.
    let (anchor_x, anchor_y) = measure_anchor_offset(
        &shaped.glyphs,
        &config.as_standalone().with_anchor(style.anchor()),
        font_registry,
        atlas,
        font_data,
        em_scale,
    );

    let boosted_size = config.size();
    let world_scale = scale * points_to_world; // points → world meters

    let BuiltGlyphQuads {
        mut quads,
        #[cfg(feature = "typography_overlay")]
        glyphs,
    } = build_glyph_quads(
        &shaped.glyphs,
        atlas,
        font_data,
        boosted_size,
        em_scale,
        anchor_x,
        anchor_y,
        world_scale,
        color_arr,
        &mut stats,
    );

    let padding_world = GlyphAtlas::glyph_padding_texels() * em_scale * world_scale;
    glyph_quad::clip_overlapping_quads(&mut quads, padding_world);

    stats.atlas_ms =
        atlas_start.elapsed().as_secs_f32() * crate::constants::MILLISECONDS_PER_SECOND;
    stats.emitted_quads = quads.len();

    // Anchor values are in boosted (points) space. Scale back to original
    // units for downstream consumers (typography overlay).
    ShapedWorldText {
        quads,
        anchor_y: anchor_y * points_to_world,
        #[cfg(feature = "typography_overlay")]
        glyphs,
        stats,
    }
}

fn build_glyph_quads(
    glyphs: &[ShapedGlyph],
    atlas: &mut GlyphAtlas,
    font_data: &[u8],
    boosted_size: f32,
    em_scale: f32,
    anchor_x: f32,
    anchor_y: f32,
    world_scale: f32,
    color_arr: [f32; 4],
    stats: &mut TextBuildStats,
) -> BuiltGlyphQuads {
    let mut quads = Vec::with_capacity(glyphs.len());
    #[cfg(feature = "typography_overlay")]
    let mut computed_glyphs = Vec::with_capacity(glyphs.len());

    for shaped_glyph in glyphs {
        let glyph_key = GlyphKey {
            font_id:     shaped_glyph.font_face.requested_font_id,
            glyph_index: shaped_glyph.id,
        };

        let metrics = match atlas.lookup_or_queue(glyph_key, font_data) {
            GlyphLookup::Ready(metrics) => {
                stats.ready_glyphs += 1;
                metrics
            },
            GlyphLookup::Pending => {
                stats.pending_glyphs += 1;
                continue;
            },
            GlyphLookup::Queued => {
                stats.queued_glyphs += 1;
                continue;
            },
        };

        let quad_width = metrics.pixel_width.to_f32() * em_scale;
        let quad_height = metrics.pixel_height.to_f32() * em_scale;

        let quad_x =
            (metrics.bearing_x - metrics.pad_x_em).mul_add(boosted_size, shaped_glyph.x) - anchor_x;
        let quad_y = -((metrics.bearing_y + metrics.pad_y_em)
            .mul_add(-boosted_size, shaped_glyph.baseline + shaped_glyph.y)
            - anchor_y);

        quads.push((
            metrics.page_index,
            GlyphQuadData {
                position: [quad_x * world_scale, quad_y * world_scale, 0.0],
                size:     [quad_width * world_scale, quad_height * world_scale],
                uv_rect:  metrics.uv_rect,
                color:    color_arr,
            },
        ));

        #[cfg(feature = "typography_overlay")]
        if let Some(rect) = ink_rect(
            font_data,
            shaped_glyph.id,
            boosted_size,
            shaped_glyph.x,
            shaped_glyph.baseline + shaped_glyph.y,
            Vec2::new(anchor_x, anchor_y),
            world_scale,
        ) {
            let origin_x = (shaped_glyph.x - anchor_x) * world_scale;
            computed_glyphs.push(ComputedGlyphMetrics {
                rect,
                origin_x: origin_x.min(rect[0]),
                origin_y: -(shaped_glyph.baseline + shaped_glyph.y - anchor_y) * world_scale,
                advance_x: shaped_glyph.advance * world_scale,
            });
        }
    }

    BuiltGlyphQuads {
        quads,
        #[cfg(feature = "typography_overlay")]
        glyphs: computed_glyphs,
    }
}

/// Queues all glyphs for async rasterization and returns `true` if every glyph
/// in the run is already cached in the atlas.
fn ensure_all_glyphs_ready(
    glyphs: &[ShapedGlyph],
    atlas: &mut GlyphAtlas,
    font_data: &[u8],
    stats: &mut TextBuildStats,
) -> bool {
    let mut all_ready = true;
    for shaped_glyph in glyphs {
        let glyph_key = GlyphKey {
            font_id:     shaped_glyph.font_face.requested_font_id,
            glyph_index: shaped_glyph.id,
        };
        match atlas.lookup_or_queue(glyph_key, font_data) {
            GlyphLookup::Ready(_) => {},
            GlyphLookup::Pending => {
                stats.pending_glyphs += 1;
                all_ready = false;
            },
            GlyphLookup::Queued => {
                stats.queued_glyphs += 1;
                all_ready = false;
            },
        }
    }
    all_ready
}

/// Measures the total text extent and returns the `(anchor_x, anchor_y)` offset
/// for the given anchor mode.
fn measure_anchor_offset(
    glyphs: &[ShapedGlyph],
    style: &WorldTextStyle,
    font_registry: &FontRegistry,
    atlas: &mut GlyphAtlas,
    font_data: &[u8],
    em_scale: f32,
) -> (f32, f32) {
    let mut max_x = 0.0_f32;
    for shaped_glyph in glyphs {
        let glyph_key = GlyphKey {
            font_id:     shaped_glyph.font_face.requested_font_id,
            glyph_index: shaped_glyph.id,
        };
        if let Some(metrics) = atlas.get_or_insert(glyph_key, font_data) {
            // Anchor measurement uses ink extent (atlas-invariant), not
            // quad extent. Derivation: quad spans bitmap = ink + 2·pad
            // and its left edge sits at (bearing - pad). The quad's
            // right edge is therefore (bearing + ink + pad), one pad
            // beyond the ink. Subtract that single pad to land on the
            // ink's right edge — atlas-invariant.
            let quad_right = metrics.pixel_width.to_f32().mul_add(
                em_scale,
                (metrics.bearing_x - metrics.pad_x_em).mul_add(style.size(), shaped_glyph.x),
            );
            let ink_right = metrics.pad_x_em.mul_add(-style.size(), quad_right);
            max_x = max_x.max(ink_right);
        }
    }
    let mut baselines: Vec<f32> = glyphs.iter().map(|glyph| glyph.baseline).collect();
    baselines
        .dedup_by(|current, next| (*current - *next).abs() < constants::BASELINE_DEDUP_EPSILON);
    let line_count = baselines.len().max(1);
    let natural_line_height = if style.line_height_raw() > 0.0 {
        style.line_height_raw()
    } else {
        font_registry.font(FontId(style.font_id())).map_or_else(
            || style.size(),
            |font| font.metrics(style.size()).line_height,
        )
    };
    let max_y = natural_line_height * line_count.to_f32();
    style.anchor().offset(max_x, max_y)
}

/// Computes the ink bounding box for a single glyph, returned as
/// `[x, y, width, height]`
/// in world units, or `None` if the font face or glyph bbox is unavailable.
#[cfg(feature = "typography_overlay")]
fn ink_rect(
    font_data: &[u8],
    glyph_id: u16,
    font_size: f32,
    glyph_x: f32,
    baseline_offset: f32,
    anchor: Vec2,
    scale: f32,
) -> Option<[f32; 4]> {
    let face = ttf_parser::Face::parse(font_data, 0).ok()?;
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
