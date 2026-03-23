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
    /// Show per-glyph bounding boxes and advance widths (gizmo lines).
    pub show_glyph_metrics: bool,
    /// Show per-glyph bounding boxes drawn by the shader (uses CPU
    /// bilinear scan to compute UV bounds, shader draws the lines).
    pub show_shader_bbox:   bool,
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
    /// Anti-aliasing expansion factor for glyph bounding boxes. Controls
    /// how far the bounding box extends beyond the mathematical outline
    /// to account for the MSDF shader's anti-aliased edge. Higher values
    /// produce larger boxes.
    pub aa_factor:          f32,
}

impl Default for TypographyOverlay {
    fn default() -> Self {
        Self {
            show_font_metrics:  true,
            show_glyph_metrics: false,
            show_shader_bbox:   true,
            show_labels:        true,
            color:              Color::from(WHITE),
            line_width:         DEFAULT_LINE_WIDTH,
            label_size:         6.0,
            extend:             8.0,
            aa_factor:          1.2,
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
        &GlobalTransform,
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
    cameras: Query<(&GlobalTransform, &Projection, &Camera), Changed<GlobalTransform>>,
    all_cameras: Query<(&GlobalTransform, &Projection, &Camera)>,
    atlas: Res<crate::text::MsdfAtlas>,
    font_registry: Res<FontRegistry>,
    cache: Res<ShapedTextCache>,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
    mut commands: Commands,
) {
    let camera_changed = !cameras.is_empty();
    let changed_entities: Vec<Entity> = text_changed.iter().collect();

    for (entity, world_text, style, overlay, computed, text_gtransform) in &query {
        // Only rebuild if the text changed or the camera moved.
        if !camera_changed && !changed_entities.contains(&entity) {
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
            let gizmo_asset = build_metric_line_gizmo(
                &font_metrics,
                &line_metrics,
                text_width,
                overlay,
                anchor_x,
                anchor_y,
            );

            commands.entity(entity).with_child((
                OverlayElement,
                Gizmo {
                    handle:      gizmo_assets.add(gizmo_asset),
                    line_config: GizmoLineConfig {
                        width: overlay.line_width,
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
                    &line_metrics,
                    overlay,
                    anchor_x,
                    anchor_y,
                    font_size,
                );
            }
        }

        // Build per-glyph bounding box gizmo from the renderer's actual
        // quad rects — guaranteed to match what's drawn on screen.
        if overlay.show_glyph_metrics {
            // Compute AA expansion using the MSDF shader's formula.
            //
            // The shader's visible edge extends beyond the outline by:
            //   extension_bp = 0.48 / screen_px_range * sdf_range
            // where screen_px_range = max(0.5 * sdf_range * screen_px_per_bp, 1.0)
            // and screen_px_per_bp = world_per_bp / world_per_screen_pixel.
            #[allow(clippy::cast_precision_loss)]
            let aa_expansion = all_cameras
                .iter()
                .next()
                .map_or(0.0, |(cam_gt, proj, cam)| {
                    let viewport_height = cam
                        .physical_viewport_size()
                        .map_or(1080.0, |size| size.y as f32);
                    let dist = cam_gt.translation().distance(text_gtransform.translation());
                    let frustum_height = match proj {
                        Projection::Perspective(persp) => 2.0 * dist * (persp.fov / 2.0).tan(),
                        Projection::Orthographic(ortho) => ortho.area.height(),
                        _ => return 0.0,
                    };

                    let world_per_screen_px = frustum_height / viewport_height;
                    let sdf_range = atlas.sdf_range() as f32;
                    let canonical = atlas.canonical_size() as f32;

                    // World units per bitmap pixel.
                    let world_per_bp = font_size / canonical * LAYOUT_TO_WORLD;

                    // Screen pixels per bitmap pixel.
                    let screen_px_per_bp = world_per_bp / world_per_screen_px;

                    // screen_px_range (same formula as the shader).
                    let spr = (0.5 * sdf_range * screen_px_per_bp).max(1.0);

                    // Extension beyond the outline in bitmap pixels.
                    let ext_bp = overlay.aa_factor / spr * sdf_range;

                    ext_bp * world_per_bp
                });

            let glyph_gizmo = build_glyph_box_gizmo(&computed.glyph_rects, aa_expansion);

            commands.entity(entity).with_child((
                OverlayElement,
                Gizmo {
                    handle:      gizmo_assets.add(glyph_gizmo),
                    line_config: GizmoLineConfig {
                        width: 2.0,
                        ..default()
                    },
                    depth_bias:  -0.1,
                },
                Transform::IDENTITY,
            ));
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

/// Builds a retained `GizmoAsset` containing all font metric lines.
fn build_metric_line_gizmo(
    font_metrics: &crate::text::FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    text_width: f32,
    overlay: &TypographyOverlay,
    anchor_x: f32,
    anchor_y: f32,
) -> GizmoAsset {
    let mut gizmo = GizmoAsset::default();
    let extend = overlay.extend;
    let color = overlay.color;

    let x_start = -extend;
    let x_end = text_width + extend;

    let baseline_y = line_metrics.baseline;
    let ascent_y = baseline_y - line_metrics.ascent;
    let descent_y = baseline_y + line_metrics.descent;
    let top_y = line_metrics.top;
    let bottom_y = line_metrics.bottom;

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

    for &(_label, layout_y) in &metric_lines {
        let y = layout_to_world_y(layout_y, anchor_y);
        let x0 = layout_to_world_x(x_start, anchor_x);
        let x1 = layout_to_world_x(x_end, anchor_x);

        gizmo.line(Vec3::new(x0, y, 0.001), Vec3::new(x1, y, 0.001), color);
    }

    // Line height bracket on the right side.
    let bracket_x = layout_to_world_x(extend.mul_add(0.3, x_end), anchor_x);
    let top_world = layout_to_world_y(top_y, anchor_y);
    let bottom_world = layout_to_world_y(bottom_y, anchor_y);
    gizmo.line(
        Vec3::new(bracket_x, top_world, 0.001),
        Vec3::new(bracket_x, bottom_world, 0.001),
        color,
    );

    // Ascent dimension bracket on the left side.
    // Vertical line from baseline to ascent, with horizontal ticks
    // pointing LEFT (away from the text), like the Apple reference.
    let tick_len = 0.008;
    let bracket_layout_x = extend.mul_add(-0.5, x_start);
    let bx = layout_to_world_x(bracket_layout_x, anchor_x);
    let ascent_world = layout_to_world_y(ascent_y, anchor_y);
    let baseline_world = layout_to_world_y(baseline_y, anchor_y);

    // Vertical span line.
    gizmo.line(
        Vec3::new(bx, ascent_world, 0.001),
        Vec3::new(bx, baseline_world, 0.001),
        color,
    );
    // Top tick at ascent (pointing left, away from text).
    gizmo.line(
        Vec3::new(bx - tick_len, ascent_world, 0.001),
        Vec3::new(bx, ascent_world, 0.001),
        color,
    );
    // Bottom tick at baseline (pointing left, away from text).
    gizmo.line(
        Vec3::new(bx - tick_len, baseline_world, 0.001),
        Vec3::new(bx, baseline_world, 0.001),
        color,
    );

    gizmo
}

/// Spawns metric label `WorldText` entities as children of the overlay entity.
/// Currently spawns only the "Baseline" label as a first step.
#[allow(clippy::too_many_arguments)]
fn spawn_metric_labels(
    commands: &mut Commands,
    parent: Entity,
    line_metrics: &LineMetricsSnapshot,
    overlay: &TypographyOverlay,
    anchor_x: f32,
    anchor_y: f32,
    font_size: f32,
) {
    let extend = overlay.extend;
    let label_size = font_size * LABEL_SIZE_RATIO;
    let color = overlay.color;

    // Label X position: to the left of the metric lines.
    let label_x = layout_to_world_x(-extend, anchor_x);

    let baseline_y_layout = line_metrics.baseline;
    let ascent_y_layout = baseline_y_layout - line_metrics.ascent;

    // Baseline label — positioned at the left end of the baseline line.
    let baseline_world_y = layout_to_world_y(baseline_y_layout, anchor_y);
    commands.entity(parent).with_child((
        OverlayElement,
        WorldText::new("Baseline"),
        TextStyle::new()
            .with_size(label_size)
            .with_color(color)
            .with_anchor(TextAnchor::BottomRight),
        Transform::from_xyz(label_x - 0.01, baseline_world_y, 0.001),
    ));

    // Ascent label — positioned at the midpoint of the ascent bracket,
    // to the left of the bracket's vertical line.
    let bracket_layout_x = extend.mul_add(-1.5, 0.0);
    let bracket_world_x = layout_to_world_x(bracket_layout_x, anchor_x);
    let ascent_mid_layout = f32::midpoint(ascent_y_layout, baseline_y_layout);
    let ascent_mid_world = layout_to_world_y(ascent_mid_layout, anchor_y);
    commands.entity(parent).with_child((
        OverlayElement,
        WorldText::new("Ascent"),
        TextStyle::new()
            .with_size(label_size)
            .with_color(color)
            .with_anchor(TextAnchor::CenterRight),
        Transform::from_xyz(bracket_world_x - 0.01, ascent_mid_world, 0.001),
    ));
}

/// Builds a retained `GizmoAsset` containing per-glyph bounding boxes.
///
/// Uses the renderer's actual quad rects (world-space, after clipping) so
/// the boxes match exactly what's drawn on screen.
fn build_glyph_box_gizmo(glyph_rects: &[[f32; 4]], aa_expansion: f32) -> GizmoAsset {
    let mut gizmo = GizmoAsset::default();
    let color = Color::srgb(1.0, 1.0, 0.0);

    for &[x, y, w, h] in glyph_rects {
        // Expand the outline bbox by the AA width to match the visible edge.
        let ex = aa_expansion;
        let tl = Vec3::new(x - ex, y + ex, 0.002);
        let tr = Vec3::new(x + w + ex, y + ex, 0.002);
        let br = Vec3::new(x + w + ex, y - h - ex, 0.002);
        let bl = Vec3::new(x - ex, y - h - ex, 0.002);

        gizmo.line(tl, tr, color);
        gizmo.line(tr, br, color);
        gizmo.line(br, bl, color);
        gizmo.line(bl, tl, color);
    }

    // TODO: advance markers — needs a different visual treatment
    // (e.g. arrow below baseline like the Apple diagram's "Advancement")
    // rather than a vertical line that looks like a bounding box edge.

    gizmo
}
