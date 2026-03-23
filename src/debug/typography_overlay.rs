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
    pub show_font_metrics: bool,
    /// Show per-glyph bounding boxes drawn by the shader (uses CPU
    /// bilinear scan to compute UV bounds, shader draws the lines).
    pub show_shader_bbox:  bool,
    /// Show text labels on the metric lines.
    pub show_labels:       bool,
    /// Color for overlay lines and labels (includes alpha).
    pub color:             Color,
    /// Gizmo line width in pixels.
    pub line_width:        f32,
    /// Font size for metric labels.
    pub label_size:        f32,
    /// How far annotation lines extend beyond text bounds (in layout units).
    pub extend:            f32,
}

impl Default for TypographyOverlay {
    fn default() -> Self {
        Self {
            show_font_metrics: true,
            show_shader_bbox:  true,
            show_labels:       true,
            color:             Color::from(WHITE),
            line_width:        DEFAULT_LINE_WIDTH,
            label_size:        6.0,
            extend:            8.0,
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
            let (lines_gizmo, arrows_gizmo) = build_metric_gizmos(
                &font_metrics,
                &line_metrics,
                text_width,
                overlay,
                anchor_x,
                anchor_y,
            );

            // Thin horizontal metric lines.
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

            // Thicker dimension arrows.
            commands.entity(entity).with_child((
                OverlayElement,
                Gizmo {
                    handle:      gizmo_assets.add(arrows_gizmo),
                    line_config: GizmoLineConfig {
                        width: overlay.line_width * 3.0,
                        ..default()
                    },
                    depth_bias:  -0.1,
                },
                Transform::IDENTITY,
            ));

            // Spawn labels as WorldText children.
            if overlay.show_labels {
                spawn_metric_labels(
                    &mut commands,
                    entity,
                    &font_metrics,
                    &line_metrics,
                    overlay,
                    anchor_x,
                    anchor_y,
                    font_size,
                    &mut gizmo_assets,
                );
            }
        }
    }
}

/// Convert layout Y-down to world Y-up, with anchor offset.
fn layout_to_world_y(layout_y: f32, anchor_y: f32) -> f32 {
    -(layout_y - anchor_y) * LAYOUT_TO_WORLD
}

/// Convert layout X to world X, with anchor offset.
fn layout_to_world_x(layout_x: f32, anchor_x: f32) -> f32 {
    (layout_x - anchor_x) * LAYOUT_TO_WORLD
}

