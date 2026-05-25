use std::collections::HashSet;
use std::time::Instant;

use bevy::math::Vec4;
use bevy::prelude::*;

use super::PanelText;
use super::PanelTextLayout;
use super::alpha::PanelTextAlpha;
use crate::cascade::CascadeDefaults;
use crate::cascade::CascadePanelChild;
use crate::cascade::Resolved;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::BoundingBox;
use crate::layout::LayoutTextStyle;
use crate::layout::ShapedTextCache;
use crate::layout::WorldTextStyle;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
use crate::render::constants;
use crate::render::text_shaping;
use crate::render::text_shaping::GlyphReadiness;
use crate::render::text_shaping::TextBuildStats;
use crate::render::text_shaping::TextShapingContext;
use crate::render::world_text::AwaitingReady;
use crate::render::world_text::PanelChild;
use crate::render::world_text::PendingGlyphs;
use crate::render::world_text::WorldText;
use crate::text::FontRegistry;
use crate::text::SLUG_DEFAULT_BAND_COUNT;
use crate::text::SlugBackend;

/// Shapes text for panel [`WorldText`] children that are changed or pending.
pub(super) fn shape_panel_text_children(
    changed_texts: Query<
        Entity,
        (
            With<PanelChild>,
            With<WorldText>,
            Or<(
                Changed<WorldText>,
                Changed<WorldTextStyle>,
                Changed<PanelTextLayout>,
                Changed<Resolved<PanelTextAlpha>>,
            )>,
        ),
    >,
    pending_texts: Query<Entity, (With<PanelChild>, With<WorldText>, With<PendingGlyphs>)>,
    texts: Query<(&WorldText, &WorldTextStyle, &PanelTextLayout, &ChildOf)>,
    panel_alpha: Query<&Resolved<PanelTextAlpha>, With<DiegeticPanel>>,
    existing_child_alpha: Query<&Resolved<PanelTextAlpha>, With<PanelChild>>,
    defaults: Res<CascadeDefaults>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut backend: ResMut<SlugBackend>,
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
        perf.panel_text.shaped_panels = 0;
        perf.panel_text.total_ms = perf.panel_text.mesh_build_ms;
        return;
    }

    for entity in to_process {
        let Ok((world_text, style, panel_text_child, child_of)) = texts.get(entity) else {
            continue;
        };

        if world_text.text().is_empty() {
            clear_panel_text_output(entity, &mut commands);
            continue;
        }

        let config = style.as_layout_config();
        let placement = QuadPlacement {
            bounds:    panel_text_child.bounds,
            scale:     Vec2::new(panel_text_child.scale_x, panel_text_child.scale_y),
            anchor:    Vec2::new(panel_text_child.anchor_x, panel_text_child.anchor_y),
            clip_rect: panel_text_child.clip_rect,
        };

        let (panel_text, stats) = build_panel_text(
            world_text.text(),
            &config,
            &placement,
            &mut backend,
            &font_registry,
            &shaping_cx,
            &mut cache,
        );
        aggregate.accumulate(&stats);
        shaped_panels.insert(child_of.parent());
        let readiness = GlyphReadiness::from(&stats);
        apply_panel_result(
            entity,
            child_of.parent(),
            panel_text,
            readiness,
            &panel_alpha,
            &existing_child_alpha,
            &defaults,
            &mut commands,
        );
    }

    perf.panel_text.shape_ms = shape_stage_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.panel_text.parley_ms = aggregate.shape_ms;
    perf.panel_text.shaped_panels = shaped_panels.len();
    perf.panel_text.total_ms = perf.panel_text.shape_ms + perf.panel_text.mesh_build_ms;
}

/// Placement parameters that position glyphs into panel-local space.
struct QuadPlacement {
    bounds:    BoundingBox,
    scale:     Vec2,
    anchor:    Vec2,
    clip_rect: Option<BoundingBox>,
}

fn clear_panel_text_output(entity: Entity, commands: &mut Commands) {
    commands
        .entity(entity)
        .remove::<PendingGlyphs>()
        .remove::<PanelText>();
}

