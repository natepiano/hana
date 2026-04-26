use bevy::prelude::*;

use super::AwaitingOverlayReady;
use super::GlyphMetricVisibility;
use super::OverlayContainer;
use super::TypographyOverlay;
use super::glyph;
use super::metric_lines;
use crate::cascade::CascadeDefaults;
use crate::cascade::CascadeTarget;
use crate::cascade::Resolved;
use crate::layout::LineMetricsSnapshot;
use crate::layout::MeasureTextFn;
use crate::layout::ShapedTextCache;
use crate::layout::Unit;
use crate::layout::WorldTextStyle;
use crate::render::ComputedWorldText;
use crate::render::PendingGlyphs;
use crate::render::WorldFontUnit;
use crate::render::WorldText;
use crate::text;
use crate::text::FontId;
use crate::text::FontMetrics;
use crate::text::FontRegistry;

/// Shared overlay parameters that every spawn helper threads through.
/// Exists to keep helper argument lists under the "context struct when > 7
/// parameters" style threshold.
pub(super) struct OverlayContext<'w, 's, 'a> {
    pub(super) commands:  &'a mut Commands<'w, 's>,
    pub(super) entity:    Entity,
    pub(super) overlay:   &'a TypographyOverlay,
    pub(super) anchor_x:  f32,
    pub(super) anchor_y:  f32,
    pub(super) font_size: f32,
    pub(super) scale:     f32,
}

/// Font-level metrics shared by helpers that draw glyph-level or line-level
/// guides. Exists to reduce helper parameter counts.
pub(super) struct FontContext<'a> {
    pub(super) font_metrics: &'a FontMetrics,
    pub(super) line_metrics: &'a LineMetricsSnapshot,
}

/// Asset store handles for overlay mesh/material spawning. Exists to reduce
/// helper parameter counts.
pub(super) struct OverlayAssets<'a> {
    pub(super) meshes:    &'a mut Assets<Mesh>,
    pub(super) materials: &'a mut Assets<StandardMaterial>,
}

/// Text-shaping services (measurer + cache) passed to helpers that measure
/// overlay labels. Exists to reduce helper parameter counts.
pub(super) struct TextServices<'a> {
    pub(super) measure_text: &'a MeasureTextFn,
    pub(super) cache:        &'a mut ShapedTextCache,
}

/// Horizontal extents of the glyph run and the uniform arrow column spacing.
pub(super) struct GlyphExtents {
    pub(super) first_left:    f32,
    pub(super) last_right:    f32,
    pub(super) arrow_spacing: f32,
}

pub fn build_typography_overlay(
    query: Query<(
        Entity,
        &WorldText,
        &WorldTextStyle,
        &TypographyOverlay,
        &ComputedWorldText,
    )>,
    text_changed: Query<
        Entity,
        (
            With<TypographyOverlay>,
            Without<PendingGlyphs>,
            Or<(
                Added<TypographyOverlay>,
                Changed<TypographyOverlay>,
                Changed<WorldText>,
                Changed<WorldTextStyle>,
                Changed<ComputedWorldText>,
            )>,
        ),
    >,
    containers: Query<(Entity, &ChildOf, Option<&Children>), With<OverlayContainer>>,
    resolved_units: Query<&Resolved<WorldFontUnit>>,
    font_registry: Res<FontRegistry>,
    mut cache: ResMut<ShapedTextCache>,
    defaults: Res<CascadeDefaults>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut dot_materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let changed_entities: Vec<Entity> = text_changed.iter().collect();
    let measure_text =
        text::create_parley_measurer(font_registry.font_context(), font_registry.family_names());

    for (entity, world_text, style, overlay, computed) in &query {
        if !changed_entities.contains(&entity) {
            continue;
        }
        if world_text.0.is_empty() {
            continue;
        }

        let Some(container_entity) = overlay_container_entity(&containers, entity) else {
            continue;
        };

        despawn_overlay_children(&mut commands, &containers, container_entity);

        let font_id = FontId(style.font_id());
        let Some(font) = font_registry.font(font_id) else {
            continue;
        };

        // Standalone world text is shaped in boosted point space, then the
        // renderer scales the result back to world meters. Use the same
        // boosted measure here so the overlay sees the same line metrics.
        let points_to_world = Unit::Points.meters_per_unit();
        let boost = if points_to_world > 0.0 {
            1.0 / points_to_world
        } else {
            1.0
        };
        let font_size = style.size() * boost;
        let font_metrics = font.metrics(font_size);

        // `world_scale` is a raw meters-per-unit override that bypasses the
        // cascade. Otherwise read the per-entity `Resolved<WorldFontUnit>`,
        // falling back to `CascadeDefaults.world_font_unit`.
        let unit_scale = style.world_scale().unwrap_or_else(|| {
            resolved_units
                .get(entity)
                .map_or_else(
                    |_| WorldFontUnit::global_default(&defaults),
                    |resolved| resolved.0,
                )
                .0
                .meters_per_unit()
        });
        let scale = unit_scale * points_to_world;
        let anchor_x = if scale > 0.0 {
            computed.anchor_x / scale
        } else {
            0.0
        };
        let anchor_y = if scale > 0.0 {
            computed.anchor_y / scale
        } else {
            0.0
        };

        let measure = style.as_layout_config().scaled(boost).as_measure();
        let Some(line_metrics) = cache
            .get_shaped(&world_text.0, &measure)
            .and_then(|s| s.line_metrics.first().copied())
        else {
            continue;
        };

        let mut ctx = OverlayContext {
            commands: &mut commands,
            entity: container_entity,
            overlay,
            anchor_x,
            anchor_y,
            font_size,
            scale,
        };
        let font_ctx = FontContext {
            font_metrics: &font_metrics,
            line_metrics: &line_metrics,
        };
        let mut assets = OverlayAssets {
            meshes:    &mut meshes,
            materials: &mut dot_materials,
        };
        let mut text_services = TextServices {
            measure_text: &measure_text,
            cache:        &mut cache,
        };

        if overlay.font_metrics == GlyphMetricVisibility::Shown {
            let bounds_target = metric_lines::spawn_font_metric_gizmos(
                &mut ctx,
                font.name(),
                &font_ctx,
                computed,
                &mut text_services,
                &mut assets,
            );

            // Mark for deferred readiness check — label glyphs may still
            // need rasterization and transform propagation.
            ctx.commands.entity(entity).insert(AwaitingOverlayReady {
                ready_target: bounds_target,
            });
        }

        if overlay.glyph_metrics == GlyphMetricVisibility::Shown {
            glyph::spawn_glyph_metric_gizmos(&mut ctx, &font_ctx, computed, &mut assets);
        }

        if overlay.font_metrics != GlyphMetricVisibility::Shown {
            ctx.commands.entity(entity).insert(AwaitingOverlayReady {
                ready_target: container_entity,
            });
        }
    }
}

fn overlay_container_entity(
    containers: &Query<(Entity, &ChildOf, Option<&Children>), With<OverlayContainer>>,
    entity: Entity,
) -> Option<Entity> {
    containers.iter().find_map(|(child_entity, child_of, _)| {
        (child_of.parent() == entity).then_some(child_entity)
    })
}

fn despawn_overlay_children(
    commands: &mut Commands,
    containers: &Query<(Entity, &ChildOf, Option<&Children>), With<OverlayContainer>>,
    container_entity: Entity,
) {
    if let Some((_, _, Some(children))) = containers
        .iter()
        .find(|(entity, _, _)| *entity == container_entity)
    {
        for child in children {
            commands.entity(*child).despawn();
        }
    }
}
