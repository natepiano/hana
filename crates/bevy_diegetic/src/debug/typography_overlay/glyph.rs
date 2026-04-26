//! Per-glyph overlay spawners: glyph bounding boxes, the "Bounding Box"
//! callout, origin dots, and the advancement dimension arrow.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy_kana::ToF32;
use bevy_kana::ToUsize;

use super::GlyphMetricVisibility;
use super::pipeline::FontContext;
use super::pipeline::OverlayAssets;
use super::pipeline::OverlayContext;
use super::scaling;
use crate::callouts;
use crate::debug::constants::BASELINE_COLOR;
use crate::debug::constants::BBOX_COLOR;
use crate::debug::constants::CALLOUT_Z_OFFSET;
use crate::debug::constants::LABEL_ADVANCEMENT;
use crate::debug::constants::LABEL_BOUNDING_BOX;
use crate::debug::constants::LABEL_ORIGIN;
use crate::debug::constants::LABEL_SIZE_RATIO;
use crate::default_panel_material;
use crate::layout::Anchor;
use crate::layout::Border;
use crate::layout::El;
use crate::layout::LayoutBuilder;
use crate::layout::Sizing;
use crate::layout::WorldTextStyle;
use crate::panel::DiegeticPanel;
use crate::panel::SurfaceShadow;
use crate::render::ComputedWorldText;
use crate::render::WorldText;

/// Geometry inputs for the horizontal advancement dimension arrow. Exists to
/// reduce helper parameter counts.
struct ArrowGeometry {
    origin_x:      f32,
    origin_y:      f32,
    advance_end_x: f32,
    descent_world: f32,
    dot_radius:    f32,
    spacing:       f32,
    z:             f32,
}

/// Geometry and styling for a dashed callout line segment. Exists to reduce
/// helper parameter counts.
struct DashedLine {
    start:     Vec3,
    end:       Vec3,
    dash_len:  f32,
    gap_len:   f32,
    color:     Color,
    thickness: f32,
}

/// Spawns per-glyph bounding boxes, origin dots, and the advancement arrow.
pub(super) fn spawn_glyph_metric_gizmos(
    ctx: &mut OverlayContext<'_, '_, '_>,
    font_ctx: &FontContext<'_>,
    computed: &ComputedWorldText,
    assets: &mut OverlayAssets<'_>,
) {
    spawn_glyph_box_panels(ctx, computed, BBOX_COLOR);

    // "Bounding Box" callout from the first glyph's bbox.
    if !computed.glyph_rects.is_empty() && ctx.overlay.labels == GlyphMetricVisibility::Shown {
        spawn_bounding_box_callout(ctx, font_ctx, computed, BBOX_COLOR);
    }

    // Origin dots + Advancement arrow below the first glyph.
    if !computed.glyph_rects.is_empty() && ctx.overlay.labels == GlyphMetricVisibility::Shown {
        spawn_origin_and_advancement(ctx, font_ctx, computed, assets);
    }
}

/// Spawns one transparent bordered world panel per glyph bounding box.
fn spawn_glyph_box_panels(
    ctx: &mut OverlayContext<'_, '_, '_>,
    computed: &ComputedWorldText,
    bbox_color: Color,
) {
    let mut material = default_panel_material();
    material.base_color = Color::NONE;
    material.alpha_mode = AlphaMode::Blend;
    material.unlit = true;

    let border_width = scaling::bbox_border_width(ctx.overlay, ctx.font_size, ctx.scale);

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

        let Ok(panel) = DiegeticPanel::world()
            .size(w, h)
            .anchor(Anchor::Center)
            .surface_shadow(ctx.overlay.surface_shadow)
            .material(material.clone())
            .with_tree(tree)
            .build()
        else {
            continue;
        };

        ctx.commands.entity(ctx.entity).with_child((
            panel,
            Transform::from_xyz(x + w / 2.0, y - h / 2.0, CALLOUT_Z_OFFSET),
        ));
    }
}

