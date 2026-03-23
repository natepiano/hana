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

use crate::layout::GlyphShadowMode;
use crate::layout::TextAnchor;
use crate::layout::TextStyle;
use crate::render::ComputedWorldText;
use crate::render::LineMetricsSnapshot;
use crate::render::ShapedTextCache;
use crate::render::WorldText;
use crate::text::FontId;
use crate::text::FontRegistry;

/// Layout-units-to-world-units conversion factor.
const LAYOUT_TO_WORLD: f32 = 0.01;

/// Default line width for overlay gizmos (in pixels).
const DEFAULT_LINE_WIDTH: f32 = 0.5;

/// Line width for metric lines, bounding boxes, and callout backgrounds.
const THIN_LINE_WIDTH: f32 = 1.0;

/// Line width for arrows, callout lines, and arrow points.
const THICK_LINE_WIDTH: f32 = 2.5;

/// Gap between arrow tips and the metric lines they point at (world units).
const ARROW_GAP: f32 = 0.006;

/// Font size for metric labels relative to the text's font size.
/// Apple's reference diagram uses labels roughly 1/10th the display size.
const LABEL_SIZE_RATIO: f32 = 0.08;

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
///     TextStyle::new().with_size(48.0),
///     TypographyOverlay::default(),
///     Transform::from_xyz(0.0, 2.0, 0.0),
/// ));
/// ```
#[derive(Component, Clone, Debug, bevy::prelude::Reflect)]
pub struct TypographyOverlay {
    /// Show font-level metric lines (ascent, descent, cap height, x-height,
    /// baseline, top, bottom).
    pub show_font_metrics:  bool,
    /// Show per-glyph bounding boxes as gizmo lines (from font bbox).
    pub show_glyph_metrics: bool,
    /// Show text labels on the metric lines.
    pub show_labels:        bool,
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
            show_font_metrics:  true,
            show_glyph_metrics: true,
            show_labels:        true,
            color:              Color::from(WHITE),
            line_width:         DEFAULT_LINE_WIDTH,
            label_size:         6.0,
            extend:             8.0,
        }
    }
}

/// Marker for child entities spawned by the typography overlay.
/// Used to despawn/rebuild overlay elements when the text changes.
#[derive(Component)]
pub struct OverlayElement;

