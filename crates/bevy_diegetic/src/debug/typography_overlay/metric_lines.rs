use bevy::prelude::*;

use super::GlyphMetricVisibility;
use super::TypographyOverlay;
use super::labels;
use super::pipeline::FontContext;
use super::pipeline::GlyphExtents;
use super::pipeline::OverlayAssets;
use super::pipeline::OverlayContext;
use super::pipeline::TextServices;
use super::scaling;
use crate::callouts;
use crate::debug::constants::BASELINE_COLOR;
use crate::debug::constants::LABEL_ASCENT;
use crate::debug::constants::LABEL_BASELINE;
use crate::debug::constants::LABEL_BOTTOM;
use crate::debug::constants::LABEL_CAP_HEIGHT;
use crate::debug::constants::LABEL_DESCENT;
use crate::debug::constants::LABEL_TOP;
use crate::debug::constants::LABEL_X_HEIGHT;
use crate::debug::constants::METRIC_ARROW_Z_OFFSET;
use crate::debug::constants::METRIC_LINE_Z_OFFSET;
use crate::default_panel_material;
use crate::layout::Anchor;
use crate::layout::Border;
use crate::layout::Direction;
use crate::layout::El;
use crate::layout::LayoutBuilder;
use crate::layout::LayoutTree;
use crate::layout::LineMetricsSnapshot;
use crate::layout::Sizing;
use crate::panel::DiegeticPanel;
use crate::render::ComputedWorldText;
use crate::text::FontMetrics;

pub(super) struct MetricLineSpec {
    pub(super) offset_y: f32,
    pub(super) color:    Color,
}

/// Spawns horizontal metric lines and dimension arrows for font-level metrics.
pub(super) fn spawn_font_metric_gizmos(
    ctx: &mut OverlayContext<'_, '_, '_>,
    font_name: &str,
    font_ctx: &FontContext<'_>,
    computed: &ComputedWorldText,
    text_services: &mut TextServices<'_>,
    assets: &mut OverlayAssets<'_>,
) -> Entity {
    let extents = GlyphExtents {
        first_left:    computed.glyph_rects.first().map_or(0.0, |r| r[0]),
        last_right:    computed.glyph_rects.last().map_or(0.0, |r| r[0] + r[2]),
        arrow_spacing: scaling::arrow_spacing(computed.first_advance),
    };

    let (_, _, metric_lines) = build_metric_gizmos(
        font_ctx.font_metrics,
        font_ctx.line_metrics,
        ctx.overlay,
        ctx.anchor_y,
        &extents,
        ctx.font_size,
        ctx.scale,
    );

    spawn_metric_line_panel(ctx, font_ctx, &extents);
    spawn_metric_arrow_callouts(ctx, font_ctx, &extents);

    if ctx.overlay.labels == GlyphMetricVisibility::Shown {
        labels::spawn_metric_labels(ctx, font_name, font_ctx, &metric_lines, &extents);
    }

    labels::spawn_overlay_bounds_target(ctx, font_name, font_ctx, &extents, text_services, assets)
}