fn build_panel_text(
    text: &str,
    config: &LayoutTextStyle,
    placement: &QuadPlacement,
    backend: &mut SlugBackend,
    font_registry: &FontRegistry,
    shaping_cx: &TextShapingContext,
    cache: &mut ShapedTextCache,
) -> (Option<PanelText>, TextBuildStats) {
    let mut stats = TextBuildStats {
        texts: 1,
        ..Default::default()
    };
    let layout_start = Instant::now();
    let layout_run =
        text_shaping::shape_text_cached(text, config, font_registry, shaping_cx, cache);
    stats.shape_ms = layout_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    stats.glyphs = layout_run.glyphs.len();
    let positioned_glyphs =
        text_shaping::positioned_glyphs(&layout_run.glyphs, font_registry, &mut stats);
    if stats.failed_glyphs > 0 {
        return (None, stats);
    }

    let run_start = Instant::now();
    let anchor = panel_layout_anchor(placement);
    let prepared = match backend.prepare_positioned_run_with_scale(
        &positioned_glyphs,
        anchor,
        config.size(),
        placement.scale,
        SLUG_DEFAULT_BAND_COUNT,
    ) {
        Ok(prepared) => prepared,
        Err(err) => {
            bevy::log::warn!("panel text unsupported: {err}");
            stats.failed_glyphs += positioned_glyphs.len().max(1);
            stats.atlas_ms = run_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
            return (None, stats);
        },
    };

    stats.ready_glyphs = positioned_glyphs.len();
    stats.emitted_quads = prepared.glyph_count();
    stats.atlas_ms = run_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;

    let clip_rect = placement.clip_rect.map(|clip_rect| {
        panel_clip_rect_local(
            Some(clip_rect),
            placement.scale.x,
            placement.scale.y,
            placement.anchor.x,
            placement.anchor.y,
        )
        .to_array()
    });
    (
        Some(PanelText {
            prepared,
            render_mode: config.render_mode(),
            shadow_mode: config.shadow_mode(),
            alpha_mode: config.alpha_mode(),
            fill_color: config.color(),
            clip_rect,
        }),
        stats,
    )
}

fn panel_layout_anchor(placement: &QuadPlacement) -> Vec2 {
    Vec2::new(
        placement.anchor.x / placement.scale.x - placement.bounds.x,
        placement.anchor.y / placement.scale.y - placement.bounds.y,
    )
}

fn panel_clip_rect_local(
    clip_rect: Option<BoundingBox>,
    scale_x: f32,
    scale_y: f32,
    anchor_x: f32,
    anchor_y: f32,
) -> Vec4 {
    clip_rect.map_or(constants::UNCLIPPED_TEXT_CLIP_RECT, |clip| {
        Vec4::new(
            clip.x.mul_add(scale_x, -anchor_x),
            (clip.y + clip.height).mul_add(-scale_y, anchor_y),
            (clip.x + clip.width).mul_add(scale_x, -anchor_x),
            clip.y.mul_add(-scale_y, anchor_y),
        )
    })
}

fn apply_panel_result(
    entity: Entity,
    panel_entity: Entity,
    panel_text: Option<PanelText>,
    readiness: GlyphReadiness,
    panel_alpha: &Query<&Resolved<PanelTextAlpha>, With<DiegeticPanel>>,
    existing_child_alpha: &Query<&Resolved<PanelTextAlpha>, With<PanelChild>>,
    defaults: &CascadeDefaults,
    commands: &mut Commands,
) {
    match readiness {
        GlyphReadiness::Ready | GlyphReadiness::Invisible => {
            let Some(panel_text) = panel_text else {
                commands.entity(entity).remove::<PendingGlyphs>();
                return;
            };
            let panel_fallback = panel_alpha.get(panel_entity).map_or_else(
                |_| PanelTextAlpha::global_default(defaults),
                |resolved| resolved.0,
            );
            let resolved = panel_text.alpha_mode.map_or(panel_fallback, PanelTextAlpha);
            let alpha_unchanged = existing_child_alpha
                .get(entity)
                .is_ok_and(|current| current.0 == resolved);
            commands
                .entity(entity)
                .insert(panel_text)
                .remove::<PendingGlyphs>()
                .insert(AwaitingReady);
            if !alpha_unchanged {
                commands.entity(entity).insert(Resolved(resolved));
            }
        },
        GlyphReadiness::Pending => {
            commands.entity(entity).insert_if_new(PendingGlyphs);
        },
        GlyphReadiness::Failed => {
            commands
                .entity(entity)
                .remove::<PendingGlyphs>()
                .remove::<PanelText>();
        },
        GlyphReadiness::Idle => {},
    }
}
