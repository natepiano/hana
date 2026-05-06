use std::collections::HashMap;
use std::time::Instant;

use bevy::prelude::*;

use super::ComputedWorldText;
use super::PanelTextChild;
use super::WorldFontUnit;
use super::WorldText;
use super::WorldTextAlpha;
use super::mesh_spawning;
use super::mesh_spawning::MeshSpawnAssets;
use super::mesh_spawning::WorldTextMesh;
use super::mesh_spawning::WorldTextShadowProxy;
use super::readiness::AwaitingReady;
use super::readiness::PendingGlyphs;
use super::shaping;
use crate::cascade::CascadeDefaults;
use crate::cascade::CascadeTarget;
use crate::cascade::Resolved;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::ShapedTextCache;
use crate::layout::WorldTextStyle;
use crate::render::constants;
use crate::render::glyph_quad::GlyphQuadData;
use crate::render::msdf_material::MsdfTextMaterial;
use crate::render::text_shaping::GlyphReadiness;
use crate::render::text_shaping::TextBuildStats;
use crate::render::text_shaping::TextShapingContext;
use crate::text::FontRegistry;
use crate::text::MsdfAtlas;

/// Renders [`WorldText`] entities as MSDF glyph meshes.
///
/// Processes entities in two cases:
/// - **Changed**: `WorldText` or `TextStyle` was modified — re-shape and check glyphs.
/// - **Pending**: entity has [`PendingGlyphs`] — re-check atlas each frame.
///
/// When all glyphs are ready, builds meshes and fires [`WorldTextReady`](super::WorldTextReady).
/// When glyphs are still missing, adds/keeps [`PendingGlyphs`].
pub fn render_world_text(
    changed_texts: Query<
        Entity,
        (
            With<WorldText>,
            Without<PanelTextChild>,
            Or<(
                Changed<WorldText>,
                Changed<WorldTextStyle>,
                Changed<Resolved<WorldTextAlpha>>,
                Changed<Resolved<WorldFontUnit>>,
            )>,
        ),
    >,
    pending_texts: Query<
        Entity,
        (
            With<WorldText>,
            With<PendingGlyphs>,
            Without<PanelTextChild>,
        ),
    >,
    texts: Query<(&WorldText, &WorldTextStyle), Without<PanelTextChild>>,
    resolved_alphas: Query<&Resolved<WorldTextAlpha>, Without<PanelTextChild>>,
    resolved_units: Query<&Resolved<WorldFontUnit>, Without<PanelTextChild>>,
    old_meshes: Query<(Entity, &ChildOf), Or<(With<WorldTextMesh>, With<WorldTextShadowProxy>)>>,
    mut atlas: ResMut<MsdfAtlas>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MsdfTextMaterial>>,
    defaults: Res<CascadeDefaults>,
    mut commands: Commands,
) {
    let to_process = collect_entities_to_process(&changed_texts, &pending_texts);
    if to_process.is_empty() {
        return;
    }

    let total_start = Instant::now();
    let mut text_count = 0_usize;
    let mut text_stats = TextBuildStats::default();
    let mut mesh_ms_total = 0.0_f32;

    for entity in to_process {
        let Ok((world_text, style)) = texts.get(entity) else {
            continue;
        };
        text_count += 1;

        if world_text.0.is_empty() {
            mesh_spawning::despawn_mesh_children(entity, &old_meshes, &mut commands);
            commands.entity(entity).remove::<PendingGlyphs>();
            continue;
        }

        let resolved_unit =
            re_resolve_world_font_unit(entity, style, &resolved_units, &defaults, &mut commands);
        // `WorldTextStyle.world_scale` is a raw meters-per-unit override
        // that bypasses the cascade entirely.
        let scale = style
            .world_scale()
            .unwrap_or_else(|| resolved_unit.0.meters_per_unit());

        let shaped = shaping::shape_world_text(
            &world_text.0,
            style,
            &font_registry,
            &mut atlas,
            &shaping_cx,
            &mut cache,
            scale,
        );
        text_stats.accumulate(&shaped.stats);

        let readiness = GlyphReadiness::from(&shaped.stats);

        #[cfg(feature = "typography_overlay")]
        if readiness == GlyphReadiness::Ready {
            commands.entity(entity).insert(ComputedWorldText {
                anchor_x:      shaped.anchor_x,
                anchor_y:      shaped.anchor_y,
                glyph_rects:   shaped.glyph_rects,
                first_advance: shaped.first_advance,
            });
        }

        let mut page_quads: HashMap<u32, Vec<GlyphQuadData>> = HashMap::new();
        for (page_index, quad) in shaped.quads {
            page_quads.entry(page_index).or_default().push(quad);
        }
        let total_quads: usize = page_quads.values().map(Vec::len).sum();

        if total_quads > 0 {
            mesh_spawning::despawn_mesh_children(entity, &old_meshes, &mut commands);
            let resolved_alpha = re_resolve_world_text_alpha(
                entity,
                style,
                &resolved_alphas,
                &defaults,
                &mut commands,
            );
            let mut assets = MeshSpawnAssets {
                meshes:    &mut meshes,
                materials: &mut materials,
                commands:  &mut commands,
            };
            mesh_ms_total += mesh_spawning::spawn_world_text_meshes(
                &page_quads,
                entity,
                style,
                &atlas,
                resolved_alpha.0,
                &mut assets,
            );
        }

        match readiness {
            GlyphReadiness::Pending => {
                commands.entity(entity).insert_if_new(PendingGlyphs);
            },
            GlyphReadiness::Ready => {
                commands.entity(entity).remove::<PendingGlyphs>();
                commands.entity(entity).insert(AwaitingReady);
            },
            GlyphReadiness::Idle => {},
        }
    }

    let total_ms = total_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    if total_ms > constants::WORLD_TEXT_DEBUG_LOG_THRESHOLD_MS
        || text_stats.queued_glyphs > 0
        || text_stats.pending_glyphs > 0
    {
        bevy::log::debug!(
            "render_world_text: total={total_ms:.1}ms texts={text_count} shape={:.1}ms atlas={:.1}ms mesh={mesh_ms_total:.1}ms glyphs={} ready={} queued={} pending={} quads={}",
            text_stats.shape_ms,
            text_stats.atlas_ms,
            text_stats.glyphs,
            text_stats.ready_glyphs,
            text_stats.queued_glyphs,
            text_stats.pending_glyphs,
            text_stats.emitted_quads,
        );
    }
}

