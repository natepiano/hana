//! Typography overlay — renders font-level metric lines and per-glyph
//! bounding boxes as retained gizmos on any [`WorldText`] entity.
//!
//! Uses [`ComputedWorldText`] data populated by the renderer to ensure
//! exact alignment with the rendered MSDF quads — no independent layout
//! computation.
//!
//! Metric lines are drawn using Bevy's retained [`GizmoAsset`] (spawned
//! once, not redrawn every frame). Labels are spawned as [`WorldText`]
//! children.

use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;

use super::constants::ARROW_GAP_RATIO;
use super::constants::ARROW_SPACING_RATIO;
use super::constants::ARROWHEAD_RATIO;
use super::constants::CALLOUT_Z_OFFSET;
use super::constants::DEFAULT_LINE_WIDTH;
use super::constants::DOT_RADIUS_RATIO;
use super::constants::LABEL_ADVANCEMENT;
use super::constants::LABEL_ASCENT;
use super::constants::LABEL_BASELINE;
use super::constants::LABEL_BOTTOM;
use super::constants::LABEL_BOUNDING_BOX;
use super::constants::LABEL_CAP_HEIGHT;
use super::constants::LABEL_DESCENT;
use super::constants::LABEL_GAP_RATIO;
use super::constants::LABEL_LINE_HEIGHT;
use super::constants::LABEL_ORIGIN;
use super::constants::LABEL_SIZE_RATIO;
use super::constants::LABEL_TOP;
use super::constants::LABEL_X_HEIGHT;
use super::constants::METRIC_LINE_Z_OFFSET;
use super::constants::THICK_LINE_WIDTH;
use super::constants::THIN_LINE_WIDTH;
use crate::callouts;
use crate::default_panel_material;
use crate::layout::Anchor;
use crate::layout::Border;
use crate::layout::Direction;
use crate::layout::El;
use crate::layout::GlyphShadowMode;
use crate::layout::LayoutBuilder;
use crate::layout::Sizing;
use crate::layout::WorldTextStyle;
use crate::plugin::DiegeticPanel;
use crate::plugin::SurfaceShadow;
use crate::plugin::Unit;
use crate::plugin::UnitConfig;
use crate::render::ComputedWorldText;
use crate::render::LineMetricsSnapshot;
use crate::render::PendingGlyphs;
use crate::render::ShapedTextCache;
use crate::render::WorldText;
use crate::text::FontId;
use crate::text::FontMetrics;
use crate::text::FontRegistry;

/// Whether per-glyph bounding box annotations are visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bevy::prelude::Reflect)]
pub enum GlyphMetricVisibility {
    /// Glyph bounding boxes and origin dots are drawn.
    Shown,
    /// Glyph-level annotations are suppressed.
    Hidden,
}

/// Attach to a [`WorldText`] entity to render typography metric annotations.
/// Built into the library as a debug tool — only available when the
/// `typography_overlay` feature is enabled.
///
/// Metric lines are rendered as retained gizmos (spawned once, not
/// redrawn every frame). Labels are spawned as [`WorldText`] children.
///
/// # Example
///
/// ```ignore
/// commands.spawn((
///     WorldText::new("Typography"),
///     TextStyle::new(48.0),
///     TypographyOverlay::default(),
///     Transform::from_xyz(0.0, 2.0, 0.0),
/// ));
/// ```
#[derive(Component, Clone, Debug, bevy::prelude::Reflect)]
pub struct TypographyOverlay {
    /// Show font-level metric lines (ascent, descent, cap height, x-height,
    /// baseline, top, bottom).
    pub show_font_metrics:  GlyphMetricVisibility,
    /// Show per-glyph bounding boxes as gizmo lines (from font bbox).
    pub show_glyph_metrics: GlyphMetricVisibility,
    /// Show text labels on the metric lines.
    pub show_labels:        GlyphMetricVisibility,
    /// Color for overlay lines and labels (includes alpha).
    pub color:              Color,
    /// Gizmo line width in pixels.
    pub line_width:         f32,
    /// Font size for metric labels.
    pub label_size:         f32,
    /// How far annotation lines extend beyond text bounds (in layout units).
    pub extend:             f32,
}

impl Default for TypographyOverlay {
    fn default() -> Self {
        Self {
            show_font_metrics:  GlyphMetricVisibility::Shown,
            show_glyph_metrics: GlyphMetricVisibility::Shown,
            show_labels:        GlyphMetricVisibility::Shown,
            color:              Color::from(WHITE),
            line_width:         DEFAULT_LINE_WIDTH,
            label_size:         6.0,
            extend:             8.0,
        }
    }
}

/// Marker for the single container entity that holds all overlay children.
/// Spawned by [`on_overlay_added`] and despawned by [`on_overlay_removed`].
#[derive(Component)]
pub struct OverlayContainer;

/// Fired on the [`WorldText`] entity when its [`TypographyOverlay`] and
/// all descendant label text are fully rendered and interactable.
#[derive(EntityEvent)]
pub struct TypographyOverlayReady {
    /// The [`WorldText`] entity whose overlay is ready.
    pub entity: Entity,
}

/// Internal marker: overlay labels have been spawned, waiting for their
/// glyphs to finish and transforms to propagate.
#[derive(Component)]
pub struct AwaitingOverlayReady;

/// Observer: spawns an [`OverlayContainer`] child when
/// [`TypographyOverlay`] is added to an entity.
pub fn on_overlay_added(trigger: On<Add, TypographyOverlay>, mut commands: Commands) {
    commands.entity(trigger.entity).with_child((
        OverlayContainer,
        Transform::IDENTITY,
        Visibility::Inherited,
    ));
}

