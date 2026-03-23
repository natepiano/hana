//! Typography overlay — renders font-level metric lines and per-glyph
//! bounding boxes as gizmos on any [`WorldText`] entity.
//!
//! Uses [`ComputedWorldText`] data populated by the renderer to ensure
//! exact alignment with the rendered MSDF quads — no independent layout
//! computation.

use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;

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

/// Gizmo group for typography overlay lines.
///
/// Registered with a thin default line width so overlay lines are visually
/// distinct from other debug gizmos.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct TypographyOverlayGizmoGroup;

/// Attach to a [`WorldText`] entity to render typography metric annotations
/// as gizmos. Built into the library as a debug tool — only available when
/// the `typography_overlay` feature is enabled.
///
/// The overlay uses the renderer's own computed layout data
/// ([`ComputedWorldText`]) to guarantee alignment with the rendered text.
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
#[derive(Component, Clone, Debug)]
pub struct TypographyOverlay {
    /// Show font-level metric lines (ascent, descent, cap height, x-height,
    /// baseline, top, bottom).
    pub show_font_metrics:  bool,
    /// Show per-glyph bounding boxes and advance widths.
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
            show_glyph_metrics: false,
            show_labels:        true,
            color:              Color::from(WHITE),
            line_width:         DEFAULT_LINE_WIDTH,
            label_size:         6.0,
            extend:             8.0,
        }
    }
}

/// System that updates the gizmo line width from the overlay config.
pub fn update_typography_gizmo_config(
    query: Query<&TypographyOverlay>,
    mut config_store: ResMut<GizmoConfigStore>,
) {
    if let Some(overlay) = query.iter().next() {
        let (config, _) = config_store.config_mut::<TypographyOverlayGizmoGroup>();
        config.line.width = overlay.line_width;
    }
}