fn collect_entities_to_process(
    changed_texts: &Query<
        Entity,
        (
            With<WorldText>,
            Without<PanelTextChild>,
            Or<(
                Changed<WorldText>,
                Changed<WorldTextStyle>,
                Changed<Resolved<WorldTextAlpha>>,
                Changed<Resolved<WorldFontUnit>>,
            )>,
        ),
    >,
    pending_texts: &Query<
        Entity,
        (
            With<WorldText>,
            With<PendingGlyphs>,
            Without<PanelTextChild>,
        ),
    >,
) -> Vec<Entity> {
    let mut to_process: Vec<Entity> = changed_texts.iter().collect();
    for entity in pending_texts.iter() {
        if !to_process.contains(&entity) {
            to_process.push(entity);
        }
    }
    to_process
}

/// Tier-1 re-resolve for `WorldFontUnit`: recomputes from the current
/// [`WorldTextStyle`] and writes a fresh `Resolved<WorldFontUnit>` only
/// when the value actually transitioned. Covers mutations to
/// `WorldTextStyle.unit` that the cascade plugin's `On<Add>` observer
/// doesn't see.
fn re_resolve_world_font_unit(
    entity: Entity,
    style: &WorldTextStyle,
    current: &Query<&Resolved<WorldFontUnit>, Without<PanelTextChild>>,
    defaults: &CascadeDefaults,
    commands: &mut Commands,
) -> WorldFontUnit {
    let resolved = WorldFontUnit::override_value(style)
        .unwrap_or_else(|| WorldFontUnit::global_default(defaults));
    if current
        .get(entity)
        .map_or(true, |current_value| current_value.0 != resolved)
    {
        commands.entity(entity).insert(Resolved(resolved));
    }
    resolved
}

/// Tier-1 re-resolve for `WorldTextAlpha`: mirrors
/// [`re_resolve_world_font_unit`] for the alpha-mode cascade.
fn re_resolve_world_text_alpha(
    entity: Entity,
    style: &WorldTextStyle,
    current: &Query<&Resolved<WorldTextAlpha>, Without<PanelTextChild>>,
    defaults: &CascadeDefaults,
    commands: &mut Commands,
) -> WorldTextAlpha {
    let resolved = WorldTextAlpha::override_value(style)
        .unwrap_or_else(|| WorldTextAlpha::global_default(defaults));
    if current
        .get(entity)
        .map_or(true, |current_value| current_value.0 != resolved)
    {
        commands.entity(entity).insert(Resolved(resolved));
    }
    resolved
}
