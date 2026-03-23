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
            let first_glyph_left = computed
                .glyph_rects
                .first()
                .map_or(0.0, |r| r[0]);
            let line_left = layout_to_world_x(-overlay.extend, anchor_x);

            let (lines_gizmo, arrows_gizmo, metric_lines) = build_metric_gizmos(
                &font_metrics,
                &line_metrics,
                text_width,
                overlay,
                anchor_x,
                anchor_y,
                line_left,
                first_glyph_left,
            );

            // Horizontal metric lines.
            commands.entity(entity).with_child((
                OverlayElement,
                Gizmo {
                    handle:      gizmo_assets.add(lines_gizmo),
                    line_config: GizmoLineConfig {
                        width: overlay.line_width,
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
                        width: overlay.line_width * 2.0,
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
                    &font_metrics,
                    &line_metrics,
                    &metric_lines,
                    overlay,
                    anchor_x,
                    anchor_y,
                    font_size,
                    line_left,
                    first_glyph_left,
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
                        width: overlay.line_width,
                        ..default()
                    },
                    depth_bias:  -0.1,
                },
                Transform::IDENTITY,
            ));

            // "Bounding Box" callout from the first glyph's bbox.
            if computed.glyph_rects.len() >= 1 && overlay.show_labels {
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
                            width: overlay.line_width,
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
) -> (GizmoAsset, GizmoAsset, Vec<(&'static str, f32)>) {
    let mut lines_gizmo = GizmoAsset::default();
    let mut arrows_gizmo = GizmoAsset::default();
    let extend = overlay.extend;
    let color = overlay.color;
    let z = 0.001;

    let x_start = -extend;
    let x_end = text_width + extend;

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
    metric_lines.push(("Cap Height", baseline_y - font_metrics.cap_height));
    metric_lines.push(("x-Height", baseline_y - font_metrics.x_height));
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
    let arrow_gap = 0.003;
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

    (lines_gizmo, arrows_gizmo, metric_lines)
}

/// Spawns labels for metric lines and dimension arrows.
///
/// Line labels sit above each line on the left (`BottomRight` anchor).
/// Ascent/Descent labels sit next to their arrows (`CenterLeft` anchor):
/// - Ascent: halfway between Cap Height and Ascent lines
/// - Descent: halfway between Baseline and Descent lines
#[allow(clippy::too_many_arguments)]
fn spawn_metric_labels(
    commands: &mut Commands,
    parent: Entity,
    font_metrics: &crate::text::FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    metric_lines: &[(&str, f32)],
    overlay: &TypographyOverlay,
    anchor_x: f32,
    anchor_y: f32,
    font_size: f32,
    line_left_world: f32,
    first_glyph_left_world: f32,
) {
    let extend = overlay.extend;
    let label_size = font_size * LABEL_SIZE_RATIO;
    let color = overlay.color;
    let z = 0.001;

    // Line labels sit at the left end of the metric lines.
    let label_x = layout_to_world_x(-extend, anchor_x);

    for &(label, layout_y) in metric_lines {
        // Skip Ascent and Descent — they get arrow labels instead.
        if label == "Ascent" || label == "Descent" {
            continue;
        }

        let line_world_y = layout_to_world_y(layout_y, anchor_y);
        commands.entity(parent).with_child((
            OverlayElement,
            WorldText::new(label),
            TextStyle::new()
                .with_size(label_size)
                .with_color(color)
                .with_anchor(TextAnchor::BottomRight)
                .with_shadow_mode(GlyphShadowMode::None),
            Transform::from_xyz(label_x, line_world_y, z),
        ));
    }

    // Arrow labels — positioned next to the dimension arrows.
    let arrow_label_x = (line_left_world + first_glyph_left_world) / 2.0 + 0.01;

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
            .with_anchor(TextAnchor::CenterLeft)
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
            .with_anchor(TextAnchor::CenterLeft)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(arrow_label_x, descent_mid_world, z),
    ));
}