/// System that builds the typography overlay when a [`TypographyOverlay`]
/// is first added or when the text/style changes.
///
/// Spawns retained gizmo lines and [`WorldText`] labels as children of
/// the overlay entity.
#[allow(clippy::type_complexity)]
pub fn build_typography_overlay(
    query: Query<(
        Entity,
        &WorldText,
        &TextStyle,
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
                Changed<WorldText>,
                Changed<TextStyle>,
                Changed<ComputedWorldText>,
            )>,
        ),
    >,
    old_elements: Query<(Entity, &ChildOf), With<OverlayElement>>,
    font_registry: Res<FontRegistry>,
    cache: Res<ShapedTextCache>,
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

        // Despawn previous overlay elements.
        for (elem_entity, child_of) in &old_elements {
            if child_of.parent() == entity {
                commands.entity(elem_entity).despawn();
            }
        }

        let font_id = FontId(style.font_id());
        let Some(font) = font_registry.font(font_id) else {
            continue;
        };

        let font_size = style.size();
        let font_metrics = font.metrics(font_size);

        let anchor_x = computed.anchor_x;
        let anchor_y = computed.anchor_y;
        let text_width = computed.text_width;

        let measure = style.as_layout_config().as_measure();
        let Some(line_metrics) = cache
            .get_shaped(&world_text.0, &measure)
            .and_then(|s| s.line_metrics.first().copied())
        else {
            continue;
        };

        // Build retained gizmo with all metric lines.
        if overlay.show_font_metrics {
            // Left X of the first glyph bounding box in world units.
            let first_glyph_left = computed.glyph_rects.first().map_or(0.0, |r| r[0]);
            let last_glyph_right = computed.glyph_rects.last().map_or(0.0, |r| r[0] + r[2]);
            let line_left = layout_to_world_x(-overlay.extend * 2.5, anchor_x);
            let line_right = layout_to_world_x(text_width + overlay.extend * 2.5, anchor_x);

            // Gap between right-side arrows: advance width minus bbox width
            // of the last glyph. Computed once, reused for both arrows.
            let last_bbox_width = computed.glyph_rects.last().map_or(0.0, |r| r[2]);
            let advance_minus_bbox = computed.first_advance - last_bbox_width;

            let (lines_gizmo, arrows_gizmo, metric_lines) = build_metric_gizmos(
                &font_metrics,
                &line_metrics,
                text_width,
                overlay,
                anchor_x,
                anchor_y,
                line_left,
                first_glyph_left,
                last_glyph_right,
                line_right,
                advance_minus_bbox,
            );

            // Horizontal metric lines.
            commands.entity(entity).with_child((
                OverlayElement,
                Gizmo {
                    handle:      gizmo_assets.add(lines_gizmo),
                    line_config: GizmoLineConfig {
                        width: THIN_LINE_WIDTH,
                        ..default()
                    },
                    depth_bias:  -0.1,
                },
                Transform::IDENTITY,
            ));

            // Dimension arrows (thicker).
            commands.entity(entity).with_child((
                OverlayElement,
                Gizmo {
                    handle:      gizmo_assets.add(arrows_gizmo),
                    line_config: GizmoLineConfig {
                        width: THICK_LINE_WIDTH,
                        ..default()
                    },
                    depth_bias:  -0.1,
                },
                Transform::IDENTITY,
            ));

            if overlay.show_labels {
                spawn_metric_labels(
                    &mut commands,
                    entity,
                    font.name(),
                    &font_metrics,
                    &line_metrics,
                    &metric_lines,
                    overlay,
                    anchor_x,
                    anchor_y,
                    font_size,
                    line_left,
                    first_glyph_left,
                    last_glyph_right,
                    advance_minus_bbox,
                    &mut gizmo_assets,
                );
            }
        }

        // Per-glyph bounding boxes from the font bbox.
        if overlay.show_glyph_metrics {
            let bbox_color = Color::srgba(1.0, 1.0, 0.6, 0.7);
            let glyph_gizmo = build_glyph_box_gizmo(&computed.glyph_rects, bbox_color);

            commands.entity(entity).with_child((
                OverlayElement,
                Gizmo {
                    handle:      gizmo_assets.add(glyph_gizmo),
                    line_config: GizmoLineConfig {
                        width: THIN_LINE_WIDTH,
                        ..default()
                    },
                    depth_bias:  -0.1,
                },
                Transform::IDENTITY,
            ));

            // "Bounding Box" callout from the first glyph's bbox.
            if !computed.glyph_rects.is_empty() && overlay.show_labels {
                let bbox_color = Color::srgba(1.0, 1.0, 0.6, 0.7);
                let label_size = font_size * LABEL_SIZE_RATIO;
                let z = 0.002;

                let first = &computed.glyph_rects[0];
                let first_x = first[0];
                let first_y = first[1];
                let first_w = first[2];
                let first_h = first[3];

                // Shelf starts at right edge of first bbox, at vertical midpoint.
                let shelf_left_x = first_x + first_w;
                let shelf_y = first_y - first_h / 2.0;

                // Shelf extends rightward by half the gap to the second glyph.
                let shelf_len = if computed.glyph_rects.len() >= 2 {
                    let second_x = computed.glyph_rects[1][0];
                    (second_x - shelf_left_x) / 2.0
                } else {
                    0.01
                };
                let shelf_right_x = shelf_left_x + shelf_len;

                // Vertical line goes up to halfway between Cap Height and Ascent.
                let baseline_y_layout = line_metrics.baseline;
                let ascent_y_layout = baseline_y_layout - line_metrics.ascent;
                let cap_height_y_layout = baseline_y_layout - font_metrics.cap_height;
                let callout_top_layout = f32::midpoint(cap_height_y_layout, ascent_y_layout);
                let callout_top_world = layout_to_world_y(callout_top_layout, anchor_y);

                let mut callout_gizmo = GizmoAsset::default();
                // Horizontal shelf.
                callout_gizmo.line(
                    Vec3::new(shelf_left_x, shelf_y, z),
                    Vec3::new(shelf_right_x, shelf_y, z),
                    bbox_color,
                );
                // Vertical riser.
                callout_gizmo.line(
                    Vec3::new(shelf_right_x, shelf_y, z),
                    Vec3::new(shelf_right_x, callout_top_world, z),
                    bbox_color,
                );

                commands.entity(entity).with_child((
                    OverlayElement,
                    Gizmo {
                        handle:      gizmo_assets.add(callout_gizmo),
                        line_config: GizmoLineConfig {
                            width: THIN_LINE_WIDTH,
                            ..default()
                        },
                        depth_bias:  -0.1,
                    },
                    Transform::IDENTITY,
                ));

                // Label at the top of the riser, same height as Ascent label.
                let ascent_mid_layout = f32::midpoint(cap_height_y_layout, ascent_y_layout);
                let ascent_mid_world = layout_to_world_y(ascent_mid_layout, anchor_y);
                commands.entity(entity).with_child((
                    OverlayElement,
                    WorldText::new("Bounding Box"),
                    TextStyle::new()
                        .with_size(label_size)
                        .with_color(bbox_color)
                        .with_anchor(TextAnchor::CenterLeft)
                        .with_shadow_mode(GlyphShadowMode::None),
                    Transform::from_xyz(shelf_right_x + 0.01, ascent_mid_world, z),
                ));
            }

            // Origin dots + Advancement arrow below the first glyph.
            if !computed.glyph_rects.is_empty() && overlay.show_labels {
                let callout_color = Color::srgb(0.9, 0.2, 0.2);
                let label_size = font_size * LABEL_SIZE_RATIO;
                let z = 0.002;
                let dot_radius = 0.003;

                let first = &computed.glyph_rects[0];
                let first_mid_x = first[0] + first[2] / 2.0;

                let baseline_world = layout_to_world_y(line_metrics.baseline, anchor_y);
                let descent_world =
                    layout_to_world_y(line_metrics.baseline + line_metrics.descent, anchor_y);

                let origin_x = layout_to_world_x(0.0, anchor_x);
                let origin_y = baseline_world;

                // Origin dot — small filled circle at (origin, baseline).
                commands.entity(entity).with_child((
                    OverlayElement,
                    Mesh3d(meshes.add(Circle::new(dot_radius))),
                    MeshMaterial3d(dot_materials.add(StandardMaterial {
                        base_color: Color::WHITE,
                        unlit: true,
                        ..default()
                    })),
                    Transform::from_xyz(origin_x, origin_y, z),
                ));

                // Origin label — halfway between the bottom of the first
                // glyph's bbox and the descent line.
                let first_bbox_bottom = first[1] - first[3];
                let origin_label_y = (first_bbox_bottom + descent_world) / 2.0;
                let mut origin_callout = GizmoAsset::default();
                origin_callout.line(
                    Vec3::new(origin_x, origin_y - dot_radius, z),
                    Vec3::new(first_mid_x, origin_label_y + 0.005, z),
                    callout_color,
                );
                commands.entity(entity).with_child((
                    OverlayElement,
                    Gizmo {
                        handle:      gizmo_assets.add(origin_callout),
                        line_config: GizmoLineConfig {
                            width: THICK_LINE_WIDTH,
                            ..default()
                        },
                        depth_bias:  -0.1,
                    },
                    Transform::IDENTITY,
                ));
                commands.entity(entity).with_child((
                    OverlayElement,
                    WorldText::new("Origin"),
                    TextStyle::new()
                        .with_size(label_size)
                        .with_color(overlay.color)
                        .with_anchor(TextAnchor::TopCenter)
                        .with_shadow_mode(GlyphShadowMode::None),
                    Transform::from_xyz(first_mid_x, origin_label_y, z),
                ));

                // Advancement end dot — filled circle at (origin + advance, baseline).
                let advance_end_x = origin_x + computed.first_advance;
                commands.entity(entity).with_child((
                    OverlayElement,
                    Mesh3d(meshes.add(Circle::new(dot_radius))),
                    MeshMaterial3d(dot_materials.add(StandardMaterial {
                        base_color: Color::WHITE,
                        unlit: true,
                        ..default()
                    })),
                    Transform::from_xyz(advance_end_x, origin_y, z),
                ));

                // Advancement arrow — horizontal double-headed arrow below descent.
                let arrow_y = descent_world - 0.03;
                let arrow_size = 0.005;
                let arrow_gap = ARROW_GAP;

                let mut adv_gizmo = GizmoAsset::default();

                // Vertical tick lines — from below the arrow to just above
                // the origin/advance dots on the baseline.
                let tick_above = origin_y + dot_radius * 3.0;
                adv_gizmo.line(
                    Vec3::new(origin_x, arrow_y - 0.005, z),
                    Vec3::new(origin_x, tick_above, z),
                    overlay.color,
                );
                adv_gizmo.line(
                    Vec3::new(advance_end_x, arrow_y - 0.005, z),
                    Vec3::new(advance_end_x, tick_above, z),
                    overlay.color,
                );

                // Horizontal line with arrowheads.
                let left_tip = origin_x + arrow_gap;
                let right_tip = advance_end_x - arrow_gap;
                adv_gizmo.line(
                    Vec3::new(left_tip, arrow_y, z),
                    Vec3::new(right_tip, arrow_y, z),
                    overlay.color,
                );
                // Left arrowhead.
                adv_gizmo.line(
                    Vec3::new(left_tip, arrow_y, z),
                    Vec3::new(left_tip + arrow_size, arrow_y + arrow_size, z),
                    overlay.color,
                );
                adv_gizmo.line(
                    Vec3::new(left_tip, arrow_y, z),
                    Vec3::new(left_tip + arrow_size, arrow_y - arrow_size, z),
                    overlay.color,
                );
                // Right arrowhead.
                adv_gizmo.line(
                    Vec3::new(right_tip, arrow_y, z),
                    Vec3::new(right_tip - arrow_size, arrow_y + arrow_size, z),
                    overlay.color,
                );
                adv_gizmo.line(
                    Vec3::new(right_tip, arrow_y, z),
                    Vec3::new(right_tip - arrow_size, arrow_y - arrow_size, z),
                    overlay.color,
                );

                commands.entity(entity).with_child((
                    OverlayElement,
                    Gizmo {
                        handle:      gizmo_assets.add(adv_gizmo),
                        line_config: GizmoLineConfig {
                            width: THIN_LINE_WIDTH,
                            ..default()
                        },
                        depth_bias:  -0.1,
                    },
                    Transform::IDENTITY,
                ));

                // "Advancement" label centered below the arrow.
                let adv_mid_x = (origin_x + advance_end_x) / 2.0;
                let adv_label_y = arrow_y - 0.01;
                commands.entity(entity).with_child((
                    OverlayElement,
                    WorldText::new("Advancement"),
                    TextStyle::new()
                        .with_size(label_size)
                        .with_color(overlay.color)
                        .with_anchor(TextAnchor::TopCenter)
                        .with_shadow_mode(GlyphShadowMode::None),
                    Transform::from_xyz(adv_mid_x, adv_label_y, z),
                ));
            }
        }
    }
}

