use std::time::Instant;

use bevy::prelude::*;

use super::BackendRenderServices;
use super::ComputedWorldText;
use super::PanelTextChild;
use super::WorldFontUnit;
use super::WorldText;
use super::WorldTextAlpha;
use super::mesh_spawning;
use super::mesh_spawning::SlugMeshSpawnAssets;
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
use crate::render::text_shaping::GlyphReadiness;
use crate::render::text_shaping::TextBuildStats;
use crate::render::text_shaping::TextShapingContext;
use crate::text::FontRegistry;
use crate::text::slug::SlugPreparedTextRun;

type ChangedWorldTextQuery<'w, 's> = Query<
    'w,
    's,
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
>;
type PendingWorldTextQuery<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<WorldText>,
        With<PendingGlyphs>,
        Without<PanelTextChild>,
    ),
>;
/// Renders [`WorldText`] entities as MSDF glyph meshes.
///
/// Processes entities in two cases:
/// - **Changed**: `WorldText` or `TextStyle` was modified — re-shape and check glyphs.
/// - **Pending**: entity has [`PendingGlyphs`] — re-check atlas each frame.
///
/// When all glyphs are ready, builds meshes and fires [`WorldTextReady`](super::WorldTextReady).
/// When glyphs are still missing, adds/keeps [`PendingGlyphs`].
pub(super) fn render_world_text(
    changed_texts: ChangedWorldTextQuery<'_, '_>,
    pending_texts: PendingWorldTextQuery<'_, '_>,
    texts: Query<(&WorldText, &WorldTextStyle), Without<PanelTextChild>>,
    resolved_alphas: Query<&Resolved<WorldTextAlpha>, Without<PanelTextChild>>,
    resolved_units: Query<&Resolved<WorldFontUnit>, Without<PanelTextChild>>,
    old_meshes: Query<(Entity, &ChildOf), Or<(With<WorldTextMesh>, With<WorldTextShadowProxy>)>>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut backend_services: BackendRenderServices,
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

        if world_text.text().is_empty() {
            mesh_spawning::despawn_mesh_children(entity, &old_meshes, &mut commands);
            commands.entity(entity).remove::<PendingGlyphs>();
            continue;
        }

        let resolved_unit =
            re_resolve_world_font_unit(entity, style, &resolved_units, &defaults, &mut commands);
        let scale = style
            .world_scale()
            .unwrap_or_else(|| resolved_unit.0.meters_per_unit());

        let mut shared_services = SlugWorldTextRenderServices::new(
            &font_registry,
            &shaping_cx,
            &mut cache,
            &resolved_alphas,
            &old_meshes,
            &mut meshes,
            &defaults,
        );
        let (stats, mesh_ms) = shared_services.render_entity(
            entity,
            world_text.text(),
            style,
            scale,
            &mut backend_services,
            &mut commands,
        );
        text_stats.accumulate(&stats);
        mesh_ms_total += mesh_ms;
    }

    let total_ms = total_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    log_render_stats(total_ms, text_count, &text_stats, mesh_ms_total);
}

struct SlugWorldTextRenderServices<
    'a,
    'alpha_world,
    'alpha_state,
    'alpha_data,
    'mesh_world,
    'mesh_state,
    'mesh_data,
> {
    font_registry:   &'a FontRegistry,
    shaping_cx:      &'a TextShapingContext,
    cache:           &'a mut ShapedTextCache,
    resolved_alphas: &'a Query<
        'alpha_world,
        'alpha_state,
        &'alpha_data Resolved<WorldTextAlpha>,
        Without<PanelTextChild>,
    >,
    old_meshes: &'a Query<
        'mesh_world,
        'mesh_state,
        (Entity, &'mesh_data ChildOf),
        Or<(With<WorldTextMesh>, With<WorldTextShadowProxy>)>,
    >,
    meshes:          &'a mut Assets<Mesh>,
    defaults:        &'a CascadeDefaults,
}

impl<'a, 'alpha_world, 'alpha_state, 'alpha_data, 'mesh_world, 'mesh_state, 'mesh_data>
    SlugWorldTextRenderServices<
        'a,
        'alpha_world,
        'alpha_state,
        'alpha_data,
        'mesh_world,
        'mesh_state,
        'mesh_data,
    >
{
    const fn new(
        font_registry: &'a FontRegistry,
        shaping_cx: &'a TextShapingContext,
        cache: &'a mut ShapedTextCache,
        resolved_alphas: &'a Query<
            'alpha_world,
            'alpha_state,
            &'alpha_data Resolved<WorldTextAlpha>,
            Without<PanelTextChild>,
        >,
        old_meshes: &'a Query<
            'mesh_world,
            'mesh_state,
            (Entity, &'mesh_data ChildOf),
            Or<(With<WorldTextMesh>, With<WorldTextShadowProxy>)>,
        >,
        meshes: &'a mut Assets<Mesh>,
        defaults: &'a CascadeDefaults,
    ) -> Self {
        Self {
            font_registry,
            shaping_cx,
            cache,
            resolved_alphas,
            old_meshes,
            meshes,
            defaults,
        }
    }

    fn render_entity(
        &mut self,
        entity: Entity,
        text: &str,
        style: &WorldTextStyle,
        scale: f32,
        backend_services: &mut BackendRenderServices<'_>,
        commands: &mut Commands,
    ) -> (TextBuildStats, f32) {
        let slug_text = shaping::build_world_slug_text(
            text,
            style,
            self.font_registry,
            &mut backend_services.slug_backend,
            self.shaping_cx,
            self.cache,
            scale,
        );
        let readiness = GlyphReadiness::from(&slug_text.stats);
        let mut mesh_ms_total = 0.0_f32;

        #[cfg(feature = "typography_overlay")]
        if readiness == GlyphReadiness::Ready || readiness == GlyphReadiness::Invisible {
            commands.entity(entity).insert(ComputedWorldText {
                anchor_y: slug_text.anchor_y,
                glyphs:   slug_text.glyphs,
            });
        }

        if matches!(
            readiness,
            GlyphReadiness::Ready | GlyphReadiness::Invisible | GlyphReadiness::Failed
        ) {
            backend_services.slug_backend.clear_run_storage();
            mesh_spawning::despawn_mesh_children(entity, self.old_meshes, commands);
        }

        if let Some(prepared) = slug_text.prepared.as_ref() {
            mesh_ms_total += self.spawn_run(prepared, entity, style, backend_services, commands);
        }

        apply_readiness_markers(entity, readiness, commands);
        (slug_text.stats, mesh_ms_total)
    }

    fn spawn_run(
        &mut self,
        prepared: &SlugPreparedTextRun,
        entity: Entity,
        style: &WorldTextStyle,
        backend_services: &mut BackendRenderServices<'_>,
        commands: &mut Commands,
    ) -> f32 {
        let resolved_alpha = re_resolve_world_text_alpha(
            entity,
            style,
            self.resolved_alphas,
            self.defaults,
            commands,
        );
        let mut assets = SlugMeshSpawnAssets {
            meshes: self.meshes,
            materials: &mut backend_services.slug_materials,
            storage_buffers: &mut backend_services.storage_buffers,
            commands,
        };
        mesh_spawning::spawn_slug_world_text_meshes(
            prepared,
            &mut backend_services.slug_backend,
            entity,
            style,
            resolved_alpha.0,
            &mut assets,
        )
    }
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
    changed_texts: &ChangedWorldTextQuery<'_, '_>,
    pending_texts: &PendingWorldTextQuery<'_, '_>,
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
