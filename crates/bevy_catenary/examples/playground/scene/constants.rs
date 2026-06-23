use bevy::prelude::*;

use crate::constants::DRAGGABLE_CUBE_SIZE;
use crate::constants::NODE_CUBE_SIZE;

// orthogonal routing
pub(super) const ORTHOGONAL_ROUTING_OBSTACLE_HALF_EXTENTS: Vec3 = Vec3::splat(0.5);
pub(super) const ORTHOGONAL_ROUTING_OBSTACLE_SIZE_MULTIPLIER: f32 = 2.0;
pub(super) const ORTHOGONAL_ROUTING_END_Z: f32 = 1.2;
pub(super) const ORTHOGONAL_ROUTING_OBSTACLE_OFFSETS: [Vec3; 3] = [
    Vec3::new(-1.1, -0.35, ORTHOGONAL_ROUTING_START_Z),
    Vec3::new(0.0, 0.35, 0.0),
    Vec3::new(1.1, 0.0, ORTHOGONAL_ROUTING_END_Z),
];
pub(super) const ORTHOGONAL_ROUTING_START_Z: f32 = -1.2;

// cap styles
pub(super) const CAP_STYLE_ENDPOINT_X_MULTIPLIERS: [(f32, f32); 3] =
    [(-2.0, -1.0), (-0.5, 0.5), (1.0, 2.0)];
pub(super) const CAP_STYLE_LEFT_TUBE_INDEX: usize = 0;
pub(super) const CAP_STYLE_LIGHT_PHASES: [f32; 3] = [0.3, 0.7, 0.0];
pub(super) const CAP_STYLE_MIDDLE_TUBE_INDEX: usize = 1;
pub(super) const CAP_STYLE_RIGHT_TUBE_INDEX: usize = 2;

// entity attachment
pub(super) const DRAGGABLE_CUBE_DIMENSION: f32 = DRAGGABLE_CUBE_SIZE * 2.0;
pub(super) const ENTITY_ATTACHMENT_Z: f32 = 0.0;

// inside view
pub(super) const INSIDE_VIEW_ENDPOINT_X_OFFSET: f32 = 0.8;
pub(super) const INSIDE_VIEW_END_Y_OFFSET: f32 = -1.5;
pub(super) const INSIDE_VIEW_START_Y_OFFSET: f32 = 0.8;
pub(super) const INSIDE_VIEW_TUBE_SIDES: u32 = 64;
pub(super) const INSIDE_VIEW_Z_EXTENT: f32 = 3.0;

// node mesh
pub(super) const NODE_CUBE_DIMENSION: f32 = NODE_CUBE_SIZE * 2.0;

// shared hub
pub(super) const SHARED_HUB_POSITION_Z: f32 = 0.0;
pub(super) const SHARED_HUB_SPHERE_RINGS: u32 = 32;
pub(super) const SHARED_HUB_SPHERE_SECTORS: u32 = 32;
pub(super) const SHARED_HUB_SPOKE_CENTER_INDEX: usize = 2;
pub(super) const SHARED_HUB_SPOKE_LEFT_INDEX: usize = 0;
pub(super) const SHARED_HUB_SPOKE_RIGHT_INDEX: usize = 1;
pub(super) const SHARED_HUB_SPOKE_X_OFFSET: f32 = 3.0;
pub(super) const SHARED_HUB_SPOKE_Y_OFFSET: f32 = 0.5;
pub(super) const SHARED_HUB_SPOKE_Z: [f32; 3] = [-1.5, -1.5, 2.0];

// solver comparison
pub(super) const SOLVER_COMPARISON_CATENARY_Z: f32 = -1.5;
pub(super) const SOLVER_COMPARISON_LINEAR_Z: f32 = 0.0;
pub(super) const SOLVER_COMPARISON_ROUTED_END_Y_OFFSET: f32 = 0.5;
pub(super) const SOLVER_COMPARISON_ROUTED_START_Y_OFFSET: f32 = -0.5;
pub(super) const SOLVER_COMPARISON_ROUTED_Z: f32 = 1.5;