/// Builds two retained `GizmoAsset`s: one for horizontal metric lines
/// (thin) and one for dimension arrows (thicker).
fn build_metric_gizmos(
    font_metrics: &crate::text::FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    text_width: f32,
    overlay: &TypographyOverlay,
    anchor_x: f32,
    anchor_y: f32,
) -> (GizmoAsset, GizmoAsset) {
    let mut lines_gizmo = GizmoAsset::default();
    let mut arrows_gizmo = GizmoAsset::default();
    let extend = overlay.extend;
    let color = overlay.color;
    let z = 0.001;

    let x_end = text_width + extend;

    let baseline_y = line_metrics.baseline;
    let ascent_y = baseline_y - line_metrics.ascent;
    let descent_y = baseline_y + line_metrics.descent;
    let top_y = line_metrics.top;
    let bottom_y = line_metrics.bottom;

    // The dimension arrows sit to the left of the text. Labels sit
    // further left. All metric lines extend past the arrows.
    let arrow_x = -extend * 1.5;
    let line_x_start = extend.mul_add(-3.0, arrow_x);

    let mut metric_lines: Vec<(&str, f32)> = Vec::with_capacity(7);
    if (top_y - ascent_y).abs() > 0.5 {
        metric_lines.push(("top", top_y));
    }
    metric_lines.push(("ascent", ascent_y));
    metric_lines.push(("cap height", baseline_y - font_metrics.cap_height));
    metric_lines.push(("x-height", baseline_y - font_metrics.x_height));
    metric_lines.push(("baseline", baseline_y));
    metric_lines.push(("descent", descent_y));
    if (bottom_y - descent_y).abs() > 0.5 {
        metric_lines.push(("bottom", bottom_y));
    }

    // Horizontal metric lines — all extend from well left of the arrows
    // to the right of the text.
    for &(_label, layout_y) in &metric_lines {
        let y = layout_to_world_y(layout_y, anchor_y);
        let x0 = layout_to_world_x(line_x_start, anchor_x);
        let x1 = layout_to_world_x(x_end, anchor_x);
        lines_gizmo.line(Vec3::new(x0, y, z), Vec3::new(x1, y, z), color);
    }

    // Line height bracket on the right side.
    let bracket_x = layout_to_world_x(extend.mul_add(0.3, x_end), anchor_x);
    let top_world = layout_to_world_y(top_y, anchor_y);
    let bottom_world = layout_to_world_y(bottom_y, anchor_y);
    lines_gizmo.line(
        Vec3::new(bracket_x, top_world, z),
        Vec3::new(bracket_x, bottom_world, z),
        color,
    );

    // Dimension arrows — thicker, drawn in a separate gizmo asset.
    let arrow_size = 0.005; // Arrowhead size in world units.
    let arrow_gap = 0.003; // Gap between arrowhead tip and the metric line.
    let bx = layout_to_world_x(arrow_x, anchor_x);
    let ascent_world = layout_to_world_y(ascent_y, anchor_y);
    let baseline_world = layout_to_world_y(baseline_y, anchor_y);
    let descent_world = layout_to_world_y(descent_y, anchor_y);

    // Draws a vertical dimension arrow between two horizontal lines.
    // The arrow tips stop `gap` world units short of the lines.
    let draw_dimension = |gizmo: &mut GizmoAsset, x: f32, y_top: f32, y_bottom: f32, gap: f32| {
        let tip_top = y_top - gap;
        let tip_bottom = y_bottom + gap;
        // Vertical line between arrow tips.
        gizmo.line(Vec3::new(x, tip_top, z), Vec3::new(x, tip_bottom, z), color);
        // Top arrowhead (pointing up toward the line).
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
        // Bottom arrowhead (pointing down toward the line).
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

    // Ascent dimension: ascent line ↕ baseline.
    draw_dimension(
        &mut arrows_gizmo,
        bx,
        ascent_world,
        baseline_world,
        arrow_gap,
    );

    // Descent dimension: baseline ↕ descent line.
    draw_dimension(
        &mut arrows_gizmo,
        bx,
        baseline_world,
        descent_world,
        arrow_gap,
    );

    (lines_gizmo, arrows_gizmo)
}

/// Spawns metric label `WorldText` entities and callout lines as children
/// of the overlay entity.
#[allow(clippy::too_many_arguments)]
fn spawn_metric_labels(
    commands: &mut Commands,
    parent: Entity,
    font_metrics: &crate::text::FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    overlay: &TypographyOverlay,
    anchor_x: f32,
    anchor_y: f32,
    font_size: f32,
    gizmo_assets: &mut Assets<GizmoAsset>,
) {
    let extend = overlay.extend;
    let label_size = font_size * LABEL_SIZE_RATIO;
    let color = overlay.color;
    let z = 0.001;

    let baseline_y_layout = line_metrics.baseline;
    let x_height_y_layout = baseline_y_layout - font_metrics.x_height;

    // Arrow X position matches the dimension arrows in `build_metric_gizmos`.
    let arrow_layout_x = -extend * 1.5;
    let arrow_world_x = layout_to_world_x(arrow_layout_x, anchor_x);

    // Ascent label — below x-height, to the left of the arrow shaft,
    // centered between x-height and baseline.
    let ascent_label_mid = f32::midpoint(x_height_y_layout, baseline_y_layout);
    let ascent_label_world = layout_to_world_y(ascent_label_mid, anchor_y);
    commands.entity(parent).with_child((
        OverlayElement,
        WorldText::new("Ascent"),
        TextStyle::new()
            .with_size(label_size)
            .with_color(color)
            .with_anchor(TextAnchor::CenterRight)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(arrow_world_x - 0.01, ascent_label_world, z),
    ));

    // Descent label — to the left of the arrow shaft, centered vertically.
    let descent_y_layout = baseline_y_layout + line_metrics.descent;
    let descent_mid_layout = f32::midpoint(baseline_y_layout, descent_y_layout);
    let descent_mid_world = layout_to_world_y(descent_mid_layout, anchor_y);
    commands.entity(parent).with_child((
        OverlayElement,
        WorldText::new("Descent"),
        TextStyle::new()
            .with_size(label_size)
            .with_color(color)
            .with_anchor(TextAnchor::CenterRight)
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(arrow_world_x - 0.01, descent_mid_world, z),
    ));

    // Baseline callout — white label with a red callout line ascending
    // to the baseline. The line is a child of the label so it moves with it.
    let baseline_world_y = layout_to_world_y(baseline_y_layout, anchor_y);
    let callout_x = layout_to_world_x(0.0, anchor_x);
    // Position the Baseline label at the midpoint between baseline and descent.
    let callout_y = descent_mid_world;
    let callout_color = Color::srgb(0.9, 0.2, 0.2);

    // Red callout line in the label's local space. Starts just above
    // the "B" cap height and goes up to just below the baseline.
    let label_cap_world = font_metrics.cap_height * (label_size / font_size) * LAYOUT_TO_WORLD;
    let gap = 0.003;
    let line_local_x = 0.003;
    let line_local_bottom = label_cap_world * 0.5 + gap * 2.0;
    let line_local_top = (baseline_world_y - callout_y) - gap;
    let mut callout_gizmo = GizmoAsset::default();
    callout_gizmo.line(
        Vec3::new(line_local_x, line_local_bottom, z),
        Vec3::new(line_local_x, line_local_top, z),
        callout_color,
    );

    let baseline_label = commands
        .spawn((
            OverlayElement,
            WorldText::new("Baseline"),
            TextStyle::new()
                .with_size(label_size)
                .with_color(color)
                .with_anchor(TextAnchor::CenterLeft)
                .with_shadow_mode(GlyphShadowMode::None),
            Transform::from_xyz(callout_x, callout_y, z),
        ))
        .with_child((
            OverlayElement,
            Gizmo {
                handle:      gizmo_assets.add(callout_gizmo),
                line_config: GizmoLineConfig {
                    width: overlay.line_width * 3.0,
                    ..default()
                },
                depth_bias:  -0.1,
            },
            Transform::IDENTITY,
        ))
        .id();
    commands.entity(parent).add_child(baseline_label);
}
