use bevy::prelude::*;

use super::GlyphMetricVisibility;
use super::OverlayContainer;
use super::TypographyOverlay;
use super::glyph;
use super::metric_lines;
use crate::cascade::CascadeDefault;
use crate::cascade::FontUnit;
use crate::cascade::Resolved;
use crate::layout::LineMetricsSnapshot;
use crate::layout::MeasureTextFn;
use crate::layout::ShapedTextCache;
use crate::layout::Unit;
use crate::layout::WorldTextStyle;
use crate::render::ComputedWorldText;
use crate::render::TextContent;
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
    pub(super) anchor_y:  f32,
    pub(super) font_size: f32,
    pub(super) scale:     f32,
}

/// Font-level metrics shared by helpers that draw glyph-level or line-level
/// guides. Exists to reduce helper parameter counts.
pub(super) struct FontContext<'a> {
    pub(super) font: &'a FontMetrics,
    pub(super) line: &'a LineMetricsSnapshot,
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
        &TextContent,
        &WorldTextStyle,
        &TypographyOverlay,
        &ComputedWorldText,
    )>,
    text_changed: Query<
        Entity,
        (
            With<TypographyOverlay>,
            Or<(
                Added<TypographyOverlay>,
                Changed<TypographyOverlay>,
                Changed<TextContent>,
                Changed<WorldTextStyle>,
                Changed<ComputedWorldText>,
            )>,
        ),
    >,
    containers: Query<(Entity, &ChildOf, Option<&Children>), With<OverlayContainer>>,
    resolved_units: Query<&Resolved<FontUnit>>,
    font_registry: Res<FontRegistry>,
    mut cache: ResMut<ShapedTextCache>,
    font_default: Res<CascadeDefault<FontUnit>>,
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
        if world_text.text().is_empty() {
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

        // Read the per-entity `Resolved<FontUnit>`, falling back to
        // `CascadeDefault<FontUnit>`.
        let unit_scale = resolved_units
            .get(entity)
            .map_or(font_default.0, |resolved| resolved.0)
            .0
            .meters_per_unit();
        let scale = unit_scale * points_to_world;
        let anchor_y = if scale > 0.0 {
            computed.anchor_y / scale
        } else {
            0.0
        };

        let measure = style.as_layout_config().scaled(boost).as_measure();
        let Some(line_metrics) = cache
            .get_shaped(world_text.text(), &measure)
            .and_then(|s| s.line_metrics.first().copied())
        else {
            continue;
        };

        let mut ctx = OverlayContext {
            commands: &mut commands,
            entity: container_entity,
            overlay,
            anchor_y,
            font_size,
            scale,
        };
        let font_context = FontContext {
            font: &font_metrics,
            line: &line_metrics,
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
            metric_lines::spawn_font_metric_gizmos(
                &mut ctx,
                font.name(),
                &font_context,
                computed,
                &mut text_services,
                &mut assets,
            );
        }

        if overlay.glyph_metrics == GlyphMetricVisibility::Shown {
            glyph::spawn_glyph_metric_gizmos(&mut ctx, &font_context, computed, &mut assets);
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
