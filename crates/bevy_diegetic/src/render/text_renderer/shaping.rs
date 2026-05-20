use std::collections::HashSet;
use std::time::Instant;

use bevy::prelude::*;
use bevy_kana::ToF32;

use super::batching;
#[cfg(feature = "slug_text")]
use super::batching::PanelSlugTextRun;
use super::batching::PanelTextAlpha;
use super::batching::PanelTextQuads;
use crate::cascade::CascadeDefaults;
use crate::cascade::CascadePanelChild;
use crate::cascade::Resolved;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::BoundingBox;
use crate::layout::GlyphLoadingPolicy;
use crate::layout::LayoutTextStyle;
use crate::layout::ShapedTextCache;
use crate::layout::WorldTextStyle;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
#[cfg(feature = "slug_text")]
use crate::render::TextRendererBackend;
#[cfg(feature = "slug_text")]
use crate::render::TextRendererPreference;
use crate::render::constants::TEXT_Z_OFFSET;
use crate::render::glyph_quad;
use crate::render::glyph_quad::GlyphQuadData;
use crate::render::text_shaping;
use crate::render::text_shaping::GlyphQuadPlacement;
use crate::render::text_shaping::GlyphReadiness;
use crate::render::text_shaping::PositionedGlyph;
use crate::render::text_shaping::TextBuildStats;
use crate::render::text_shaping::TextShapingContext;
use crate::render::world_text::AwaitingReady;
use crate::render::world_text::PanelTextChild;
use crate::render::world_text::PendingGlyphs;
use crate::render::world_text::WorldText;
#[cfg(feature = "slug_text")]
use crate::slug_text_spike::DEFAULT_BAND_COUNT;
#[cfg(feature = "slug_text")]
use crate::slug_text_spike::SlugBackend;
use crate::text::AtlasSlot;
use crate::text::FontRegistry;
use crate::text::GlyphAtlas;
use crate::text::GlyphLookup;

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
    existing_child_alpha: Query<&Resolved<PanelTextAlpha>, With<PanelTextChild>>,
    defaults: Res<CascadeDefaults>,
    mut atlas_slot: ResMut<AtlasSlot>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    #[cfg(feature = "slug_text")] text_backend: Res<TextRendererPreference>,
    #[cfg(feature = "slug_text")] mut slug_backend: ResMut<SlugBackend>,
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

        #[cfg(feature = "slug_text")]
        if text_backend.backend() == TextRendererBackend::Slug {
            let (panel_slug_run, stats) = build_panel_slug_text(
                &world_text.0,
                &config,
                &placement,
                &mut slug_backend,
                &font_registry,
                &shaping_cx,
                &mut cache,
            );
            aggregate.accumulate(&stats);
            shaped_panels.insert(child_of.parent());
            let readiness = GlyphReadiness::from(&stats);
            apply_panel_slug_result(
                entity,
                child_of.parent(),
                panel_slug_run,
                readiness,
                &panel_alpha,
                &existing_child_alpha,
                &defaults,
                &mut commands,
            );
            continue;
        }

        let mut services = TextQuadServices {
            font_registry: &font_registry,
            atlas:         atlas_slot.rasterize_target_mut(),
            shaping_cx:    &shaping_cx,
            cache:         &mut cache,
        };
        let (quads, stats) = shape_text_to_quads(&world_text.0, &config, &placement, &mut services);

        aggregate.accumulate(&stats);
        shaped_panels.insert(child_of.parent());

        // During a parallel-atlas swap, the text-shaping pass above
        // has queued visible glyphs onto pending. Force `Pending` so
        // we keep `PendingGlyphs` and don't replace `PanelTextQuads`
        // (which would let the batcher build materials whose UVs
        // came from pending but whose image still points to active).
        let readiness = if atlas_slot.is_swapping() {
            GlyphReadiness::Pending
        } else {
            GlyphReadiness::from(&stats)
        };
        apply_panel_quad_result(
            entity,
            child_of.parent(),
            quads,
            &config,
            readiness,
            &panel_alpha,
            &existing_child_alpha,
            &defaults,
            &mut commands,
        );
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
    atlas:         &'a mut GlyphAtlas,
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

