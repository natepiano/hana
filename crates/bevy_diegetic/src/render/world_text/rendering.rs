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
use crate::render::glyph_material::GlyphMaterial;
use crate::render::glyph_quad::GlyphQuadData;
use crate::render::text_shaping::GlyphReadiness;
use crate::render::text_shaping::TextBuildStats;
use crate::render::text_shaping::TextShapingContext;
use crate::text::AtlasSlot;
use crate::text::FontRegistry;

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
    mut atlas_slot: ResMut<AtlasSlot>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GlyphMaterial>>,
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
            // allow-banned: text-shaping pipeline binding
            &world_text.0,
            style,
            &font_registry,
            atlas_slot.rasterize_target_mut(),
            &shaping_cx,
            &mut cache,
            scale,
        );
        text_stats.accumulate(&shaped.stats); // allow-banned: text-shaping result field

        // During a parallel-atlas swap, the text-shaping pass has
        // already queued every visible glyph onto pending via
        // `rasterize_target_mut`. Force `Pending` so we keep
        // `PendingGlyphs` on the entity and skip mesh respawn —
        // existing meshes/materials keep rendering from active until
        // the driver finalizes the swap.
        let readiness = if atlas_slot.is_swapping() {
            GlyphReadiness::Pending
        } else {
            GlyphReadiness::from(&shaped.stats) // allow-banned: text-shaping result field
        };

        #[cfg(feature = "typography_overlay")]
        if readiness == GlyphReadiness::Ready || readiness == GlyphReadiness::Invisible {
            commands.entity(entity).insert(ComputedWorldText {
                anchor_y: shaped.anchor_y,
                glyphs:   shaped.glyphs,
            });
        }

        let mut page_quads: HashMap<u32, Vec<GlyphQuadData>> = HashMap::new();
        for (page_index, quad) in shaped.quads {
            page_quads.entry(page_index).or_default().push(quad);
        }
        let total_quads: usize = page_quads.values().map(Vec::len).sum();

        if !atlas_slot.is_swapping()
            && matches!(
                readiness,
                GlyphReadiness::Ready | GlyphReadiness::Invisible | GlyphReadiness::Failed
            )
        {
            mesh_spawning::despawn_mesh_children(entity, &old_meshes, &mut commands);
        }

        // Skip mesh respawn while a swap is in flight — existing
        // meshes keep rendering against the active atlas, and we'll
        // get triggered again by the `AtlasSwapCompleted` observer
        // once the new atlas is live.
        if total_quads > 0 && !atlas_slot.is_swapping() {
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
                atlas_slot.active(),
                resolved_alpha.0,
                &mut assets,
            );
        }

        apply_readiness_markers(entity, readiness, &mut commands);
    }

    let total_ms = total_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    log_render_stats(total_ms, text_count, &text_stats, mesh_ms_total);
}

fn apply_readiness_markers(entity: Entity, readiness: GlyphReadiness, commands: &mut Commands) {
    match readiness {
        GlyphReadiness::Pending => {
            commands.entity(entity).insert_if_new(PendingGlyphs);
        },
        GlyphReadiness::Ready | GlyphReadiness::Invisible => {
            commands.entity(entity).remove::<PendingGlyphs>();
            commands.entity(entity).insert(AwaitingReady);
        },
        GlyphReadiness::Failed | GlyphReadiness::Idle => {
            commands.entity(entity).remove::<PendingGlyphs>();
        },
    }
}

fn log_render_stats(total_ms: f32, text_count: usize, stats: &TextBuildStats, mesh_ms_total: f32) {
    if total_ms <= constants::WORLD_TEXT_DEBUG_LOG_THRESHOLD_MS
        && stats.queued_glyphs == 0
        && stats.pending_glyphs == 0
        && stats.failed_glyphs == 0
    {
        return;
    }
    bevy::log::debug!(
        "render_world_text: total={total_ms:.1}ms texts={text_count} text_shaping={:.1}ms atlas={:.1}ms mesh={mesh_ms_total:.1}ms glyphs={} ready={} invisible={} queued={} pending={} failed={} quads={}",
        stats.shape_ms,
        stats.atlas_ms,
        stats.glyphs,
        stats.ready_glyphs,
        stats.invisible_glyphs,
        stats.queued_glyphs,
        stats.pending_glyphs,
        stats.failed_glyphs,
        stats.emitted_quads,
    );
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
