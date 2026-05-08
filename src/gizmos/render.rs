use bevy::prelude::*;

use super::constants::CABLE_GIZMO_COLOR;
use super::constants::SEGMENT_BOUNDARY_COLOR;
use super::constants::SEGMENT_BOUNDARY_DOT_SIZE;
use super::constants::TANGENT_GIZMO_COLOR;
use super::constants::TANGENT_SAMPLING_INTERVAL;
use super::constants::TANGENT_VECTOR_SCALE;
use super::constants::WAYPOINT_DOT_COLOR;
use super::constants::WAYPOINT_DOT_SIZE;
use super::debug::CableGizmoGroup;
use super::debug::DebugGizmos;
use crate::cable::ComputedCableGeometry;

/// Renders cable geometry as gizmo lines (only when debug is enabled).
pub(super) fn render_cable_gizmos(
    cables: Query<&ComputedCableGeometry>,
    mut gizmos: Gizmos<CableGizmoGroup>,
    debug_gizmos: Res<DebugGizmos>,
) {
    if *debug_gizmos == DebugGizmos::Disabled {
        return;
    }
    for computed in &cables {
        let Some(cable_geometry) = &computed.cable_geometry else {
            continue;
        };

        for segment in &cable_geometry.segments {
            if segment.points.len() < 2 {
                continue;
            }

            for pair in segment.points.windows(2) {
                gizmos.line(pair[0], pair[1], CABLE_GIZMO_COLOR);
            }
        }

        for &waypoint in &cable_geometry.waypoints {
            draw_dot(&mut gizmos, waypoint, WAYPOINT_DOT_SIZE, WAYPOINT_DOT_COLOR);
        }
    }
}

/// Renders detailed debug info: tangent vectors and segment boundaries.
pub(super) fn render_debug_gizmos(
    cables: Query<&ComputedCableGeometry>,
    mut gizmos: Gizmos<CableGizmoGroup>,
    debug_gizmos: Res<DebugGizmos>,
) {
    if *debug_gizmos == DebugGizmos::Disabled {
        return;
    }

    for computed in &cables {
        let Some(cable_geometry) = &computed.cable_geometry else {
            continue;
        };

        for segment in &cable_geometry.segments {
            for (i, (point, tangent)) in segment.points.iter().zip(&segment.tangents).enumerate() {
                if i % TANGENT_SAMPLING_INTERVAL == 0 {
                    gizmos.line(
                        *point,
                        *point + *tangent * TANGENT_VECTOR_SCALE,
                        TANGENT_GIZMO_COLOR,
                    );
                }
            }

            if let Some(first) = segment.points.first() {
                draw_dot(
                    &mut gizmos,
                    *first,
                    SEGMENT_BOUNDARY_DOT_SIZE,
                    SEGMENT_BOUNDARY_COLOR,
                );
            }
            if let Some(last) = segment.points.last() {
                draw_dot(
                    &mut gizmos,
                    *last,
                    SEGMENT_BOUNDARY_DOT_SIZE,
                    SEGMENT_BOUNDARY_COLOR,
                );
            }
        }
    }
}

fn draw_dot(gizmos: &mut Gizmos<CableGizmoGroup>, point: Vec3, size: f32, color: Color) {
    gizmos.line(point - Vec3::X * size, point + Vec3::X * size, color);
    gizmos.line(point - Vec3::Y * size, point + Vec3::Y * size, color);
    gizmos.line(point - Vec3::Z * size, point + Vec3::Z * size, color);
}
