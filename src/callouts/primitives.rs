//! Low-level drawing primitives for gizmo-based callouts.

use bevy::color::Color;
use bevy::math::Vec3;
use bevy::prelude::GizmoAsset;

/// Draws a double-headed dimension arrow between two points.
///
/// The shaft is inset by `gap` from each endpoint so the arrowheads
/// don't overlap the lines they measure. Arrowhead chevrons are
/// perpendicular to the shaft direction with line length `head_size`.
pub fn draw_dimension_arrow(
    gizmo: &mut GizmoAsset,
    from: Vec3,
    to: Vec3,
    color: Color,
    head_size: f32,
    gap: f32,
) {
    let delta = to - from;
    let len = delta.length();
    if len < f32::EPSILON {
        return;
    }
    let dir = delta / len;

    // Perpendicular in the XY plane (rotate 90 degrees).
    let perp = Vec3::new(-dir.y, dir.x, 0.0);

    let tip_from = from + dir * gap;
    let tip_to = to - dir * gap;

    // Shaft.
    gizmo.line(tip_from, tip_to, color);

    // Arrowhead at `from` end (pointing toward `from`).
    gizmo.line(
        tip_from,
        tip_from + dir * head_size + perp * head_size,
        color,
    );
    gizmo.line(
        tip_from,
        tip_from + dir * head_size - perp * head_size,
        color,
    );

    // Arrowhead at `to` end (pointing toward `to`).
    gizmo.line(tip_to, tip_to - dir * head_size + perp * head_size, color);
    gizmo.line(tip_to, tip_to - dir * head_size - perp * head_size, color);
}

/// Draws a dashed line between two points. Dashes and gaps are
/// specified in world units along the line direction.
pub fn draw_dashed_line(
    gizmo: &mut GizmoAsset,
    start: Vec3,
    end: Vec3,
    dash_len: f32,
    gap_len: f32,
    color: Color,
) {
    let delta = end - start;
    let total_len = delta.length();
    if total_len < f32::EPSILON {
        return;
    }
    let dir = delta / total_len;
    let stride = dash_len + gap_len;
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let count = (total_len / stride).ceil() as usize;
    for i in 0..count {
        #[allow(clippy::cast_precision_loss)]
        let t = i as f32 * stride;
        let dash_end = (t + dash_len).min(total_len);
        gizmo.line(start + dir * t, start + dir * dash_end, color);
    }
}