/// Spawns horizontal font metric lines as a single transparent world panel.
fn spawn_metric_line_panel(
    ctx: &mut OverlayContext<'_, '_, '_>,
    font_ctx: &FontContext<'_>,
    extents: &GlyphExtents,
) {
    let line_specs = metric_line_specs(
        font_ctx.font_metrics,
        font_ctx.line_metrics,
        ctx.overlay,
        ctx.anchor_y,
        ctx.scale,
    );
    if line_specs.len() < 2 {
        return;
    }

    let width = 5.0_f32.mul_add(
        extents.arrow_spacing,
        extents.last_right - extents.first_left,
    );
    if width <= 0.0 {
        return;
    }

    let height = line_specs.last().map_or(0.0, |line| line.offset_y)
        - line_specs.first().map_or(0.0, |line| line.offset_y);
    if height <= 0.0 {
        return;
    }

    let border_width = scaling::metric_line_border_width(ctx.overlay, ctx.font_size, ctx.scale);
    let tree = build_metric_line_tree(width, height, &line_specs, border_width);

    let mut material = default_panel_material();
    material.base_color = Color::NONE;
    material.alpha_mode = AlphaMode::Blend;
    material.unlit = true;

    let x = 3.0_f32.mul_add(-extents.arrow_spacing, extents.first_left);
    let line_metrics = font_ctx.line_metrics;
    let top_layout =
        if (line_metrics.top - (line_metrics.baseline - line_metrics.ascent)).abs() > 0.5 {
            line_metrics.top
        } else {
            line_metrics.baseline - line_metrics.ascent
        };
    let top_world = scaling::layout_to_world_y(top_layout, ctx.anchor_y, ctx.scale);

    let Ok(panel) = DiegeticPanel::world()
        .size(width, height)
        .anchor(Anchor::TopLeft)
        .surface_shadow(ctx.overlay.surface_shadow)
        .material(material)
        .with_tree(tree)
        .build()
    else {
        return;
    };

    ctx.commands.entity(ctx.entity).with_child((
        panel,
        Transform::from_xyz(x, top_world, METRIC_LINE_Z_OFFSET),
    ));
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
    let top_world = scaling::layout_to_world_y(top_layout, anchor_y, scale);
    let offset = |layout_y: f32| top_world - scaling::layout_to_world_y(layout_y, anchor_y, scale);

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
        color:    BASELINE_COLOR,
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
) -> LayoutTree {
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
    let head = scaling::arrowhead_size(font_size, scale);
    let gap = scaling::arrow_gap(font_size, scale);

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

    let left_outermost = 3.0_f32.mul_add(-extents.arrow_spacing, extents.first_left);
    let right_outermost = 2.0_f32.mul_add(extents.arrow_spacing, extents.last_right);
    let line_x0 = left_outermost;
    let line_x1 = right_outermost;

    for &(label, layout_y) in &metric_lines {
        let y = scaling::layout_to_world_y(layout_y, anchor_y, scale);
        let line_color = if label == LABEL_BASELINE {
            BASELINE_COLOR
        } else {
            color
        };
        lines_gizmo.line(
            Vec3::new(line_x0, y, z),
            Vec3::new(line_x1, y, z),
            line_color,
        );
    }

    let ascent_world = scaling::layout_to_world_y(ascent_y, anchor_y, scale);
    let baseline_world = scaling::layout_to_world_y(baseline_y, anchor_y, scale);
    let descent_world = scaling::layout_to_world_y(descent_y, anchor_y, scale);

    let left_1 = extents.first_left - extents.arrow_spacing;
    let left_2 = 3.0_f32.mul_add(-extents.arrow_spacing, extents.first_left);

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

    let x_height_world =
        scaling::layout_to_world_y(baseline_y - font_metrics.x_height, anchor_y, scale);
    let cap_height_world =
        scaling::layout_to_world_y(baseline_y - font_metrics.cap_height, anchor_y, scale);

    let right_1 = extents.last_right + extents.arrow_spacing;
    let right_2 = 2.0_f32.mul_add(extents.arrow_spacing, extents.last_right);

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

const fn solid_arrow_cap(head: f32, tint: Option<Color>) -> callouts::CalloutCap {
    let cap = callouts::CalloutCap::arrow()
        .solid()
        .length(head)
        .width(head);
    if let Some(color) = tint {
        cap.color(color)
    } else {
        cap
    }
}

fn spawn_metric_arrow_callouts(
    ctx: &mut OverlayContext<'_, '_, '_>,
    font_ctx: &FontContext<'_>,
    extents: &GlyphExtents,
) {
    let line_metrics = font_ctx.line_metrics;
    let font_metrics = font_ctx.font_metrics;
    let baseline_y = line_metrics.baseline;
    let ascent_y = baseline_y - line_metrics.ascent;
    let descent_y = baseline_y + line_metrics.descent;
    let ascent_world = scaling::layout_to_world_y(ascent_y, ctx.anchor_y, ctx.scale);
    let baseline_world = scaling::layout_to_world_y(baseline_y, ctx.anchor_y, ctx.scale);
    let descent_world = scaling::layout_to_world_y(descent_y, ctx.anchor_y, ctx.scale);
    let x_height_world =
        scaling::layout_to_world_y(baseline_y - font_metrics.x_height, ctx.anchor_y, ctx.scale);
    let cap_height_world = scaling::layout_to_world_y(
        baseline_y - font_metrics.cap_height,
        ctx.anchor_y,
        ctx.scale,
    );

    let left_1 = extents.first_left - extents.arrow_spacing;
    let left_2 = 3.0_f32.mul_add(-extents.arrow_spacing, extents.first_left);
    let right_1 = extents.last_right + extents.arrow_spacing;
    let right_2 = 2.0_f32.mul_add(extents.arrow_spacing, extents.last_right);
    let head = scaling::arrowhead_size(ctx.font_size, ctx.scale);
    let gap = scaling::arrow_gap(ctx.font_size, ctx.scale);
    let thickness = scaling::callout_line_thickness(ctx.overlay, ctx.font_size, ctx.scale);

    let plain = || solid_arrow_cap(head, None);
    let tinted = || solid_arrow_cap(head, Some(BASELINE_COLOR));
    let pos = |x, y| Vec3::new(x, y, METRIC_ARROW_Z_OFFSET);

    for (from, to, start_cap, end_cap) in [
        (
            pos(left_1, ascent_world),
            pos(left_1, baseline_world),
            plain(),
            tinted(),
        ),
        (
            pos(left_1, baseline_world),
            pos(left_1, descent_world),
            tinted(),
            plain(),
        ),
        (
            pos(left_2, ascent_world),
            pos(left_2, descent_world),
            plain(),
            plain(),
        ),
        (
            pos(right_1, x_height_world),
            pos(right_1, baseline_world),
            plain(),
            tinted(),
        ),
        (
            pos(right_2, cap_height_world),
            pos(right_2, baseline_world),
            plain(),
            tinted(),
        ),
    ] {
        callouts::spawn_callout_line(
            ctx.commands,
            ctx.entity,
            &callouts::CalloutLine::new(from, to)
                .color(ctx.overlay.color)
                .thickness(thickness)
                .surface_shadow(ctx.overlay.surface_shadow)
                .start_inset(gap)
                .end_inset(gap)
                .start_cap(start_cap)
                .end_cap(end_cap),
        );
    }
}