/// Observer: despawns the [`OverlayContainer`] child (and all its
/// descendants) when [`TypographyOverlay`] is removed from an entity.
pub fn on_overlay_removed(
    trigger: On<Remove, TypographyOverlay>,
    containers: Query<(Entity, &ChildOf), With<OverlayContainer>>,
    mut commands: Commands,
) {
    let parent = trigger.entity;
    for (container_entity, child_of) in &containers {
        if child_of.parent() == parent {
            commands.entity(container_entity).despawn();
        }
    }
}

/// System that builds the typography overlay when a [`TypographyOverlay`]
/// is first added or when the text/style changes.
///
/// Spawns retained gizmo lines and [`WorldText`] labels as children of
/// the overlay entity.
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
    font_registry: Res<FontRegistry>,
    cache: Res<ShapedTextCache>,
    unit_config: Res<UnitConfig>,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut dot_materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let changed_entities: Vec<Entity> = text_changed.iter().collect();

    for (entity, world_text, style, overlay, computed) in &query {
        if !changed_entities.contains(&entity) {
            continue;
        }
        if world_text.0.is_empty() {
            continue;
        }

        // Find the overlay container child for this entity.
        let Some(container_entity) = containers
            .iter()
            .find_map(|(e, child_of, _)| (child_of.parent() == entity).then_some(e))
        else {
            continue;
        };

        // Despawn previous overlay children, keeping the container.
        if let Some((_, _, Some(children))) =
            containers.iter().find(|(e, _, _)| *e == container_entity)
        {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }

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

        let unit_scale = style
            .world_scale()
            .or_else(|| style.unit().map(Unit::meters_per_unit))
            .unwrap_or_else(|| unit_config.world_font.meters_per_unit());
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

        if overlay.show_font_metrics == GlyphMetricVisibility::Shown {
            spawn_font_metric_gizmos(
                &mut commands,
                container_entity,
                font.name(),
                &font_metrics,
                &line_metrics,
                overlay,
                computed,
                anchor_y,
                font_size,
                scale,
                &mut gizmo_assets,
            );
        }

        if overlay.show_glyph_metrics == GlyphMetricVisibility::Shown {
            spawn_glyph_metric_gizmos(
                &mut commands,
                container_entity,
                &font_metrics,
                &line_metrics,
                overlay,
                computed,
                anchor_x,
                anchor_y,
                font_size,
                scale,
                &mut gizmo_assets,
                &mut meshes,
                &mut dot_materials,
            );
        }

        // Mark for deferred readiness check — label glyphs may still
        // need rasterization and transform propagation.
        commands.entity(entity).insert(AwaitingOverlayReady);
    }
}

/// Checks overlay label readiness and fires [`TypographyOverlayReady`]
/// once all descendant [`WorldText`] labels have no [`PendingGlyphs`].
/// Runs after `CalculateBounds` so transforms and AABBs are available.
pub fn emit_typography_overlay_ready(
    awaiting: Query<Entity, With<AwaitingOverlayReady>>,
    pending: Query<(), With<PendingGlyphs>>,
    children_query: Query<&Children>,
    mut commands: Commands,
) {
    for entity in &awaiting {
        let any_pending = children_query
            .iter_descendants(entity)
            .any(|d| pending.get(d).is_ok());
        if any_pending {
            continue;
        }
        commands.entity(entity).remove::<AwaitingOverlayReady>();
        commands
            .entity(entity)
            .trigger(|e| TypographyOverlayReady { entity: e });
    }
}

/// Spawns horizontal metric lines and dimension arrows for font-level metrics.
fn spawn_font_metric_gizmos(
    commands: &mut Commands,
    entity: Entity,
    font_name: &str,
    font_metrics: &FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    overlay: &TypographyOverlay,
    computed: &ComputedWorldText,
    anchor_y: f32,
    font_size: f32,
    scale: f32,
    _gizmo_assets: &mut Assets<GizmoAsset>,
) {
    let extents = GlyphExtents {
        first_left:    computed.glyph_rects.first().map_or(0.0, |r| r[0]),
        last_right:    computed.glyph_rects.last().map_or(0.0, |r| r[0] + r[2]),
        arrow_spacing: arrow_spacing(computed.first_advance),
    };

    let (_lines_gizmo, _arrows_gizmo, metric_lines) = build_metric_gizmos(
        font_metrics,
        line_metrics,
        overlay,
        anchor_y,
        &extents,
        font_size,
        scale,
    );

    spawn_metric_line_panel(
        commands,
        entity,
        overlay,
        font_metrics,
        line_metrics,
        &extents,
        anchor_y,
        font_size,
        scale,
    );

    spawn_metric_arrow_callouts(
        commands,
        entity,
        font_metrics,
        line_metrics,
        overlay,
        anchor_y,
        font_size,
        scale,
        &extents,
    );

    if overlay.show_labels == GlyphMetricVisibility::Shown {
        spawn_metric_labels(
            commands,
            entity,
            font_name,
            font_metrics,
            line_metrics,
            &metric_lines,
            overlay,
            anchor_y,
            font_size,
            scale,
            &extents,
        );
    }
}

/// Spawns horizontal font metric lines as a single transparent world panel.
fn spawn_metric_line_panel(
    commands: &mut Commands,
    entity: Entity,
    overlay: &TypographyOverlay,
    font_metrics: &FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    extents: &GlyphExtents,
    anchor_y: f32,
    font_size: f32,
    scale: f32,
) {
    let line_specs = metric_line_specs(font_metrics, line_metrics, overlay, anchor_y, scale);
    if line_specs.len() < 2 {
        return;
    }

    let width = extents.last_right - extents.first_left + 5.0 * extents.arrow_spacing;
    if width <= 0.0 {
        return;
    }

    let height = line_specs.last().map_or(0.0, |line| line.offset_y)
        - line_specs.first().map_or(0.0, |line| line.offset_y);
    if height <= 0.0 {
        return;
    }

    let border_width = metric_line_border_width(overlay, font_size, scale);
    let tree = build_metric_line_tree(width, height, &line_specs, border_width);

    let mut material = default_panel_material();
    material.base_color = Color::NONE;
    material.alpha_mode = AlphaMode::Blend;
    material.unlit = true;

    let x = extents.first_left - 3.0 * extents.arrow_spacing;
    let top_layout =
        if (line_metrics.top - (line_metrics.baseline - line_metrics.ascent)).abs() > 0.5 {
            line_metrics.top
        } else {
            line_metrics.baseline - line_metrics.ascent
        };
    let top_world = layout_to_world_y(top_layout, anchor_y, scale);

    commands.entity(entity).with_child((
        DiegeticPanel::world()
            .size(width, height)
            .anchor(Anchor::TopLeft)
            .surface_shadow(SurfaceShadow::On)
            .material(material)
            .with_tree(tree)
            .build()
            .expect("metric line panel uses valid dimensions"),
        Transform::from_xyz(x, top_world, METRIC_LINE_Z_OFFSET),
    ));
}