/// Spawns the "Bounding Box" callout label with shelf and riser lines.
fn spawn_bounding_box_callout(
    ctx: &mut OverlayContext<'_, '_, '_>,
    font_ctx: &FontContext<'_>,
    computed: &ComputedWorldText,
    bbox_color: Color,
) {
    let label_size = scaling::font_scale(ctx.font_size, ctx.scale) * LABEL_SIZE_RATIO;
    let callout_thickness = scaling::font_scale(ctx.font_size, ctx.scale) * 0.0025;
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
    let shelf_len = scaling::arrow_spacing(computed.first_advance) / 2.0;
    let shelf_end_x = shelf_right_x + shelf_len;

    // Vertical line goes up to halfway between Cap Height and Ascent.
    let baseline_y_layout = font_ctx.line_metrics.baseline;
    let ascent_y_layout = baseline_y_layout - font_ctx.line_metrics.ascent;
    let cap_height_y_layout = baseline_y_layout - font_ctx.font_metrics.cap_height;
    let callout_top_layout = f32::midpoint(cap_height_y_layout, ascent_y_layout);
    let callout_top_world = scaling::layout_to_world_y(callout_top_layout, ctx.anchor_y, ctx.scale);

    callouts::spawn_callout_line(
        ctx.commands,
        ctx.entity,
        &callouts::CalloutLine::new(
            Vec3::new(shelf_right_x, shelf_y, z),
            Vec3::new(shelf_end_x, shelf_y, z),
        )
        .color(bbox_color)
        .thickness(callout_thickness)
        .surface_shadow(ctx.overlay.surface_shadow),
    );
    callouts::spawn_callout_line(
        ctx.commands,
        ctx.entity,
        &callouts::CalloutLine::new(
            Vec3::new(shelf_end_x, shelf_y, z),
            Vec3::new(shelf_end_x, callout_top_world, z),
        )
        .color(bbox_color)
        .thickness(callout_thickness)
        .surface_shadow(ctx.overlay.surface_shadow),
    );

    // Label at the top of the riser, to the left (CenterRight anchor).
    let ascent_mid_layout = f32::midpoint(cap_height_y_layout, ascent_y_layout);
    let ascent_mid_world = scaling::layout_to_world_y(ascent_mid_layout, ctx.anchor_y, ctx.scale);
    ctx.commands.entity(ctx.entity).with_child((
        WorldText::new(LABEL_BOUNDING_BOX),
        WorldTextStyle::new(label_size)
            .with_color(bbox_color)
            .with_anchor(Anchor::CenterRight)
            .with_shadow_mode(ctx.overlay.label_shadow_mode()),
        Transform::from_xyz(
            shelf_end_x - scaling::label_gap(ctx.font_size, ctx.scale),
            ascent_mid_world,
            z,
        ),
    ));
}

/// Spawns origin dots, origin label, advancement end dot, and advancement arrow.
fn spawn_origin_and_advancement(
    ctx: &mut OverlayContext<'_, '_, '_>,
    font_ctx: &FontContext<'_>,
    computed: &ComputedWorldText,
    assets: &mut OverlayAssets<'_>,
) {
    let label_size = scaling::font_scale(ctx.font_size, ctx.scale) * LABEL_SIZE_RATIO;
    let z = CALLOUT_Z_OFFSET;
    let dot_radius = scaling::dot_radius(ctx.font_size, ctx.scale);

    let first = &computed.glyph_rects[0];
    let first_mid_x = first[0] + first[2] / 2.0;

    let line_metrics = font_ctx.line_metrics;
    let baseline_world = scaling::layout_to_world_y(line_metrics.baseline, ctx.anchor_y, ctx.scale);
    let descent_world = scaling::layout_to_world_y(
        line_metrics.baseline + line_metrics.descent,
        ctx.anchor_y,
        ctx.scale,
    );

    let origin_x = scaling::layout_to_world_x(0.0, ctx.anchor_x, ctx.scale);
    let origin_y = baseline_world;

    // Origin dot — small filled circle at (origin, baseline).
    spawn_overlay_dot(
        ctx,
        assets,
        dot_radius,
        Vec3::new(origin_x, origin_y, z),
        Color::WHITE,
    );

    // Origin label — centered between the bottom of the first
    // glyph's bbox and the Descent line.
    let first_bbox_bottom = first[1] - first[3];
    let origin_label_y = f32::midpoint(first_bbox_bottom, descent_world);

    // Callout line from just above the label toward the origin
    // dot, touching the circle edge. The label's cap height in
    // world units gives the visual top of the text.
    let label_ascent_world = line_metrics.ascent * LABEL_SIZE_RATIO * ctx.scale;
    let label_top_y = origin_label_y + label_ascent_world;
    let dx = origin_x - first_mid_x;
    let dy = origin_y - label_top_y;
    let len = dx.hypot(dy);
    let edge_x = (dx / len).mul_add(-dot_radius, origin_x);
    let edge_y = (dy / len).mul_add(-dot_radius, origin_y);
    callouts::spawn_callout_line(
        ctx.commands,
        ctx.entity,
        &callouts::CalloutLine::new(
            Vec3::new(edge_x, edge_y, z),
            Vec3::new(first_mid_x, label_top_y, z),
        )
        .color(BASELINE_COLOR)
        .thickness(scaling::callout_line_thickness(
            ctx.overlay,
            ctx.font_size,
            ctx.scale,
        ))
        .surface_shadow(ctx.overlay.surface_shadow),
    );
    ctx.commands.entity(ctx.entity).with_child((
        WorldText::new(LABEL_ORIGIN),
        WorldTextStyle::new(label_size)
            .with_color(ctx.overlay.color)
            .with_anchor(Anchor::Center)
            .with_shadow_mode(ctx.overlay.label_shadow_mode()),
        Transform::from_xyz(first_mid_x, origin_label_y, z),
    ));

    // Advancement end dot — filled circle at (origin + advance, baseline).
    let advance_end_x = origin_x + computed.first_advance;
    spawn_overlay_dot(
        ctx,
        assets,
        dot_radius,
        Vec3::new(advance_end_x, origin_y, z),
        Color::WHITE,
    );

    // Advancement arrow — horizontal double-headed arrow below descent.
    let spacing = scaling::arrow_spacing(computed.first_advance);
    spawn_advancement_arrow(
        ctx,
        &ArrowGeometry {
            origin_x,
            origin_y,
            advance_end_x,
            descent_world,
            dot_radius,
            spacing,
            z,
        },
    );
}

