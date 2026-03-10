//! Systems for diegetic UI panel layout computation and debug rendering.

use std::sync::Arc;

use bevy::prelude::*;

use super::DiegeticPanelGizmoGroup;
use super::components::ComputedDiegeticPanel;
use super::components::DiegeticPanel;
use super::components::DiegeticTextMeasurer;
use crate::layout::BoundingBox;
use crate::layout::LayoutEngine;
use crate::layout::RenderCommandKind;

/// Recomputes layout for panels whose [`DiegeticPanel`] component has changed.
pub(super) fn compute_panel_layouts(
    mut panels: Query<(&DiegeticPanel, &mut ComputedDiegeticPanel), Changed<DiegeticPanel>>,
    measurer: Res<DiegeticTextMeasurer>,
) {
    for (panel, mut computed) in &mut panels {
        let engine = LayoutEngine::new(Arc::clone(&measurer.0));
        let result = engine.compute(&panel.tree, panel.layout_width, panel.layout_height);
        computed.result = Some(result);
    }
}

/// Renders debug gizmo wireframes for all panels with computed layouts.
pub(super) fn render_panel_gizmos(
    panels: Query<(&DiegeticPanel, &ComputedDiegeticPanel, &GlobalTransform)>,
    mut gizmos: Gizmos<DiegeticPanelGizmoGroup>,
) {
    for (panel, computed, global_transform) in &panels {
        let Some(result) = &computed.result else {
            continue;
        };

        let scale_x = panel.world_width / panel.layout_width;
        let scale_y = panel.world_height / panel.layout_height;
        let half_w = panel.world_width * 0.5;
        let half_h = panel.world_height * 0.5;

        for cmd in &result.commands {
            let z_offset = match &cmd.kind {
                RenderCommandKind::Rectangle { .. } => 0.0,
                RenderCommandKind::Text { .. } => 0.001,
                RenderCommandKind::Border { .. } => 0.002,
                RenderCommandKind::ScissorStart | RenderCommandKind::ScissorEnd => continue,
            };

            let color = match &cmd.kind {
                RenderCommandKind::Rectangle { color } => *color,
                RenderCommandKind::Text { .. } => Color::srgba(0.9, 0.9, 0.2, 0.8),
                RenderCommandKind::Border { border } => border.color,
                _ => continue,
            };

            draw_rect_outline(
                &mut gizmos,
                global_transform,
                &cmd.bounds,
                scale_x,
                scale_y,
                half_w,
                half_h,
                z_offset,
                color,
            );
        }
    }
}

/// Draws a rectangle outline in world space from layout-space bounds.
///
/// Transforms layout coordinates (top-left origin, Y-down) to panel-local
/// coordinates (center origin, Y-up), then to world space via the entity's
/// [`GlobalTransform`].
#[allow(clippy::too_many_arguments)]
fn draw_rect_outline(
    gizmos: &mut Gizmos<DiegeticPanelGizmoGroup>,
    global_transform: &GlobalTransform,
    bounds: &BoundingBox,
    scale_x: f32,
    scale_y: f32,
    half_w: f32,
    half_h: f32,
    z: f32,
    color: Color,
) {
    // Layout coordinates → panel-local coordinates.
    // Layout: origin at top-left, X-right, Y-down.
    // Panel local: origin at center, X-right, Y-up.
    let left = bounds.x.mul_add(scale_x, -half_w);
    let right = (bounds.x + bounds.width).mul_add(scale_x, -half_w);
    let top = (-bounds.y).mul_add(scale_y, half_h);
    let bottom = (-(bounds.y + bounds.height)).mul_add(scale_y, half_h);

    // Panel-local → world via the entity's transform.
    let tl = global_transform.transform_point(Vec3::new(left, top, z));
    let tr = global_transform.transform_point(Vec3::new(right, top, z));
    let br = global_transform.transform_point(Vec3::new(right, bottom, z));
    let bl = global_transform.transform_point(Vec3::new(left, bottom, z));

    gizmos.line(tl, tr, color);
    gizmos.line(tr, br, color);
    gizmos.line(br, bl, color);
    gizmos.line(bl, tl, color);
}
