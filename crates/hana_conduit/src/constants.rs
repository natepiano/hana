use bevy::color::Color;

// cable gizmo
pub(crate) const CABLE_GIZMO_COLOR: Color = Color::srgb(1.0, 0.6, 0.0);

// segment boundary
pub(crate) const SEGMENT_BOUNDARY_COLOR: Color = Color::srgb(1.0, 0.0, 0.0);
pub(crate) const SEGMENT_BOUNDARY_DOT_SIZE: f32 = 0.03;

// tangent
pub(crate) const TANGENT_GIZMO_COLOR: Color = Color::srgb(1.0, 1.0, 0.0);
pub(crate) const TANGENT_SAMPLING_INTERVAL: usize = 4;
pub(crate) const TANGENT_VECTOR_SCALE: f32 = 0.1;

// waypoint
pub(crate) const WAYPOINT_DOT_COLOR: Color = Color::srgb(0.0, 1.0, 0.0);
pub(crate) const WAYPOINT_DOT_SIZE: f32 = 0.05;