/// Spawns per-glyph bounding boxes, origin dots, and the advancement arrow.
fn spawn_glyph_metric_gizmos(
    commands: &mut Commands,
    entity: Entity,
    font_metrics: &FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    overlay: &TypographyOverlay,
    computed: &ComputedWorldText,
    anchor_x: f32,
    anchor_y: f32,
    font_size: f32,
    scale: f32,
    gizmo_assets: &mut Assets<GizmoAsset>,
    meshes: &mut Assets<Mesh>,
    dot_materials: &mut Assets<StandardMaterial>,
) {
    let bbox_color = Color::srgba(1.0, 1.0, 0.6, 0.7);
    spawn_glyph_box_panels(
        commands, entity, overlay, computed, bbox_color, font_size, scale,
    );

    // "Bounding Box" callout from the first glyph's bbox.
    if !computed.glyph_rects.is_empty() && overlay.show_labels == GlyphMetricVisibility::Shown {
        spawn_bounding_box_callout(
            commands,
            entity,
            font_metrics,
            line_metrics,
            computed,
            anchor_y,
            font_size,
            scale,
            bbox_color,
            gizmo_assets,
        );
    }

    // Origin dots + Advancement arrow below the first glyph.
    if !computed.glyph_rects.is_empty() && overlay.show_labels == GlyphMetricVisibility::Shown {
        spawn_origin_and_advancement(
            commands,
            entity,
            line_metrics,
            overlay,
            computed,
            anchor_x,
            anchor_y,
            font_size,
            scale,
            gizmo_assets,
            meshes,
            dot_materials,
        );
    }
}

/// Spawns one transparent bordered world panel per glyph bounding box.
fn spawn_glyph_box_panels(
    commands: &mut Commands,
    entity: Entity,
    overlay: &TypographyOverlay,
    computed: &ComputedWorldText,
    bbox_color: Color,
    font_size: f32,
    scale: f32,
) {
    let mut material = default_panel_material();
    material.base_color = Color::NONE;
    material.alpha_mode = AlphaMode::Blend;
    material.unlit = true;

    let border_width = bbox_border_width(overlay, font_size, scale);

    for &[x, y, w, h] in &computed.glyph_rects {
        if w <= 0.0 || h <= 0.0 {
            continue;
        }

        let mut builder = LayoutBuilder::new(w, h);
        builder.with(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::GROW)
                .border(Border::all(border_width, bbox_color)),
            |_| {},
        );
        let tree = builder.build();

        commands.entity(entity).with_child((
            DiegeticPanel::world()
                .size(w, h)
                .anchor(Anchor::Center)
                .surface_shadow(SurfaceShadow::On)
                .material(material.clone())
                .with_tree(tree)
                .build()
                .expect("glyph bounding box panels use valid dimensions"),
            Transform::from_xyz(x + w / 2.0, y - h / 2.0, CALLOUT_Z_OFFSET),
        ));
    }
}

/// Spawns the "Bounding Box" callout label with shelf and riser lines.
fn spawn_bounding_box_callout(
    commands: &mut Commands,
    entity: Entity,
    font_metrics: &FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    computed: &ComputedWorldText,
    anchor_y: f32,
    font_size: f32,
    scale: f32,
    bbox_color: Color,
    _gizmo_assets: &mut Assets<GizmoAsset>,
) {
    let label_size = font_scale(font_size, scale) * LABEL_SIZE_RATIO;
    let callout_thickness = font_scale(font_size, scale) * 0.0025;
    let z = CALLOUT_Z_OFFSET;

    let Some(last) = computed.glyph_rects.last() else {
        return;
    };
    let last_x = last[0];
    let last_y = last[1];
    let last_w = last[2];
    let last_h = last[3];

    // Shelf starts at right edge of last bbox, at vertical midpoint.
    let shelf_right_x = last_x + last_w;
    let shelf_y = last_y - last_h / 2.0;

    // Shelf extends rightward, then riser goes up. Label sits to the
    // left of the riser (CenterRight anchor) so it's always clear of
    // adjacent glyphs even when bounding boxes overlap.
    let shelf_len = arrow_spacing(computed.first_advance) / 2.0;
    let shelf_end_x = shelf_right_x + shelf_len;

    // Vertical line goes up to halfway between Cap Height and Ascent.
    let baseline_y_layout = line_metrics.baseline;
    let ascent_y_layout = baseline_y_layout - line_metrics.ascent;
    let cap_height_y_layout = baseline_y_layout - font_metrics.cap_height;
    let callout_top_layout = f32::midpoint(cap_height_y_layout, ascent_y_layout);
    let callout_top_world = layout_to_world_y(callout_top_layout, anchor_y, scale);

    callouts::spawn_callout_line(
        commands,
        entity,
        &callouts::CalloutLine::new(
            Vec3::new(shelf_right_x, shelf_y, z),
            Vec3::new(shelf_end_x, shelf_y, z),
        )
        .color(bbox_color)
        .thickness(callout_thickness),
    );
    callouts::spawn_callout_line(
        commands,
        entity,
        &callouts::CalloutLine::new(
            Vec3::new(shelf_end_x, shelf_y, z),
            Vec3::new(shelf_end_x, callout_top_world, z),
        )
        .color(bbox_color)
        .thickness(callout_thickness),
    );

    // Label at the top of the riser, to the left (CenterRight anchor).
    let ascent_mid_layout = f32::midpoint(cap_height_y_layout, ascent_y_layout);
    let ascent_mid_world = layout_to_world_y(ascent_mid_layout, anchor_y, scale);
    commands.entity(entity).with_child((
        WorldText::new(LABEL_BOUNDING_BOX),
        WorldTextStyle::new(label_size)
            .with_color(bbox_color)
            .with_anchor(Anchor::CenterRight)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(
            shelf_end_x - label_gap(font_size, scale),
            ascent_mid_world,
            z,
        ),
    ));
}

