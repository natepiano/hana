use bevy::prelude::*;

use crate::cable::CableSystems;
use crate::cable::ComputedCableGeometry;
use crate::constants::CABLE_GIZMO_COLOR;
use crate::constants::SEGMENT_BOUNDARY_COLOR;
use crate::constants::SEGMENT_BOUNDARY_DOT_SIZE;
use crate::constants::TANGENT_GIZMO_COLOR;
use crate::constants::TANGENT_SAMPLING_INTERVAL;
use crate::constants::TANGENT_VECTOR_SCALE;
use crate::constants::WAYPOINT_DOT_COLOR;
use crate::constants::WAYPOINT_DOT_SIZE;

/// Gizmo group for cable debug wireframes.
///
/// Enable or disable via Bevy's `GizmoConfigStore`.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct CableGizmoGroup;

/// Resource that toggles detailed debug visualization.
#[derive(Clone, Debug, Default, PartialEq, Eq, Resource)]
pub enum DebugGizmos {
    /// Debug gizmos are rendered.
    Enabled,
    /// Debug gizmos are hidden.
    #[default]
    Disabled,
}

pub(super) struct GizmosPlugin;

impl Plugin for GizmosPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugGizmos>()
            .init_gizmo_group::<CableGizmoGroup>()
            .add_systems(
                Update,
                (render_cable_gizmos, render_debug_gizmos)
                    .chain()
                    .after(CableSystems::Compute),
            );
    }
}

/// Renders cable geometry as gizmo lines (only when debug is enabled).
fn render_cable_gizmos(
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
fn render_debug_gizmos(
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
