use std::time::Instant;

use bevy::prelude::*;

use super::BackendRenderServices;
use super::ComputedWorldText;
use super::PanelChild;
use super::WorldText;
use super::mesh_spawning;
use super::mesh_spawning::MeshSpawnAssets;
use super::mesh_spawning::WorldTextMesh;
use super::readiness::AwaitingReady;
use super::shaping;
use crate::cascade::CascadeDefault;
use crate::cascade::FontUnit;
use crate::cascade::Resolved;
use crate::cascade::TextAlpha;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::ShapedTextCache;
use crate::layout::WorldTextStyle;
use crate::render::constants;
use crate::render::text_shaping::GlyphReadiness;
use crate::render::text_shaping::TextBuildStats;
use crate::render::text_shaping::TextShapingContext;
use crate::text::FontRegistry;
use crate::text::PreparedTextRun;

type ChangedWorldTextQuery<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<WorldText>,
        Without<PanelChild>,
        Or<(
            Changed<WorldText>,
            Changed<WorldTextStyle>,
            Changed<Resolved<FontUnit>>,
        )>,
    ),
>;
/// Renders [`WorldText`] entities as slug glyph meshes.
///
/// Processes entities whose `WorldText`, `WorldTextStyle`, or resolved font unit
/// changed — runs text shaping, builds their glyph meshes, and fires
/// [`WorldTextReady`](super::WorldTextReady). Glyph geometry is built
/// synchronously, so a changed entity is ready within the same pass.
pub(super) fn render_world_text(
    changed_texts: ChangedWorldTextQuery<'_, '_>,
    texts: Query<(&WorldText, &WorldTextStyle), Without<PanelChild>>,
    resolved_alphas: Query<&Resolved<TextAlpha>, Without<PanelChild>>,
    resolved_units: Query<&Resolved<FontUnit>, Without<PanelChild>>,
    old_meshes: Query<(Entity, &ChildOf), With<WorldTextMesh>>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut backend_services: BackendRenderServices,
    font_default: Res<CascadeDefault<FontUnit>>,
    alpha_default: Res<CascadeDefault<TextAlpha>>,
    mut commands: Commands,
) {
    let to_process: Vec<Entity> = changed_texts.iter().collect();
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
            continue;
        }

        // The cascade seeds and keeps `Resolved` current; the renderer reads it
        // directly. The fallback to the global default only guards the edge
        // where a standalone is rendered before its spawn seed has flushed.
        let resolved_unit = resolved_units
            .get(entity)
            .map_or(font_default.0.0, |r| r.0.0);
        let scale = style
            .world_scale()
            .unwrap_or_else(|| resolved_unit.meters_per_unit());
        let resolved_alpha = resolved_alphas
            .get(entity)
            .map_or(alpha_default.0.0, |r| r.0.0);

        let mut shared_services = WorldTextRenderServices::new(
            &font_registry,
            &shaping_cx,
            &mut cache,
            &old_meshes,
            &mut meshes,
        );
        let (stats, mesh_ms) = shared_services.render_entity(
            entity,
            world_text.text(),
            style,
            scale,
            resolved_alpha,
            &mut backend_services,
            &mut commands,
        );
        text_stats.accumulate(&stats);
        mesh_ms_total += mesh_ms;
    }

    let total_ms = total_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    log_render_stats(total_ms, text_count, &text_stats, mesh_ms_total);
}

struct WorldTextRenderServices<'a, 'mesh_world, 'mesh_state, 'mesh_data> {
    font_registry: &'a FontRegistry,
    shaping_cx:    &'a TextShapingContext,
    cache:         &'a mut ShapedTextCache,
    old_meshes:
        &'a Query<'mesh_world, 'mesh_state, (Entity, &'mesh_data ChildOf), With<WorldTextMesh>>,
    meshes:        &'a mut Assets<Mesh>,
}

