use bevy::prelude::*;

// cable gizmo constants
pub(super) const CABLE_GIZMO_COLOR: Color = Color::srgb(1.0, 0.6, 0.0);

// segment boundary constants
pub(super) const SEGMENT_BOUNDARY_COLOR: Color = Color::srgb(1.0, 0.0, 0.0);
pub(super) const SEGMENT_BOUNDARY_DOT_SIZE: f32 = 0.03;

// tangent constants
pub(super) const TANGENT_GIZMO_COLOR: Color = Color::srgb(1.0, 1.0, 0.0);
pub(super) const TANGENT_SAMPLING_INTERVAL: usize = 4;
pub(super) const TANGENT_VECTOR_SCALE: f32 = 0.1;

// waypoint constants
pub(super) const WAYPOINT_DOT_COLOR: Color = Color::srgb(0.0, 1.0, 0.0);
pub(super) const WAYPOINT_DOT_SIZE: f32 = 0.05;
