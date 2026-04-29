use std::collections::HashSet;
use std::time::Instant;

use bevy::prelude::*;
use bevy_kana::ToF32;

use super::batching;
use super::batching::PanelTextAlpha;
use super::batching::PanelTextQuads;
use crate::cascade::CascadeDefaults;
use crate::cascade::CascadePanelChild;
use crate::cascade::Resolved;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::BoundingBox;
use crate::layout::GlyphLoadingPolicy;
use crate::layout::LayoutTextStyle;
use crate::layout::ShapedGlyph;
use crate::layout::ShapedTextCache;
use crate::layout::WorldTextStyle;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
use crate::render::constants::TEXT_Z_OFFSET;
use crate::render::glyph_quad;
use crate::render::glyph_quad::GlyphQuadData;
use crate::render::text_shaping;
use crate::render::text_shaping::GlyphReadiness;
use crate::render::text_shaping::TextBuildStats;
use crate::render::text_shaping::TextShapingContext;
use crate::render::world_text::AwaitingReady;
use crate::render::world_text::PanelTextChild;
use crate::render::world_text::PendingGlyphs;
use crate::render::world_text::WorldText;
use crate::text::Font;
use crate::text::FontId;
use crate::text::FontRegistry;
use crate::text::GlyphKey;
use crate::text::GlyphLookup;
use crate::text::MsdfAtlas;

/// Shapes text for panel [`WorldText`] children that are changed or pending.
pub(super) fn shape_panel_text_children(
    changed_texts: Query<
        Entity,
        (
            With<PanelTextChild>,
            With<WorldText>,
            Or<(
                Changed<WorldText>,
                Changed<WorldTextStyle>,
                Changed<PanelTextChild>,
                Changed<Resolved<PanelTextAlpha>>,
            )>,
        ),
    >,
    pending_texts: Query<Entity, (With<PanelTextChild>, With<WorldText>, With<PendingGlyphs>)>,
    texts: Query<(&WorldText, &WorldTextStyle, &PanelTextChild, &ChildOf)>,
    panel_alpha: Query<&Resolved<PanelTextAlpha>, With<DiegeticPanel>>,
    defaults: Res<CascadeDefaults>,
    mut atlas: ResMut<MsdfAtlas>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut perf: ResMut<DiegeticPerfStats>,
    mut commands: Commands,
) {
    let shape_stage_start = Instant::now();
    let mut aggregate = TextBuildStats::default();
    let mut shaped_panels: HashSet<Entity> = HashSet::new();

    let mut to_process = Vec::new();
    for entity in &changed_texts {
        to_process.push(entity);
    }
    for entity in &pending_texts {
        if !to_process.contains(&entity) {
            to_process.push(entity);
        }
    }

    if to_process.is_empty() {
        perf.panel_text.shape_ms = 0.0;
        perf.panel_text.parley_ms = 0.0;
        perf.panel_text.atlas_lookup_ms = 0.0;
        perf.panel_text.shaped_panels = 0;
        perf.panel_text.queued_glyphs = 0;
        perf.panel_text.pending_glyphs = 0;
        perf.panel_text.total_ms = perf.panel_text.mesh_build_ms;
        return;
    }

    for entity in to_process {
        let Ok((world_text, style, panel_text_child, child_of)) = texts.get(entity) else {
            continue;
        };

        if world_text.0.is_empty() {
            commands
                .entity(entity)
                .remove::<PendingGlyphs>()
                .remove::<PanelTextQuads>();
            continue;
        }

        let config = style.as_layout_config();
        let placement = QuadPlacement {
            bounds:    panel_text_child.bounds,
            scale:     Vec2::new(panel_text_child.scale_x, panel_text_child.scale_y),
            anchor:    Vec2::new(panel_text_child.anchor_x, panel_text_child.anchor_y),
            clip_rect: panel_text_child.clip_rect,
        };
        let mut services = TextQuadServices {
            font_registry: &font_registry,
            atlas:         &mut atlas,
            shaping_cx:    &shaping_cx,
            cache:         &mut cache,
        };
        let (quads, stats) = shape_text_to_quads(&world_text.0, &config, &placement, &mut services);

        aggregate.accumulate(&stats);
        shaped_panels.insert(child_of.parent());

        match GlyphReadiness::from(&stats) {
            GlyphReadiness::Ready => {
                let panel_text_quads = PanelTextQuads {
                    quads,
                    render_mode: config.render_mode(),
                    shadow_mode: config.shadow_mode(),
                    alpha_mode: config.alpha_mode(),
                };
                let panel_fallback = panel_alpha.get(child_of.parent()).map_or_else(
                    |_| PanelTextAlpha::global_default(&defaults),
                    |resolved| resolved.0,
                );
                let resolved =
                    PanelTextAlpha::entity_value(&panel_text_quads).unwrap_or(panel_fallback);
                commands
                    .entity(entity)
                    .insert((panel_text_quads, Resolved(resolved)));
                commands.entity(entity).remove::<PendingGlyphs>();
                commands.entity(entity).insert(AwaitingReady);
            },
            GlyphReadiness::Pending => {
                commands.entity(entity).insert_if_new(PendingGlyphs);
            },
            GlyphReadiness::Idle => {},
        }
    }

    perf.panel_text.shape_ms = shape_stage_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.panel_text.parley_ms = aggregate.shape_ms;
    perf.panel_text.atlas_lookup_ms = aggregate.atlas_ms;
    perf.panel_text.shaped_panels = shaped_panels.len();
    perf.panel_text.queued_glyphs = aggregate.queued_glyphs;
    perf.panel_text.pending_glyphs = aggregate.pending_glyphs;
    perf.panel_text.total_ms = perf.panel_text.shape_ms + perf.panel_text.mesh_build_ms;
}