fn clear_panel_text_output(entity: Entity, commands: &mut Commands) {
    commands
        .entity(entity)
        .remove::<PendingGlyphs>()
        .remove::<PanelTextQuads>();
    #[cfg(feature = "slug_text")]
    commands.entity(entity).remove::<PanelSlugTextRun>();
}

fn apply_panel_quad_result(
    entity: Entity,
    panel_entity: Entity,
    quads: Vec<(u32, GlyphQuadData)>,
    config: &LayoutTextStyle,
    readiness: GlyphReadiness,
    panel_alpha: &Query<&Resolved<PanelTextAlpha>, With<DiegeticPanel>>,
    existing_child_alpha: &Query<&Resolved<PanelTextAlpha>, With<PanelTextChild>>,
    defaults: &CascadeDefaults,
    commands: &mut Commands,
) {
    match readiness {
        GlyphReadiness::Ready | GlyphReadiness::Invisible => {
            let panel_text_quads = PanelTextQuads {
                quads,
                render_mode: config.render_mode(),
                shadow_mode: config.shadow_mode(),
                alpha_mode: config.alpha_mode(),
            };
            let panel_fallback = panel_alpha.get(panel_entity).map_or_else(
                |_| PanelTextAlpha::global_default(defaults),
                |resolved| resolved.0,
            );
            let resolved =
                PanelTextAlpha::entity_value(&panel_text_quads).unwrap_or(panel_fallback);
            let alpha_unchanged = existing_child_alpha
                .get(entity)
                .is_ok_and(|current| current.0 == resolved);
            commands.entity(entity).insert(panel_text_quads);
            #[cfg(feature = "slug_text")]
            commands.entity(entity).remove::<PanelSlugTextRun>();
            if !alpha_unchanged {
                commands.entity(entity).insert(Resolved(resolved));
            }
            commands.entity(entity).remove::<PendingGlyphs>();
            commands.entity(entity).insert(AwaitingReady);
        },
        GlyphReadiness::Pending => {
            commands.entity(entity).insert_if_new(PendingGlyphs);
        },
        GlyphReadiness::Failed => {
            clear_panel_text_output(entity, commands);
        },
        GlyphReadiness::Idle => {},
    }
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
    let positioned_glyphs =
        text_shaping::positioned_glyphs(&shaped.glyphs, font_registry, &mut stats);

    let atlas_start = Instant::now();
    if stats.failed_glyphs > 0
        || !all_glyphs_ready_when_required(config, &positioned_glyphs, atlas, &mut stats)
    {
        stats.atlas_ms = atlas_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
        return (Vec::new(), stats);
    }

    let linear: LinearRgba = config.color().into();
    let color = [linear.red, linear.green, linear.blue, linear.alpha];
    let em_scale = config.size() / atlas.canonical_size().to_f32();

    let mut quads = Vec::with_capacity(shaped.glyphs.len());
    for positioned_glyph in positioned_glyphs {
        let metrics = match atlas.lookup_or_queue(
            text_shaping::glyph_key(positioned_glyph),
            positioned_glyph.font.data(),
        ) {
            GlyphLookup::Ready(metrics) => {
                if metrics.pixel_width == 0 || metrics.pixel_height == 0 {
                    stats.invisible_glyphs += 1;
                    continue;
                }
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

        let shaped_glyph = positioned_glyph.glyph;
        let glyph_x = bounds.x + shaped_glyph.x;
        let glyph_y = bounds.y + shaped_glyph.baseline + shaped_glyph.y;
        let quad_width = metrics.pixel_width.to_f32() * em_scale;
        let quad_height = metrics.pixel_height.to_f32() * em_scale;
        let quad_layout_x = (metrics.bearing_x - metrics.pad_x_em).mul_add(config.size(), glyph_x);
        let quad_layout_y =
            (-(metrics.bearing_y + metrics.pad_y_em)).mul_add(config.size(), glyph_y);
        let local_x = quad_layout_x.mul_add(scale.x, -anchor.x);
        let local_y = (-quad_layout_y).mul_add(scale.y, anchor.y);
        let placement = GlyphQuadPlacement {
            position: [local_x, local_y, TEXT_Z_OFFSET],
            size:     [quad_width * scale.x, quad_height * scale.y],
        };

        quads.push((
            metrics.page_index,
            placement.into_atlas_quad(metrics, color),
        ));
    }

    let padding_world = GlyphAtlas::glyph_padding_texels() * em_scale * scale.x;
    glyph_quad::clip_overlapping_quads(&mut quads, padding_world);

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

#[cfg(feature = "slug_text")]
fn build_panel_slug_text(
    text: &str,
    config: &LayoutTextStyle,
    placement: &QuadPlacement,
    slug_backend: &mut SlugBackend,
    font_registry: &FontRegistry,
    shaping_cx: &TextShapingContext,
    cache: &mut ShapedTextCache,
) -> (Option<PanelSlugTextRun>, TextBuildStats) {
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

    let slug_start = Instant::now();
    let anchor = panel_slug_layout_anchor(placement);
    let prepared = match slug_backend.prepare_positioned_run_with_scale(
        &positioned_glyphs,
        anchor,
        config.size(),
        placement.scale,
        DEFAULT_BAND_COUNT,
    ) {
        Ok(prepared) => prepared,
        Err(err) => {
            slug_backend.record_failure();
            bevy::log::warn!("panel Slug text unsupported: {err}");
            stats.failed_glyphs += positioned_glyphs.len().max(1);
            stats.atlas_ms = slug_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
            return (None, stats);
        },
    };

    stats.ready_glyphs = positioned_glyphs.len();
    stats.emitted_quads = prepared.run.run.glyphs().len();
    stats.atlas_ms = slug_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;

    let clip_rect = placement.clip_rect.map(|clip_rect| {
        batching::panel_clip_rect_local(
            Some(clip_rect),
            placement.scale.x,
            placement.scale.y,
            placement.anchor.x,
            placement.anchor.y,
        )
        .to_array()
    });
    (
        Some(PanelSlugTextRun {
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

#[cfg(feature = "slug_text")]
fn panel_slug_layout_anchor(placement: &QuadPlacement) -> Vec2 {
    Vec2::new(
        placement.anchor.x / placement.scale.x - placement.bounds.x,
        placement.anchor.y / placement.scale.y - placement.bounds.y,
    )
}

#[cfg(feature = "slug_text")]
fn apply_panel_slug_result(
    entity: Entity,
    panel_entity: Entity,
    panel_slug_run: Option<PanelSlugTextRun>,
    readiness: GlyphReadiness,
    panel_alpha: &Query<&Resolved<PanelTextAlpha>, With<DiegeticPanel>>,
    existing_child_alpha: &Query<&Resolved<PanelTextAlpha>, With<PanelTextChild>>,
    defaults: &CascadeDefaults,
    commands: &mut Commands,
) {
    match readiness {
        GlyphReadiness::Ready | GlyphReadiness::Invisible => {
            let Some(panel_slug_run) = panel_slug_run else {
                commands.entity(entity).remove::<PendingGlyphs>();
                return;
            };
            let panel_fallback = panel_alpha.get(panel_entity).map_or_else(
                |_| PanelTextAlpha::global_default(defaults),
                |resolved| resolved.0,
            );
            let resolved = panel_slug_run
                .alpha_mode
                .map_or(panel_fallback, PanelTextAlpha);
            let alpha_unchanged = existing_child_alpha
                .get(entity)
                .is_ok_and(|current| current.0 == resolved);
            commands
                .entity(entity)
                .insert(panel_slug_run)
                .remove::<PanelTextQuads>()
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
                .remove::<PanelTextQuads>()
                .remove::<PanelSlugTextRun>();
        },
        GlyphReadiness::Idle => {},
    }
}

fn all_glyphs_ready_when_required(
    config: &LayoutTextStyle,
    positioned_glyphs: &[PositionedGlyph<'_>],
    atlas: &mut GlyphAtlas,
    stats: &mut TextBuildStats,
) -> bool {
    if config.loading_policy() != GlyphLoadingPolicy::WhenReady {
        return true;
    }

    let mut all_ready = true;
    for positioned_glyph in positioned_glyphs {
        match atlas.lookup_or_queue(
            text_shaping::glyph_key(*positioned_glyph),
            positioned_glyph.font.data(),
        ) {
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
