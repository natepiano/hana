//! Low-level drawing primitives for gizmo-based callouts.

use bevy::color::Color;
use bevy::math::Vec3;
use bevy::prelude::GizmoAsset;

/// Arrowhead line length (world units).
pub const ARROWHEAD_SIZE: f32 = 0.008;

/// Gap between arrow tips and the lines they point at (world units).
pub const ARROW_GAP: f32 = 0.006;

/// Draws a double-headed dimension arrow between two points.
///
/// The shaft is inset by [`ARROW_GAP`] from each endpoint so the
/// arrowheads don't overlap the lines they measure. Arrowhead
/// chevrons are perpendicular to the shaft direction.
pub fn draw_dimension_arrow(gizmo: &mut GizmoAsset, from: Vec3, to: Vec3, color: Color) {
    let delta = to - from;
    let len = delta.length();
    if len < f32::EPSILON {
        return;
    }
    let dir = delta / len;

    // Perpendicular in the XY plane (rotate 90 degrees).
    let perp = Vec3::new(-dir.y, dir.x, 0.0);

    let tip_from = from + dir * ARROW_GAP;
    let tip_to = to - dir * ARROW_GAP;

    // Shaft.
    gizmo.line(tip_from, tip_to, color);

    // Arrowhead at `from` end (pointing toward `from`).
    gizmo.line(
        tip_from,
        tip_from + dir * ARROWHEAD_SIZE + perp * ARROWHEAD_SIZE,
        color,
    );
    gizmo.line(
        tip_from,
        tip_from + dir * ARROWHEAD_SIZE - perp * ARROWHEAD_SIZE,
        color,
    );

    // Arrowhead at `to` end (pointing toward `to`).
    gizmo.line(
        tip_to,
        tip_to - dir * ARROWHEAD_SIZE + perp * ARROWHEAD_SIZE,
        color,
    );
    gizmo.line(
        tip_to,
        tip_to - dir * ARROWHEAD_SIZE - perp * ARROWHEAD_SIZE,
        color,
    );
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
    let mut t = 0.0;
    while t < total_len {
        let dash_end = (t + dash_len).min(total_len);
        gizmo.line(start + dir * t, start + dir * dash_end, color);
        t += stride;
    }
}