/// Shared text-shaping resources threaded through quad construction.
struct TextQuadServices<'a> {
    font_registry: &'a FontRegistry,
    atlas:         &'a mut MsdfAtlas,
    shaping_cx:    &'a TextShapingContext,
    cache:         &'a mut ShapedTextCache,
}

/// Placement parameters that position shaped glyphs into panel-local space.
struct QuadPlacement {
    bounds:    BoundingBox,
    scale:     Vec2,
    anchor:    Vec2,
    clip_rect: Option<BoundingBox>,
}

/// Shapes text and produces glyph quads in panel-local coordinates.
fn shape_text_to_quads(
    text: &str,
    config: &LayoutTextStyle,
    placement: &QuadPlacement,
    services: &mut TextQuadServices<'_>,
) -> (Vec<(u32, GlyphQuadData)>, TextBuildStats) {
    let TextQuadServices {
        font_registry,
        atlas,
        shaping_cx,
        cache,
    } = services;
    let &QuadPlacement {
        bounds,
        scale,
        anchor,
        clip_rect,
    } = placement;
    let mut stats = TextBuildStats {
        texts: 1,
        ..Default::default()
    };
    let shape_start = Instant::now();
    let shaped = text_shaping::shape_text_cached(text, config, font_registry, shaping_cx, cache);
    stats.shape_ms = shape_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    stats.glyphs = shaped.glyphs.len();

    let font_data = font_registry
        .font(FontId(config.font_id()))
        .map_or(crate::text::EMBEDDED_FONT, Font::data);

    let atlas_start = Instant::now();
    if !all_glyphs_ready_when_required(config, &shaped.glyphs, font_data, atlas, &mut stats) {
        stats.atlas_ms = atlas_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
        return (Vec::new(), stats);
    }

    let linear: LinearRgba = config.color().into();
    let color = [linear.red, linear.green, linear.blue, linear.alpha];
    let em_scale = config.size() / atlas.canonical_size().to_f32();

    let mut quads = Vec::with_capacity(shaped.glyphs.len());
    for shaped_glyph in &shaped.glyphs {
        let glyph_key = GlyphKey {
            font_id:     config.font_id(),
            glyph_index: shaped_glyph.glyph_id,
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

        let glyph_x = bounds.x + shaped_glyph.x;
        let glyph_y = bounds.y + shaped_glyph.baseline + shaped_glyph.y;
        let quad_w = metrics.pixel_width.to_f32() * em_scale;
        let quad_h = metrics.pixel_height.to_f32() * em_scale;
        let quad_layout_x = metrics.bearing_x.mul_add(config.size(), glyph_x);
        let quad_layout_y = (-metrics.bearing_y).mul_add(config.size(), glyph_y);
        let local_x = quad_layout_x.mul_add(scale.x, -anchor.x);
        let local_y = (-quad_layout_y).mul_add(scale.y, anchor.y);

        quads.push((
            metrics.page_index,
            GlyphQuadData {
                position: [local_x, local_y, TEXT_Z_OFFSET],
                size: [quad_w * scale.x, quad_h * scale.y],
                uv_rect: metrics.uv_rect,
                color,
            },
        ));
    }

    glyph_quad::clip_overlapping_quads(&mut quads);

    if let Some(clip_rect) = clip_rect {
        let clip_local =
            batching::panel_clip_rect_local(Some(clip_rect), scale.x, scale.y, anchor.x, anchor.y);
        quads.retain(|(_, quad)| {
            glyph_quad::clip_quad_to_rect(quad, clip_local.to_array()).is_some()
        });
    }

    stats.atlas_ms = atlas_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    stats.emitted_quads = quads.len();

    (quads, stats)
}

fn all_glyphs_ready_when_required(
    config: &LayoutTextStyle,
    shaped_glyphs: &[ShapedGlyph],
    font_data: &[u8],
    atlas: &mut MsdfAtlas,
    stats: &mut TextBuildStats,
) -> bool {
    if config.loading_policy() != GlyphLoadingPolicy::WhenReady {
        return true;
    }

    let mut all_ready = true;
    for shaped_glyph in shaped_glyphs {
        let glyph_key = GlyphKey {
            font_id:     config.font_id(),
            glyph_index: shaped_glyph.glyph_id,
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