impl<'a, 'mesh_world, 'mesh_state, 'mesh_data>
    WorldTextRenderServices<'a, 'mesh_world, 'mesh_state, 'mesh_data>
{
    const fn new(
        font_registry: &'a FontRegistry,
        shaping_cx: &'a TextShapingContext,
        cache: &'a mut ShapedTextCache,
        old_meshes: &'a Query<
            'mesh_world,
            'mesh_state,
            (Entity, &'mesh_data ChildOf),
            With<WorldTextMesh>,
        >,
        meshes: &'a mut Assets<Mesh>,
    ) -> Self {
        Self {
            font_registry,
            shaping_cx,
            cache,
            old_meshes,
            meshes,
        }
    }

    fn render_entity(
        &mut self,
        entity: Entity,
        text: &str,
        style: &WorldTextStyle,
        scale: f32,
        alpha_mode: AlphaMode,
        backend_services: &mut BackendRenderServices<'_>,
        commands: &mut Commands,
    ) -> (TextBuildStats, f32) {
        let text_run = shaping::build_world_text_run(
            text,
            style,
            self.font_registry,
            &mut backend_services.backend,
            self.shaping_cx,
            self.cache,
            scale,
        );
        let readiness = GlyphReadiness::from(&text_run.stats);
        let mut mesh_ms_total = 0.0_f32;

        #[cfg(feature = "typography_overlay")]
        if readiness == GlyphReadiness::Ready || readiness == GlyphReadiness::Invisible {
            commands.entity(entity).insert(ComputedWorldText {
                anchor_y: text_run.anchor_y,
                glyphs:   text_run.glyphs,
            });
        }

        if matches!(
            readiness,
            GlyphReadiness::Ready | GlyphReadiness::Invisible | GlyphReadiness::Failed
        ) {
            // Despawning the run's mesh frees its storage through the
            // `On<Remove, WorldTextMesh>` observer — the same per-run cleanup
            // the panel-text path uses. No blunt `clear_run_storage()`, which
            // would wipe every panel run's storage out of the shared cache.
            mesh_spawning::despawn_mesh_children(entity, self.old_meshes, commands);
        }

        if readiness == GlyphReadiness::Ready
            && let Some(prepared) = text_run.prepared.as_ref()
        {
            mesh_ms_total += self.spawn_run(
                prepared,
                entity,
                style,
                alpha_mode,
                backend_services,
                commands,
            );
        }

        apply_readiness_markers(entity, readiness, commands);
        (text_run.stats, mesh_ms_total)
    }

    fn spawn_run(
        &mut self,
        prepared: &PreparedTextRun,
        entity: Entity,
        style: &WorldTextStyle,
        alpha_mode: AlphaMode,
        backend_services: &mut BackendRenderServices<'_>,
        commands: &mut Commands,
    ) -> f32 {
        let mut assets = MeshSpawnAssets {
            meshes: self.meshes,
            materials: &mut backend_services.materials,
            storage_buffers: &mut backend_services.storage_buffers,
            commands,
        };
        mesh_spawning::spawn_world_text_meshes(
            prepared,
            &mut backend_services.backend,
            entity,
            style,
            alpha_mode,
            &mut assets,
        )
    }
}

fn apply_readiness_markers(entity: Entity, readiness: GlyphReadiness, commands: &mut Commands) {
    // Glyph geometry is built synchronously, so a rendered run is ready at once;
    // mark it for the post-`CalculateBounds` `WorldTextReady`. `Failed`/`Idle`
    // produce no meshes and need no signal.
    if readiness == GlyphReadiness::Ready {
        commands.entity(entity).insert(AwaitingReady);
    }
}

fn log_render_stats(total_ms: f32, text_count: usize, stats: &TextBuildStats, mesh_ms_total: f32) {
    if total_ms <= constants::WORLD_TEXT_DEBUG_LOG_THRESHOLD_MS && stats.failed_glyphs == 0 {
        return;
    }
    bevy::log::debug!(
        "render_world_text: total={total_ms:.1}ms texts={text_count} text_shaping={:.1}ms atlas={:.1}ms mesh={mesh_ms_total:.1}ms glyphs={} ready={} invisible={} failed={} quads={}",
        stats.shape_ms,
        stats.atlas_ms,
        stats.glyphs,
        stats.ready_glyphs,
        stats.invisible_glyphs,
        stats.failed_glyphs,
        stats.emitted_quads,
    );
}