/// Spawns origin dots, origin label, advancement end dot, and advancement arrow.
fn spawn_origin_and_advancement(
    commands: &mut Commands,
    entity: Entity,
    line_metrics: &LineMetricsSnapshot,
    overlay: &TypographyOverlay,
    computed: &ComputedWorldText,
    anchor_x: f32,
    anchor_y: f32,
    font_size: f32,
    scale: f32,
    gizmo_assets: &mut Assets<GizmoAsset>,
    meshes: &mut Assets<Mesh>,
    dot_materials: &mut Assets<StandardMaterial>,
) {
    let callout_color = Color::srgb(0.9, 0.2, 0.2);
    let label_size = font_scale(font_size, scale) * LABEL_SIZE_RATIO;
    let z = CALLOUT_Z_OFFSET;
    let dot_radius = dot_radius(font_size, scale);

    let first = &computed.glyph_rects[0];
    let first_mid_x = first[0] + first[2] / 2.0;

    let baseline_world = layout_to_world_y(line_metrics.baseline, anchor_y, scale);
    let descent_world = layout_to_world_y(
        line_metrics.baseline + line_metrics.descent,
        anchor_y,
        scale,
    );

    let origin_x = layout_to_world_x(0.0, anchor_x, scale);
    let origin_y = baseline_world;

    // Origin dot — small filled circle at (origin, baseline).
    commands.entity(entity).with_child((
        Mesh3d(meshes.add(Circle::new(dot_radius))),
        MeshMaterial3d(dot_materials.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(origin_x, origin_y, z),
    ));

    // Origin label — centered between the bottom of the first
    // glyph's bbox and the Descent line.
    let first_bbox_bottom = first[1] - first[3];
    let origin_label_y = f32::midpoint(first_bbox_bottom, descent_world);

    // Callout line from just above the label toward the origin
    // dot, touching the circle edge. The label's cap height in
    // world units gives the visual top of the text.
    let label_ascent_world = line_metrics.ascent * LABEL_SIZE_RATIO * scale;
    let label_top_y = origin_label_y + label_ascent_world;
    let dx = origin_x - first_mid_x;
    let dy = origin_y - label_top_y;
    let len = dx.hypot(dy);
    let edge_x = (dx / len).mul_add(-dot_radius, origin_x);
    let edge_y = (dy / len).mul_add(-dot_radius, origin_y);
    callouts::spawn_callout_line(
        commands,
        entity,
        &callouts::CalloutLine::new(
            Vec3::new(edge_x, edge_y, z),
            Vec3::new(first_mid_x, label_top_y, z),
        )
        .color(callout_color)
        .thickness(callout_line_thickness(overlay, font_size, scale)),
    );
    commands.entity(entity).with_child((
        WorldText::new(LABEL_ORIGIN),
        WorldTextStyle::new(label_size)
            .with_color(overlay.color)
            .with_anchor(Anchor::Center)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(first_mid_x, origin_label_y, z),
    ));

    // Advancement end dot — filled circle at (origin + advance, baseline).
    let advance_end_x = origin_x + computed.first_advance;
    commands.entity(entity).with_child((
        Mesh3d(meshes.add(Circle::new(dot_radius))),
        MeshMaterial3d(dot_materials.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(advance_end_x, origin_y, z),
    ));

    // Advancement arrow — horizontal double-headed arrow below descent.
    let spacing = arrow_spacing(computed.first_advance);
    spawn_advancement_arrow(
        commands,
        entity,
        overlay,
        origin_x,
        origin_y,
        advance_end_x,
        descent_world,
        dot_radius,
        label_size,
        spacing,
        font_size,
        scale,
        z,
        gizmo_assets,
    );
}

/// Spawns the horizontal advancement arrow with tick lines and label.
fn spawn_advancement_arrow(
    commands: &mut Commands,
    entity: Entity,
    overlay: &TypographyOverlay,
    origin_x: f32,
    origin_y: f32,
    advance_end_x: f32,
    descent_world: f32,
    dot_radius: f32,
    label_size: f32,
    spacing: f32,
    font_size: f32,
    scale: f32,
    z: f32,
    gizmo_assets: &mut Assets<GizmoAsset>,
) {
    let arrow_y = descent_world - spacing;
    let head = arrowhead_size(font_size, scale);
    let gap = arrow_gap(font_size, scale);

    let mut adv_gizmo = GizmoAsset::default();

    // Dashed vertical bracket lines — from below the arrow to just
    // above the origin/advance dots on the baseline.
    let tick_above = dot_radius.mul_add(3.0, origin_y);
    let tick_below = arrow_y - head;
    let dash_len = spacing * 0.125;
    let gap_len = spacing * 0.125 / 2.0;
    callouts::draw_dashed_line(
        &mut adv_gizmo,
        Vec3::new(origin_x, tick_below, z),
        Vec3::new(origin_x, tick_above, z),
        dash_len,
        gap_len,
        overlay.color,
    );
    callouts::draw_dashed_line(
        &mut adv_gizmo,
        Vec3::new(advance_end_x, tick_below, z),
        Vec3::new(advance_end_x, tick_above, z),
        dash_len,
        gap_len,
        overlay.color,
    );

    // Horizontal dimension arrow.
    callouts::spawn_callout_line(
        commands,
        entity,
        &callouts::CalloutLine::new(
            Vec3::new(origin_x, arrow_y, z),
            Vec3::new(advance_end_x, arrow_y, z),
        )
        .color(overlay.color)
        .thickness(callout_line_thickness(overlay, font_size, scale))
        .surface_shadow(SurfaceShadow::On)
        .start_inset(gap)
        .end_inset(gap)
        .start_cap(
            callouts::CalloutCap::arrow()
                .solid()
                .length(head)
                .width(head),
        )
        .end_cap(
            callouts::CalloutCap::arrow()
                .solid()
                .length(head)
                .width(head),
        ),
    );

    commands.entity(entity).with_child((
        Gizmo {
            handle:      gizmo_assets.add(adv_gizmo),
            line_config: GizmoLineConfig {
                width: THICK_LINE_WIDTH,
                ..default()
            },
            depth_bias:  -0.1,
        },
        Transform::IDENTITY,
    ));

    // "Advancement" label centered below the arrow.
    let adv_mid_x = f32::midpoint(origin_x, advance_end_x);
    let adv_label_y = spacing.mul_add(-0.5, arrow_y);
    commands.entity(entity).with_child((
        WorldText::new(LABEL_ADVANCEMENT),
        WorldTextStyle::new(label_size)
            .with_color(overlay.color)
            .with_anchor(Anchor::TopCenter)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(adv_mid_x, adv_label_y, z),
    ));
}

/// Convert layout Y-down to world Y-up, with anchor offset.
fn layout_to_world_y(layout_y: f32, anchor_y: f32, scale: f32) -> f32 {
    -(layout_y - anchor_y) * scale
}

/// Convert layout X to world X, with anchor offset.
fn layout_to_world_x(layout_x: f32, anchor_x: f32, scale: f32) -> f32 {
    (layout_x - anchor_x) * scale
}

/// Computes the uniform spacing between arrow columns from the first
/// glyph's advance width.
const fn arrow_spacing(first_advance: f32) -> f32 { first_advance * ARROW_SPACING_RATIO }

/// Scale factor for converting font-size-relative ratios to world units.
fn font_scale(font_size: f32, scale: f32) -> f32 { font_size * scale }

/// Dot radius in world units, scaled to the font size.
fn dot_radius(font_size: f32, scale: f32) -> f32 { DOT_RADIUS_RATIO * font_scale(font_size, scale) }

/// Arrowhead line length in world units, scaled to the font size.
fn arrowhead_size(font_size: f32, scale: f32) -> f32 {
    ARROWHEAD_RATIO * font_scale(font_size, scale)
}

/// Arrow gap in world units, scaled to the font size.
fn arrow_gap(font_size: f32, scale: f32) -> f32 { ARROW_GAP_RATIO * font_scale(font_size, scale) }

/// Label gap in world units, scaled to the font size.
fn label_gap(font_size: f32, scale: f32) -> f32 { LABEL_GAP_RATIO * font_scale(font_size, scale) }

/// Border width for panel-backed glyph boxes in world units.
fn bbox_border_width(overlay: &TypographyOverlay, font_size: f32, scale: f32) -> f32 {
    let min_world = font_scale(font_size, scale) * 0.0025;
    let from_line_width = overlay.line_width.max(THIN_LINE_WIDTH) * min_world;
    from_line_width.max(min_world)
}

/// Thickness for panel-backed callout line segments in world units.
fn callout_line_thickness(overlay: &TypographyOverlay, font_size: f32, scale: f32) -> f32 {
    bbox_border_width(overlay, font_size, scale)
}

/// Border width for panel-backed horizontal metric lines in world units.
fn metric_line_border_width(overlay: &TypographyOverlay, font_size: f32, scale: f32) -> f32 {
    1.5 * bbox_border_width(overlay, font_size, scale)
}

/// Horizontal extents of the glyph run and the uniform arrow column spacing.
struct GlyphExtents {
    first_left:    f32,
    last_right:    f32,
    arrow_spacing: f32,
}

struct MetricLineSpec {
    offset_y: f32,
    color:    Color,
}

fn metric_line_specs(
    font_metrics: &FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    overlay: &TypographyOverlay,
    anchor_y: f32,
    scale: f32,
) -> Vec<MetricLineSpec> {
    let baseline_y = line_metrics.baseline;
    let ascent_y = baseline_y - line_metrics.ascent;
    let descent_y = baseline_y + line_metrics.descent;
    let top_y = line_metrics.top;
    let bottom_y = line_metrics.bottom;

    let include_top = (top_y - ascent_y).abs() > 0.5;
    let top_layout = if include_top { top_y } else { ascent_y };
    let top_world = layout_to_world_y(top_layout, anchor_y, scale);
    let offset = |layout_y: f32| top_world - layout_to_world_y(layout_y, anchor_y, scale);

    let mut specs = Vec::with_capacity(7);
    if include_top {
        specs.push(MetricLineSpec {
            offset_y: 0.0,
            color:    overlay.color,
        });
    }
    specs.push(MetricLineSpec {
        offset_y: offset(ascent_y),
        color:    overlay.color,
    });
    specs.push(MetricLineSpec {
        offset_y: offset(baseline_y - font_metrics.cap_height),
        color:    overlay.color,
    });
    specs.push(MetricLineSpec {
        offset_y: offset(baseline_y - font_metrics.x_height),
        color:    overlay.color,
    });
    specs.push(MetricLineSpec {
        offset_y: offset(baseline_y),
        color:    Color::srgb(0.9, 0.2, 0.2),
    });
    specs.push(MetricLineSpec {
        offset_y: offset(descent_y),
        color:    overlay.color,
    });
    if (bottom_y - descent_y).abs() > 0.5 {
        specs.push(MetricLineSpec {
            offset_y: offset(bottom_y),
            color:    overlay.color,
        });
    }
    specs
}

fn build_metric_line_tree(
    width: f32,
    height: f32,
    line_specs: &[MetricLineSpec],
    border_width: f32,
) -> crate::layout::LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .size(width, height)
            .direction(Direction::TopToBottom),
    );

    for (index, window) in line_specs.windows(2).enumerate() {
        let current = &window[0];
        let next = &window[1];
        let segment_h = next.offset_y - current.offset_y;
        if segment_h <= 0.0 {
            continue;
        }

        let mut border = Border::new().bottom(border_width).color(next.color);
        if index == 0 {
            border = border.top(border_width).color(current.color);
        }

        builder.with(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::fixed(segment_h))
                .border(border),
            |_| {},
        );
    }
    builder.build()
}

