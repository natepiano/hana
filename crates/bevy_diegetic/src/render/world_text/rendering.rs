use std::collections::HashMap;
use std::time::Instant;

use bevy::prelude::*;

use super::BackendRenderServices;
use super::ComputedWorldText;
use super::PanelTextChild;
use super::WorldFontUnit;
use super::WorldText;
use super::WorldTextAlpha;
use super::mesh_spawning;
use super::mesh_spawning::MeshSpawnAssets;
#[cfg(feature = "slug_text")]
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
use crate::render::glyph_material::GlyphMaterial;
use crate::render::glyph_quad::GlyphQuadData;
use crate::render::text_backend::TextRendererBackend;
use crate::render::text_shaping::GlyphReadiness;
use crate::render::text_shaping::TextBuildStats;
use crate::render::text_shaping::TextShapingContext;
#[cfg(feature = "slug_text")]
use crate::slug_text_spike::SlugBuiltTextRun;
use crate::text::AtlasSlot;
use crate::text::FontRegistry;

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
    mut atlas_slot: ResMut<AtlasSlot>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GlyphMaterial>>,
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

        if world_text.0.is_empty() {
            mesh_spawning::despawn_mesh_children(entity, &old_meshes, &mut commands);
            commands.entity(entity).remove::<PendingGlyphs>();
            continue;
        }

        let resolved_unit =
            re_resolve_world_font_unit(entity, style, &resolved_units, &defaults, &mut commands);
        let scale = style
            .world_scale()
            .unwrap_or_else(|| resolved_unit.0.meters_per_unit());

        #[cfg(feature = "slug_text")]
        if backend_services.text_backend.backend() == TextRendererBackend::Slug {
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
                &world_text.0,
                style,
                scale,
                &mut backend_services,
                &mut commands,
            );
            text_stats.accumulate(&stats);
            mesh_ms_total += mesh_ms;
            continue;
        }

        let _ = backend_services.text_backend.backend();
        let mut distance_field_services = DistanceFieldWorldTextRenderServices {
            atlas_slot:      &mut atlas_slot,
            font_registry:   &font_registry,
            shaping_cx:      &shaping_cx,
            cache:           &mut cache,
            resolved_alphas: &resolved_alphas,
            old_meshes:      &old_meshes,
            meshes:          &mut meshes,
            materials:       &mut materials,
            defaults:        &defaults,
        };
        let (stats, mesh_ms) = distance_field_services.render_entity(
            entity,
            &world_text.0,
            style,
            scale,
            &mut commands,
        );
        text_stats.accumulate(&stats);
        mesh_ms_total += mesh_ms;
    }

    let total_ms = total_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    log_render_stats(total_ms, text_count, &text_stats, mesh_ms_total);
}

struct DistanceFieldWorldTextRenderServices<
    'a,
    'alpha_world,
    'alpha_state,
    'alpha_data,
    'mesh_world,
    'mesh_state,
    'mesh_data,
> {
    atlas_slot:      &'a mut AtlasSlot,
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
    materials:       &'a mut Assets<GlyphMaterial>,
    defaults:        &'a CascadeDefaults,
}

impl DistanceFieldWorldTextRenderServices<'_, '_, '_, '_, '_, '_, '_> {
    fn render_entity(
        &mut self,
        entity: Entity,
        text: &str,
        style: &WorldTextStyle,
        scale: f32,
        commands: &mut Commands,
    ) -> (TextBuildStats, f32) {
        let layout_result = shaping::shape_world_text(
            // allow-banned: text-shaping pipeline binding
            text,
            style,
            self.font_registry,
            self.atlas_slot.rasterize_target_mut(),
            self.shaping_cx,
            self.cache,
            scale,
        );
        let readiness = if self.atlas_slot.is_swapping() {
            GlyphReadiness::Pending
        } else {
            GlyphReadiness::from(&layout_result.stats)
        };

        #[cfg(feature = "typography_overlay")]
        if readiness == GlyphReadiness::Ready || readiness == GlyphReadiness::Invisible {
            commands.entity(entity).insert(ComputedWorldText {
                anchor_y: layout_result.anchor_y,
                glyphs:   layout_result.glyphs,
            });
        }

        let (page_quads, total_quads) = collect_page_quads(layout_result.quads);
        let mut mesh_ms = 0.0_f32;
        if !self.atlas_slot.is_swapping() && readiness_finished(readiness) {
            mesh_spawning::despawn_mesh_children(entity, self.old_meshes, commands);
        }

        if total_quads > 0 && !self.atlas_slot.is_swapping() {
            mesh_ms += self.spawn_quads(&page_quads, entity, style, commands);
        }

        apply_readiness_markers(entity, readiness, commands);
        (layout_result.stats, mesh_ms)
    }

    fn spawn_quads(
        &mut self,
        page_quads: &HashMap<u32, Vec<GlyphQuadData>>,
        entity: Entity,
        style: &WorldTextStyle,
        commands: &mut Commands,
    ) -> f32 {
        let resolved_alpha = re_resolve_world_text_alpha(
            entity,
            style,
            self.resolved_alphas,
            self.defaults,
            commands,
        );
        let mut assets = MeshSpawnAssets {
            meshes: self.meshes,
            materials: self.materials,
            commands,
        };
        mesh_spawning::spawn_world_text_meshes(
            page_quads,
            entity,
            style,
            self.atlas_slot.active(),
            resolved_alpha.0,
            &mut assets,
        )
    }
}

#[cfg(feature = "slug_text")]
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

#[cfg(feature = "slug_text")]
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
                glyphs:   Vec::new(),
            });
        }

        if matches!(
            readiness,
            GlyphReadiness::Ready | GlyphReadiness::Invisible | GlyphReadiness::Failed
        ) {
            mesh_spawning::despawn_mesh_children(entity, self.old_meshes, commands);
        }

        if let Some(run) = slug_text.run.as_ref() {
            mesh_ms_total += self.spawn_run(run, entity, style, backend_services, commands);
        }

        apply_readiness_markers(entity, readiness, commands);
        (slug_text.stats, mesh_ms_total)
    }

    fn spawn_run(
        &mut self,
        run: &SlugBuiltTextRun,
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
            run,
            backend_services.slug_backend.glyph_cache(),
            entity,
            style,
            resolved_alpha.0,
            &mut assets,
        )
    }
}

fn collect_page_quads(
    quads: impl IntoIterator<Item = (u32, GlyphQuadData)>,
) -> (HashMap<u32, Vec<GlyphQuadData>>, usize) {
    let mut page_quads: HashMap<u32, Vec<GlyphQuadData>> = HashMap::new();
    for (page_index, quad) in quads {
        page_quads.entry(page_index).or_default().push(quad);
    }
    let total_quads = page_quads.values().map(Vec::len).sum();
    (page_quads, total_quads)
}

const fn readiness_finished(readiness: GlyphReadiness) -> bool {
    matches!(
        readiness,
        GlyphReadiness::Ready | GlyphReadiness::Invisible | GlyphReadiness::Failed
    )
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