/// Builds a gizmo with per-glyph bounding box rectangles.
fn build_glyph_box_gizmo(glyph_rects: &[[f32; 4]], color: Color) -> GizmoAsset {
    let mut gizmo = GizmoAsset::default();

    for &[x, y, w, h] in glyph_rects {
        let tl = Vec3::new(x, y, 0.002);
        let tr = Vec3::new(x + w, y, 0.002);
        let br = Vec3::new(x + w, y - h, 0.002);
        let bl = Vec3::new(x, y - h, 0.002);

        gizmo.line(tl, tr, color);
        gizmo.line(tr, br, color);
        gizmo.line(br, bl, color);
        gizmo.line(bl, tl, color);
    }

    gizmo
}

/// Convert layout Y-down to world Y-up, with anchor offset.
fn layout_to_world_y(layout_y: f32, anchor_y: f32) -> f32 {
    -(layout_y - anchor_y) * LAYOUT_TO_WORLD
}

/// Convert layout X to world X, with anchor offset.
fn layout_to_world_x(layout_x: f32, anchor_x: f32) -> f32 {
    (layout_x - anchor_x) * LAYOUT_TO_WORLD
}

/// Builds gizmos for horizontal metric lines and dimension arrows.
/// Returns the lines gizmo, arrows gizmo, and the list of
/// `(label, layout_y)` pairs for label spawning.
fn build_metric_gizmos(
    font_metrics: &crate::text::FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    text_width: f32,
    overlay: &TypographyOverlay,
    anchor_x: f32,
    anchor_y: f32,
    line_left_world: f32,
    first_glyph_left_world: f32,
    last_glyph_right_world: f32,
    _line_right_world: f32,
    computed_advance_minus_bbox: f32,
) -> (GizmoAsset, GizmoAsset, Vec<(&'static str, f32)>) {
    let mut lines_gizmo = GizmoAsset::default();
    let mut arrows_gizmo = GizmoAsset::default();
    let extend = overlay.extend;
    let color = overlay.color;
    let z = 0.001;

    let x_start = -extend * 2.5;
    let x_end = text_width + extend * 2.5;

    let baseline_y = line_metrics.baseline;
    let ascent_y = baseline_y - line_metrics.ascent;
    let descent_y = baseline_y + line_metrics.descent;
    let top_y = line_metrics.top;
    let bottom_y = line_metrics.bottom;

    let mut metric_lines: Vec<(&str, f32)> = Vec::with_capacity(7);
    if (top_y - ascent_y).abs() > 0.5 {
        metric_lines.push(("Top", top_y));
    }
    metric_lines.push(("Ascent", ascent_y));
    // Cap Height gets an arrow on the right side, not a left label.
    metric_lines.push(("", baseline_y - font_metrics.cap_height));
    // x-Height gets an arrow on the right side, not a left label.
    metric_lines.push(("", baseline_y - font_metrics.x_height));
    metric_lines.push(("Baseline", baseline_y));
    metric_lines.push(("Descent", descent_y));
    if (bottom_y - descent_y).abs() > 0.5 {
        metric_lines.push(("Bottom", bottom_y));
    }

    for &(_label, layout_y) in &metric_lines {
        let y = layout_to_world_y(layout_y, anchor_y);
        let x0 = layout_to_world_x(x_start, anchor_x);
        let x1 = layout_to_world_x(x_end, anchor_x);
        lines_gizmo.line(Vec3::new(x0, y, z), Vec3::new(x1, y, z), color);
    }

    // Dimension arrows halfway between the line labels and the text.
    // Center arrows between the left end of the metric line and the
    // left bound of the first glyph bounding box.
    let arrow_x = (line_left_world + first_glyph_left_world) / 2.0;
    let arrow_size = 0.005;
    let arrow_gap = ARROW_GAP;
    let ascent_world = layout_to_world_y(ascent_y, anchor_y);
    let baseline_world = layout_to_world_y(baseline_y, anchor_y);
    let descent_world = layout_to_world_y(descent_y, anchor_y);

    let draw_arrow = |gizmo: &mut GizmoAsset, x: f32, y_top: f32, y_bottom: f32| {
        let tip_top = y_top - arrow_gap;
        let tip_bottom = y_bottom + arrow_gap;
        gizmo.line(Vec3::new(x, tip_top, z), Vec3::new(x, tip_bottom, z), color);
        // Top arrowhead.
        gizmo.line(
            Vec3::new(x, tip_top, z),
            Vec3::new(x - arrow_size, tip_top - arrow_size, z),
            color,
        );
        gizmo.line(
            Vec3::new(x, tip_top, z),
            Vec3::new(x + arrow_size, tip_top - arrow_size, z),
            color,
        );
        // Bottom arrowhead.
        gizmo.line(
            Vec3::new(x, tip_bottom, z),
            Vec3::new(x - arrow_size, tip_bottom + arrow_size, z),
            color,
        );
        gizmo.line(
            Vec3::new(x, tip_bottom, z),
            Vec3::new(x + arrow_size, tip_bottom + arrow_size, z),
            color,
        );
    };

    // Ascent arrow: ascent line ↕ baseline.
    draw_arrow(&mut arrows_gizmo, arrow_x, ascent_world, baseline_world);
    // Descent arrow: baseline ↕ descent line.
    draw_arrow(&mut arrows_gizmo, arrow_x, baseline_world, descent_world);

    // Line Height arrow on the far left — Ascent ↕ Descent.
    let line_height_x = layout_to_world_x(x_start + extend * 0.5, anchor_x);
    draw_arrow(
        &mut arrows_gizmo,
        line_height_x,
        ascent_world,
        descent_world,
    );

    // Right-side arrows: the gap between each arrow equals the advance
    // width minus the bounding box width of the last glyph. This gives
    // a natural spacing that relates to the font's own metrics.
    let right_gap = computed_advance_minus_bbox;
    let x_height_y = baseline_y - font_metrics.x_height;
    let x_height_world = layout_to_world_y(x_height_y, anchor_y);
    let right_arrow_x1 = last_glyph_right_world + right_gap;
    draw_arrow(
        &mut arrows_gizmo,
        right_arrow_x1,
        x_height_world,
        baseline_world,
    );

    let cap_height_y = baseline_y - font_metrics.cap_height;
    let cap_height_world = layout_to_world_y(cap_height_y, anchor_y);
    let right_arrow_x2 = right_arrow_x1 + right_gap;
    draw_arrow(
        &mut arrows_gizmo,
        right_arrow_x2,
        cap_height_world,
        baseline_world,
    );

    (lines_gizmo, arrows_gizmo, metric_lines)
}

/// Spawns labels for metric lines and dimension arrows.
///
/// Line labels sit on each line on the left (`CenterRight` anchor).
/// Ascent/Descent labels sit to the left of their arrows (`CenterRight` anchor):
/// - Ascent: halfway between Cap Height and Ascent lines
/// - Descent: halfway between Baseline and Descent lines
#[allow(clippy::too_many_arguments)]
fn spawn_metric_labels(
    commands: &mut Commands,
    parent: Entity,
    font_name: &str,
    font_metrics: &crate::text::FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    metric_lines: &[(&str, f32)],
    overlay: &TypographyOverlay,
    anchor_x: f32,
    anchor_y: f32,
    font_size: f32,
    line_left_world: f32,
    first_glyph_left_world: f32,
    last_glyph_right_world: f32,
    advance_minus_bbox: f32,
    gizmo_assets: &mut Assets<GizmoAsset>,
) {
    let extend = overlay.extend;
    let label_size = font_size * LABEL_SIZE_RATIO;
    let color = overlay.color;
    let z = 0.001;

    // Line labels sit at the left end of the metric lines.
    let label_x = layout_to_world_x(-extend, anchor_x);

    for &(label, layout_y) in metric_lines {
        // Skip Ascent, Descent (arrow labels) and empty labels (x-Height).
        if label.is_empty() || label == "Ascent" || label == "Descent" || label == "Baseline" {
            continue;
        }

        let line_world_y = layout_to_world_y(layout_y, anchor_y);
        commands.entity(parent).with_child((
            OverlayElement,
            WorldText::new(label),
            TextStyle::new()
                .with_size(label_size)
                .with_color(color)
                .with_anchor(TextAnchor::CenterRight)
                .with_shadow_mode(GlyphShadowMode::None),
            Transform::from_xyz(label_x, line_world_y, z),
        ));
    }

    // Arrow labels — positioned next to the dimension arrows.
    let arrow_x = (line_left_world + first_glyph_left_world) / 2.0;
    let arrow_label_x = arrow_x - 0.01;

    let baseline_y = line_metrics.baseline;
    let ascent_y = baseline_y - line_metrics.ascent;
    let cap_height_y = baseline_y - font_metrics.cap_height;
    let descent_y = baseline_y + line_metrics.descent;

    // Ascent label: halfway between Cap Height and Ascent.
    let ascent_mid = f32::midpoint(cap_height_y, ascent_y);
    let ascent_mid_world = layout_to_world_y(ascent_mid, anchor_y);
    commands.entity(parent).with_child((
        OverlayElement,
        WorldText::new("Ascent"),
        TextStyle::new()
            .with_size(label_size)
            .with_color(color)
            .with_anchor(TextAnchor::CenterRight)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(arrow_label_x, ascent_mid_world, z),
    ));

    // Descent label: halfway between Baseline and Descent.
    let descent_mid = f32::midpoint(baseline_y, descent_y);
    let descent_mid_world = layout_to_world_y(descent_mid, anchor_y);
    commands.entity(parent).with_child((
        OverlayElement,
        WorldText::new("Descent"),
        TextStyle::new()
            .with_size(label_size)
            .with_color(color)
            .with_anchor(TextAnchor::CenterRight)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(arrow_label_x, descent_mid_world, z),
    ));

    // Line Height label on the far left — centered between ascent and descent.
    let line_height_x = layout_to_world_x(-extend * 2.5 + extend * 0.5, anchor_x);
    let line_height_mid = f32::midpoint(ascent_y, descent_y);
    let line_height_mid_world = layout_to_world_y(line_height_mid, anchor_y);
    commands.entity(parent).with_child((
        OverlayElement,
        WorldText::new("Line Height"),
        TextStyle::new()
            .with_size(label_size)
            .with_color(color)
            .with_anchor(TextAnchor::CenterRight)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(line_height_x - 0.01, line_height_mid_world, z),
    ));

    // "Top" label above the ascent line (when font has no line gap,
    // Top == Ascent so we annotate that there's no leading).
    let has_line_gap =
        (line_metrics.top - (line_metrics.baseline - line_metrics.ascent)).abs() > 0.5;
    if !has_line_gap {
        let ascent_world = layout_to_world_y(ascent_y, anchor_y);
        let no_gap_label = format!("no line gap for {font_name}");
        commands.entity(parent).with_child((
            OverlayElement,
            WorldText::new(no_gap_label),
            TextStyle::new()
                .with_size(label_size * 0.8)
                .with_color(Color::srgba(0.7, 0.7, 0.7, 0.6))
                .with_anchor(TextAnchor::BottomLeft)
                .with_shadow_mode(GlyphShadowMode::None),
            Transform::from_xyz(last_glyph_right_world + 0.01, ascent_world, z),
        ));
    }

    // x-Height label — matches arrow position from build_metric_gizmos.
    let x_height_y = baseline_y - font_metrics.x_height;
    let x_height_mid = f32::midpoint(x_height_y, baseline_y);
    let x_height_mid_world = layout_to_world_y(x_height_mid, anchor_y);
    let right_arrow_x1 = last_glyph_right_world + advance_minus_bbox;
    commands.entity(parent).with_child((
        OverlayElement,
        WorldText::new("x-Height"),
        TextStyle::new()
            .with_size(label_size)
            .with_color(color)
            .with_anchor(TextAnchor::CenterLeft)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(right_arrow_x1 + 0.01, x_height_mid_world, z),
    ));

    // Cap Height label — between x-height and cap height lines.
    let cap_height_y = baseline_y - font_metrics.cap_height;
    let cap_mid = f32::midpoint(x_height_y, cap_height_y);
    let cap_mid_world = layout_to_world_y(cap_mid, anchor_y);
    let right_arrow_x2 = right_arrow_x1 + advance_minus_bbox;
    commands.entity(parent).with_child((
        OverlayElement,
        WorldText::new("Cap Height"),
        TextStyle::new()
            .with_size(label_size)
            .with_color(color)
            .with_anchor(TextAnchor::CenterLeft)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(right_arrow_x2 + 0.01, cap_mid_world, z),
    ));

    // Baseline label — to the right of the last glyph, halfway between
    // baseline and descent. Red callout line ascends from above the label
    // to touch the baseline at its rightmost point.
    let callout_color = Color::srgb(0.9, 0.2, 0.2);
    let baseline_world = layout_to_world_y(baseline_y, anchor_y);
    let descent_world_y = layout_to_world_y(descent_y, anchor_y);
    let baseline_label_y = (baseline_world + descent_world_y) / 2.0;
    let baseline_label_x = last_glyph_right_world + advance_minus_bbox;

    commands.entity(parent).with_child((
        OverlayElement,
        WorldText::new("Baseline"),
        TextStyle::new()
            .with_size(label_size)
            .with_color(color)
            .with_anchor(TextAnchor::TopCenter)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(baseline_label_x, baseline_label_y, z),
    ));

    // Red callout — straight up from the top center of "Baseline" to
    // the baseline line directly above it.
    let mut baseline_callout = GizmoAsset::default();
    baseline_callout.line(
        Vec3::new(baseline_label_x, baseline_label_y + 0.003, z),
        Vec3::new(baseline_label_x, baseline_world, z),
        callout_color,
    );
    commands.entity(parent).with_child((
        OverlayElement,
        Gizmo {
            handle:      gizmo_assets.add(baseline_callout),
            line_config: GizmoLineConfig {
                width: THICK_LINE_WIDTH,
                ..default()
            },
            depth_bias:  -0.1,
        },
        Transform::IDENTITY,
    ));
}