/// Builds gizmos for horizontal metric lines and dimension arrows.
/// Returns the lines gizmo, arrows gizmo, and the list of
/// `(label, layout_y)` pairs for label spawning.
fn build_metric_gizmos(
    font_metrics: &FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    overlay: &TypographyOverlay,
    anchor_y: f32,
    extents: &GlyphExtents,
    font_size: f32,
    scale: f32,
) -> (GizmoAsset, GizmoAsset, Vec<(&'static str, f32)>) {
    let mut lines_gizmo = GizmoAsset::default();
    let mut arrows_gizmo = GizmoAsset::default();
    let color = overlay.color;
    let z = METRIC_LINE_Z_OFFSET;
    let head = arrowhead_size(font_size, scale);
    let gap = arrow_gap(font_size, scale);

    let baseline_y = line_metrics.baseline;
    let ascent_y = baseline_y - line_metrics.ascent;
    let descent_y = baseline_y + line_metrics.descent;
    let top_y = line_metrics.top;
    let bottom_y = line_metrics.bottom;

    let mut metric_lines: Vec<(&str, f32)> = Vec::with_capacity(7);
    if (top_y - ascent_y).abs() > 0.5 {
        metric_lines.push((LABEL_TOP, top_y));
    }
    metric_lines.push((LABEL_ASCENT, ascent_y));
    metric_lines.push((LABEL_CAP_HEIGHT, baseline_y - font_metrics.cap_height));
    metric_lines.push((LABEL_X_HEIGHT, baseline_y - font_metrics.x_height));
    metric_lines.push((LABEL_BASELINE, baseline_y));
    metric_lines.push((LABEL_DESCENT, descent_y));
    if (bottom_y - descent_y).abs() > 0.5 {
        metric_lines.push((LABEL_BOTTOM, bottom_y));
    }

    // Left-side arrows grow leftward from the first glyph.
    // Right-side arrows grow rightward from the last glyph.
    // A full advance width separates Ascent/Descent from Line Height
    // so the Line Height shaft doesn't pass through their labels.
    // Metric lines extend one spacing past the outermost arrows.
    let left_outermost = 3.0_f32.mul_add(-extents.arrow_spacing, extents.first_left);
    let right_outermost = 2.0_f32.mul_add(extents.arrow_spacing, extents.last_right);
    let line_x0 = left_outermost;
    let line_x1 = right_outermost;

    let baseline_color = Color::srgb(0.9, 0.2, 0.2);
    for &(label, layout_y) in &metric_lines {
        let y = layout_to_world_y(layout_y, anchor_y, scale);
        let line_color = if label == LABEL_BASELINE {
            baseline_color
        } else {
            color
        };
        lines_gizmo.line(
            Vec3::new(line_x0, y, z),
            Vec3::new(line_x1, y, z),
            line_color,
        );
    }

    let ascent_world = layout_to_world_y(ascent_y, anchor_y, scale);
    let baseline_world = layout_to_world_y(baseline_y, anchor_y, scale);
    let descent_world = layout_to_world_y(descent_y, anchor_y, scale);

    // Left side: arrows grow outward from first glyph.
    // Ascent and Descent share the same column (they don't overlap vertically).
    // Line Height is a full advance width further left so its shaft
    // passes between the Ascent and Descent labels.
    let left_1 = extents.first_left - extents.arrow_spacing; // Ascent + Descent
    let left_2 = 3.0_f32.mul_add(-extents.arrow_spacing, extents.first_left); // Line Height

    let g = &mut arrows_gizmo;
    callouts::draw_dimension_arrow(
        g,
        Vec3::new(left_1, ascent_world, z),
        Vec3::new(left_1, baseline_world, z),
        color,
        head,
        gap,
    );
    callouts::draw_dimension_arrow(
        g,
        Vec3::new(left_1, baseline_world, z),
        Vec3::new(left_1, descent_world, z),
        color,
        head,
        gap,
    );
    callouts::draw_dimension_arrow(
        g,
        Vec3::new(left_2, ascent_world, z),
        Vec3::new(left_2, descent_world, z),
        color,
        head,
        gap,
    );

    // Right side: arrows grow outward from last glyph.
    let x_height_world = layout_to_world_y(baseline_y - font_metrics.x_height, anchor_y, scale);
    let cap_height_world = layout_to_world_y(baseline_y - font_metrics.cap_height, anchor_y, scale);

    let right_1 = extents.last_right + extents.arrow_spacing; // x-Height
    let right_2 = 2.0_f32.mul_add(extents.arrow_spacing, extents.last_right); // Cap Height

    callouts::draw_dimension_arrow(
        g,
        Vec3::new(right_1, x_height_world, z),
        Vec3::new(right_1, baseline_world, z),
        color,
        head,
        gap,
    );
    callouts::draw_dimension_arrow(
        g,
        Vec3::new(right_2, cap_height_world, z),
        Vec3::new(right_2, baseline_world, z),
        color,
        head,
        gap,
    );

    (lines_gizmo, arrows_gizmo, metric_lines)
}

fn spawn_metric_arrow_callouts(
    commands: &mut Commands,
    entity: Entity,
    font_metrics: &FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    overlay: &TypographyOverlay,
    anchor_y: f32,
    font_size: f32,
    scale: f32,
    extents: &GlyphExtents,
) {
    let baseline_y = line_metrics.baseline;
    let ascent_y = baseline_y - line_metrics.ascent;
    let descent_y = baseline_y + line_metrics.descent;
    let ascent_world = layout_to_world_y(ascent_y, anchor_y, scale);
    let baseline_world = layout_to_world_y(baseline_y, anchor_y, scale);
    let descent_world = layout_to_world_y(descent_y, anchor_y, scale);
    let x_height_world = layout_to_world_y(baseline_y - font_metrics.x_height, anchor_y, scale);
    let cap_height_world = layout_to_world_y(baseline_y - font_metrics.cap_height, anchor_y, scale);

    let left_1 = extents.first_left - extents.arrow_spacing;
    let left_2 = 3.0_f32.mul_add(-extents.arrow_spacing, extents.first_left);
    let right_1 = extents.last_right + extents.arrow_spacing;
    let right_2 = 2.0_f32.mul_add(extents.arrow_spacing, extents.last_right);
    let head = arrowhead_size(font_size, scale);
    let gap = arrow_gap(font_size, scale);
    let thickness = callout_line_thickness(overlay, font_size, scale);

    let baseline_color = Color::srgb(0.9, 0.2, 0.2);

    for (from, to, start_cap, end_cap) in [
        (
            Vec3::new(left_1, ascent_world, METRIC_LINE_Z_OFFSET),
            Vec3::new(left_1, baseline_world, METRIC_LINE_Z_OFFSET),
            callouts::CalloutCap::arrow()
                .solid()
                .length(head)
                .width(head),
            callouts::CalloutCap::arrow()
                .solid()
                .length(head)
                .width(head)
                .color(baseline_color),
        ),
        (
            Vec3::new(left_1, baseline_world, METRIC_LINE_Z_OFFSET),
            Vec3::new(left_1, descent_world, METRIC_LINE_Z_OFFSET),
            callouts::CalloutCap::arrow()
                .solid()
                .length(head)
                .width(head)
                .color(baseline_color),
            callouts::CalloutCap::arrow()
                .solid()
                .length(head)
                .width(head),
        ),
        (
            Vec3::new(left_2, ascent_world, METRIC_LINE_Z_OFFSET),
            Vec3::new(left_2, descent_world, METRIC_LINE_Z_OFFSET),
            callouts::CalloutCap::arrow()
                .solid()
                .length(head)
                .width(head),
            callouts::CalloutCap::arrow()
                .solid()
                .length(head)
                .width(head),
        ),
        (
            Vec3::new(right_1, x_height_world, METRIC_LINE_Z_OFFSET),
            Vec3::new(right_1, baseline_world, METRIC_LINE_Z_OFFSET),
            callouts::CalloutCap::arrow()
                .solid()
                .length(head)
                .width(head),
            callouts::CalloutCap::arrow()
                .solid()
                .length(head)
                .width(head)
                .color(baseline_color),
        ),
        (
            Vec3::new(right_2, cap_height_world, METRIC_LINE_Z_OFFSET),
            Vec3::new(right_2, baseline_world, METRIC_LINE_Z_OFFSET),
            callouts::CalloutCap::arrow()
                .solid()
                .length(head)
                .width(head),
            callouts::CalloutCap::arrow()
                .solid()
                .length(head)
                .width(head)
                .color(baseline_color),
        ),
    ] {
        callouts::spawn_callout_line(
            commands,
            entity,
            &callouts::CalloutLine::new(from, to)
                .color(overlay.color)
                .thickness(thickness)
                .surface_shadow(SurfaceShadow::On)
                .start_inset(gap)
                .end_inset(gap)
                .start_cap(start_cap)
                .end_cap(end_cap),
        );
    }
}

/// Spawns labels for metric lines and dimension arrows.
///
/// Left-side labels sit outside their arrows (`CenterRight` anchor).
/// Right-side labels sit outside their arrows (`CenterLeft` anchor).
fn spawn_metric_labels(
    commands: &mut Commands,
    parent: Entity,
    font_name: &str,
    font_metrics: &FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    metric_lines: &[(&str, f32)],
    overlay: &TypographyOverlay,
    anchor_y: f32,
    font_size: f32,
    scale: f32,
    extents: &GlyphExtents,
) {
    let label_size = font_scale(font_size, scale) * LABEL_SIZE_RATIO;
    let color = overlay.color;
    let z = METRIC_LINE_Z_OFFSET;
    let gap = label_gap(font_size, scale);

    let baseline_y = line_metrics.baseline;
    let ascent_y = baseline_y - line_metrics.ascent;
    let cap_height_y = baseline_y - font_metrics.cap_height;
    let x_height_y = baseline_y - font_metrics.x_height;
    let descent_y = baseline_y + line_metrics.descent;

    // Left-side arrow positions (match `build_metric_gizmos`).
    let left_1 = extents.first_left - extents.arrow_spacing; // Ascent + Descent
    let left_2 = 3.0_f32.mul_add(-extents.arrow_spacing, extents.first_left); // Line Height

    // Right-side arrow positions.
    let right_1 = extents.last_right + extents.arrow_spacing; // x-Height
    let right_2 = 2.0_f32.mul_add(extents.arrow_spacing, extents.last_right); // Cap Height

    spawn_line_edge_labels(
        commands,
        parent,
        metric_lines,
        anchor_y,
        label_size,
        color,
        z,
        left_2,
        gap,
        scale,
    );
    spawn_left_arrow_labels(
        commands,
        parent,
        line_metrics,
        font_name,
        anchor_y,
        label_size,
        color,
        z,
        gap,
        baseline_y,
        ascent_y,
        x_height_y,
        descent_y,
        left_1,
        left_2,
        scale,
    );
    spawn_right_arrow_labels(
        commands,
        parent,
        anchor_y,
        label_size,
        color,
        z,
        gap,
        baseline_y,
        cap_height_y,
        x_height_y,
        right_1,
        right_2,
        scale,
    );
}

/// Spawns Top/Bottom line-edge labels.
fn spawn_line_edge_labels(
    commands: &mut Commands,
    parent: Entity,
    metric_lines: &[(&str, f32)],
    anchor_y: f32,
    label_size: f32,
    color: Color,
    z: f32,
    label_x: f32,
    label_gap: f32,
    scale: f32,
) {
    for &(label, layout_y) in metric_lines {
        if label != LABEL_TOP && label != LABEL_BOTTOM {
            continue;
        }
        let line_world_y = layout_to_world_y(layout_y, anchor_y, scale);
        commands.entity(parent).with_child((
            WorldText::new(label),
            WorldTextStyle::new(label_size)
                .with_color(color)
                .with_anchor(Anchor::CenterRight)
                .with_shadow_mode(GlyphShadowMode::None),
            Transform::from_xyz(label_x - label_gap, line_world_y, z),
        ));
    }
}

/// Spawns Ascent, Descent, Line Height, and optional "no line gap" labels.
fn spawn_left_arrow_labels(
    commands: &mut Commands,
    parent: Entity,
    line_metrics: &LineMetricsSnapshot,
    font_name: &str,
    anchor_y: f32,
    label_size: f32,
    color: Color,
    z: f32,
    label_gap: f32,
    baseline_y: f32,
    ascent_y: f32,
    x_height_y: f32,
    descent_y: f32,
    left_1: f32,
    left_2: f32,
    scale: f32,
) {
    // Ascent label: halfway between Baseline and x-Height.
    let label_y_mid = f32::midpoint(baseline_y, x_height_y);
    let label_y_mid_world = layout_to_world_y(label_y_mid, anchor_y, scale);
    commands.entity(parent).with_child((
        WorldText::new(LABEL_ASCENT),
        WorldTextStyle::new(label_size)
            .with_color(color)
            .with_anchor(Anchor::CenterRight)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(left_1 - label_gap, label_y_mid_world, z),
    ));

    // Descent label: halfway between Baseline and Descent.
    let descent_mid = f32::midpoint(baseline_y, descent_y);
    let descent_mid_world = layout_to_world_y(descent_mid, anchor_y, scale);
    commands.entity(parent).with_child((
        WorldText::new(LABEL_DESCENT),
        WorldTextStyle::new(label_size)
            .with_color(color)
            .with_anchor(Anchor::CenterRight)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(left_1 - label_gap, descent_mid_world, z),
    ));

    // Line Height label: same vertical position as Ascent label.
    commands.entity(parent).with_child((
        WorldText::new(LABEL_LINE_HEIGHT),
        WorldTextStyle::new(label_size)
            .with_color(color)
            .with_anchor(Anchor::CenterRight)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(left_2 - label_gap, label_y_mid_world, z),
    ));

    // Baseline label: on the baseline, underneath Line Height.
    // Offset down by half the label's descent so the visual center
    // of the text (not the bounding box center) sits on the red line.
    let label_descent_offset = line_metrics.descent * LABEL_SIZE_RATIO * scale / 2.0;
    let baseline_label_world =
        layout_to_world_y(baseline_y, anchor_y, scale) - label_descent_offset;
    commands.entity(parent).with_child((
        WorldText::new(LABEL_BASELINE),
        WorldTextStyle::new(label_size)
            .with_color(color)
            .with_anchor(Anchor::CenterRight)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(left_2 - label_gap, baseline_label_world, z),
    ));

    // "no line gap" annotation when Top == Ascent.
    let has_line_gap =
        (line_metrics.top - (line_metrics.baseline - line_metrics.ascent)).abs() > 0.5;
    if !has_line_gap {
        let ascent_world = layout_to_world_y(ascent_y, anchor_y, scale);
        let no_gap_label = format!("no line gap for {font_name}");
        commands.entity(parent).with_child((
            WorldText::new(no_gap_label),
            WorldTextStyle::new(label_size)
                .with_color(color)
                .with_anchor(Anchor::BottomLeft)
                .with_shadow_mode(GlyphShadowMode::None),
            Transform::from_xyz(left_2, ascent_world, z),
        ));
    }
}

/// Spawns x-Height and Cap Height labels on the right side.
fn spawn_right_arrow_labels(
    commands: &mut Commands,
    parent: Entity,
    anchor_y: f32,
    label_size: f32,
    color: Color,
    z: f32,
    label_gap: f32,
    baseline_y: f32,
    cap_height_y: f32,
    x_height_y: f32,
    right_1: f32,
    right_2: f32,
    scale: f32,
) {
    // x-Height label: halfway between x-Height and Baseline.
    let x_height_mid = f32::midpoint(x_height_y, baseline_y);
    let x_height_mid_world = layout_to_world_y(x_height_mid, anchor_y, scale);
    commands.entity(parent).with_child((
        WorldText::new(LABEL_X_HEIGHT),
        WorldTextStyle::new(label_size)
            .with_color(color)
            .with_anchor(Anchor::CenterLeft)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(right_1 + label_gap, x_height_mid_world, z),
    ));

    // Cap Height label: halfway between Cap Height and x-Height.
    let cap_mid = f32::midpoint(cap_height_y, x_height_y);
    let cap_mid_world = layout_to_world_y(cap_mid, anchor_y, scale);
    commands.entity(parent).with_child((
        WorldText::new(LABEL_CAP_HEIGHT),
        WorldTextStyle::new(label_size)
            .with_color(color)
            .with_anchor(Anchor::CenterLeft)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(right_2 + label_gap, cap_mid_world, z),
    ));
}