/// Spawns the horizontal advancement arrow with tick lines and label.
fn spawn_advancement_arrow(ctx: &mut OverlayContext<'_, '_, '_>, geometry: &ArrowGeometry) {
    let arrow_y = geometry.descent_world - geometry.spacing;
    let head = scaling::arrowhead_size(ctx.font_size, ctx.scale);
    let gap = scaling::arrow_gap(ctx.font_size, ctx.scale);
    let label_size = scaling::font_scale(ctx.font_size, ctx.scale) * LABEL_SIZE_RATIO;
    let thickness = scaling::callout_line_thickness(ctx.overlay, ctx.font_size, ctx.scale);

    // Dashed vertical bracket lines — from below the arrow to just
    // above the origin/advance dots on the baseline.
    let tick_above = geometry.dot_radius.mul_add(3.0, geometry.origin_y);
    let tick_below = arrow_y - head;
    let dash_len = geometry.spacing * 0.125;
    let gap_len = geometry.spacing * 0.125 / 2.0;
    spawn_dashed_callout_line(
        ctx,
        &DashedLine {
            start: Vec3::new(geometry.origin_x, tick_below, geometry.z),
            end: Vec3::new(geometry.origin_x, tick_above, geometry.z),
            dash_len,
            gap_len,
            color: ctx.overlay.color,
            thickness,
        },
    );
    spawn_dashed_callout_line(
        ctx,
        &DashedLine {
            start: Vec3::new(geometry.advance_end_x, tick_below, geometry.z),
            end: Vec3::new(geometry.advance_end_x, tick_above, geometry.z),
            dash_len,
            gap_len,
            color: ctx.overlay.color,
            thickness,
        },
    );

    // Horizontal dimension arrow.
    callouts::spawn_callout_line(
        ctx.commands,
        ctx.entity,
        &callouts::CalloutLine::new(
            Vec3::new(geometry.origin_x, arrow_y, geometry.z),
            Vec3::new(geometry.advance_end_x, arrow_y, geometry.z),
        )
        .color(ctx.overlay.color)
        .thickness(thickness)
        .surface_shadow(ctx.overlay.surface_shadow)
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

    // "Advancement" label centered below the arrow.
    let adv_mid_x = f32::midpoint(geometry.origin_x, geometry.advance_end_x);
    let adv_label_y = geometry.spacing.mul_add(-0.5, arrow_y);
    ctx.commands.entity(ctx.entity).with_child((
        WorldText::new(LABEL_ADVANCEMENT),
        WorldTextStyle::new(label_size)
            .with_color(ctx.overlay.color)
            .with_anchor(Anchor::TopCenter)
            .with_shadow_mode(ctx.overlay.label_shadow_mode()),
        Transform::from_xyz(adv_mid_x, adv_label_y, geometry.z),
    ));
}

fn spawn_overlay_dot(
    ctx: &mut OverlayContext<'_, '_, '_>,
    assets: &mut OverlayAssets<'_>,
    radius: f32,
    position: Vec3,
    color: Color,
) {
    let common = (
        Mesh3d(assets.meshes.add(Circle::new(radius))),
        MeshMaterial3d(assets.materials.add(StandardMaterial {
            base_color: color,
            unlit: true,
            ..default()
        })),
        Transform::from_translation(position),
    );
    match ctx.overlay.surface_shadow {
        SurfaceShadow::On => ctx.commands.entity(ctx.entity).with_child(common),
        SurfaceShadow::Off => ctx
            .commands
            .entity(ctx.entity)
            .with_child((common, NotShadowCaster)),
    };
}

fn spawn_dashed_callout_line(ctx: &mut OverlayContext<'_, '_, '_>, line: &DashedLine) {
    let delta = line.end - line.start;
    let total_len = delta.length();
    if total_len < f32::EPSILON {
        return;
    }
    let dir = delta / total_len;
    let stride = line.dash_len + line.gap_len;
    let count = (total_len / stride).ceil().to_usize();
    for i in 0..count {
        let t = i.to_f32() * stride;
        let dash_end = (t + line.dash_len).min(total_len);
        callouts::spawn_callout_line(
            ctx.commands,
            ctx.entity,
            &callouts::CalloutLine::new(line.start + dir * t, line.start + dir * dash_end)
                .color(line.color)
                .thickness(line.thickness)
                .surface_shadow(ctx.overlay.surface_shadow),
        );
    }
}