/// System that draws typography overlay gizmos.
///
/// Reads [`ComputedWorldText`] (populated by the renderer) for anchor and
/// glyph positions, ensuring exact alignment with the rendered MSDF quads.
pub fn render_typography_overlay(
    query: Query<(
        &WorldText,
        &crate::layout::TextStyle,
        &GlobalTransform,
        &TypographyOverlay,
        &ComputedWorldText,
    )>,
    font_registry: Res<FontRegistry>,
    cache: Res<ShapedTextCache>,
    mut gizmos: Gizmos<TypographyOverlayGizmoGroup>,
) {
    for (world_text, style, global_transform, overlay, computed) in &query {
        if world_text.0.is_empty() {
            continue;
        }

        let font_id = FontId(style.font_id());
        let Some(font) = font_registry.font(font_id) else {
            continue;
        };

        let font_size = style.size();
        let font_metrics = font.metrics(font_size);
        let transform = global_transform.compute_transform();

        // Use the renderer's exact anchor and text width.
        let anchor_x = computed.anchor_x;
        let anchor_y = computed.anchor_y;
        let text_width = computed.text_width;

        // Get parley's line metrics for baseline and top/bottom.
        let measure = style.as_layout_config().as_measure();
        let line_metrics = cache
            .get_shaped(&world_text.0, &measure)
            .and_then(|s| s.line_metrics.first().copied());

        if overlay.show_font_metrics {
            if let Some(lm) = &line_metrics {
                draw_font_metric_lines(
                    &mut gizmos,
                    &font_metrics,
                    lm,
                    text_width,
                    overlay,
                    &transform,
                    anchor_x,
                    anchor_y,
                );
            }
        }

        if overlay.show_glyph_metrics {
            draw_glyph_metrics(
                &mut gizmos,
                &computed.glyph_positions,
                font,
                font_size,
                overlay,
                &transform,
                anchor_x,
                anchor_y,
            );
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

/// Draws horizontal metric lines for font-level metrics.
fn draw_font_metric_lines(
    gizmos: &mut Gizmos<TypographyOverlayGizmoGroup>,
    font_metrics: &crate::text::FontMetrics,
    line_metrics: &LineMetricsSnapshot,
    text_width: f32,
    overlay: &TypographyOverlay,
    transform: &Transform,
    anchor_x: f32,
    anchor_y: f32,
) {
    let extend = overlay.extend;
    let color = overlay.color;

    let x_start = -extend;
    let x_end = text_width + extend;

    let baseline_y = line_metrics.baseline;
    let ascent_y = baseline_y - line_metrics.ascent;
    let descent_y = baseline_y + line_metrics.descent;
    let top_y = line_metrics.top;
    let bottom_y = line_metrics.bottom;

    // Build metric lines, skipping top/bottom when they coincide with
    // ascent/descent (no half-leading).
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

    for &(label, layout_y) in &metric_lines {
        let y = layout_to_world_y(layout_y, anchor_y);
        let x0 = layout_to_world_x(x_start, anchor_x);
        let x1 = layout_to_world_x(x_end, anchor_x);

        let start = transform.transform_point(Vec3::new(x0, y, 0.001));
        let end = transform.transform_point(Vec3::new(x1, y, 0.001));
        gizmos.line(start, end, color);

        let _ = (overlay.show_labels, label);
    }

    // Line height bracket on the right side.
    let bracket_x = layout_to_world_x(extend.mul_add(0.3, x_end), anchor_x);
    let top_world = layout_to_world_y(top_y, anchor_y);
    let bottom_world = layout_to_world_y(bottom_y, anchor_y);
    let bracket_top = transform.transform_point(Vec3::new(bracket_x, top_world, 0.001));
    let bracket_bottom = transform.transform_point(Vec3::new(bracket_x, bottom_world, 0.001));
    gizmos.line(bracket_top, bracket_bottom, color);
}

/// Draws per-glyph bounding boxes using the renderer's glyph positions.
fn draw_glyph_metrics(
    gizmos: &mut Gizmos<TypographyOverlayGizmoGroup>,
    glyph_positions: &[(f32, f32, u16)],
    font: &crate::text::Font,
    font_size: f32,
    overlay: &TypographyOverlay,
    transform: &Transform,
    anchor_x: f32,
    anchor_y: f32,
) {
    let color = overlay.color;

    for &(glyph_x, baseline, glyph_id) in glyph_positions {
        let Some(gm) = font.glyph_metrics_by_id(glyph_id, font_size) else {
            continue;
        };

        // True glyph outline bounds in layout coords.
        // gm.bounds are in font coords (Y-up from baseline).
        // Convert to layout Y-down: layout_y = baseline - font_y.
        let left = glyph_x + gm.bounds.min_x;
        let right = glyph_x + gm.bounds.max_x;
        let top_layout = baseline - gm.bounds.max_y;
        let bottom_layout = baseline - gm.bounds.min_y;

        let corners = [
            Vec3::new(
                layout_to_world_x(left, anchor_x),
                layout_to_world_y(bottom_layout, anchor_y),
                0.002,
            ),
            Vec3::new(
                layout_to_world_x(right, anchor_x),
                layout_to_world_y(bottom_layout, anchor_y),
                0.002,
            ),
            Vec3::new(
                layout_to_world_x(right, anchor_x),
                layout_to_world_y(top_layout, anchor_y),
                0.002,
            ),
            Vec3::new(
                layout_to_world_x(left, anchor_x),
                layout_to_world_y(top_layout, anchor_y),
                0.002,
            ),
        ];

        for i in 0..4 {
            let a = transform.transform_point(corners[i]);
            let b = transform.transform_point(corners[(i + 1) % 4]);
            gizmos.line(a, b, color);
        }

        // Advance marker — vertical tick at the glyph's advance position.
        let advance_x = glyph_x + gm.advance_width;
        let tick_x = layout_to_world_x(advance_x, anchor_x);
        let tick_top = layout_to_world_y(top_layout, anchor_y);
        let tick_bottom = layout_to_world_y(bottom_layout, anchor_y);

        let a = transform.transform_point(Vec3::new(tick_x, tick_top, 0.002));
        let b = transform.transform_point(Vec3::new(tick_x, tick_bottom, 0.002));
        gizmos.line(a, b, color);
    }
}
