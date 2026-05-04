//! Named constants for the routing module. No magic values.

use bevy::math::Vec3;

// Catenary solver
pub(super) const MAX_NEWTON_ITERATIONS: u32 = 50;
pub(super) const MIN_CATENARY_PARAM: f32 = 1e-4;
/// Minimum sample points required to represent a cable segment.
pub(super) const MIN_CABLE_SAMPLE_POINTS: u32 = 2;
pub const MIN_SEGMENT_LENGTH: f32 = 0.001;
/// Initial Newton guess multiplier for near-taut cables where the standard
/// approximation degenerates.
pub(super) const NEAR_TAUT_INITIAL_GUESS_MULTIPLIER: f32 = 10.0;
/// Threshold below which gravity is considered zero (compared against `length_squared`).
pub(super) const NEAR_ZERO_GRAVITY_THRESHOLD: f32 = 0.5;
pub(super) const NEWTON_TOLERANCE: f32 = 1e-6;
pub(super) const STRAIGHT_LINE_THRESHOLD: f32 = 1.005;

// Defaults
/// Default gravity direction and magnitude.
pub const DEFAULT_GRAVITY: Vec3 = Vec3::new(0.0, -9.81, 0.0);
pub(super) const DEFAULT_GRID_SIZE: f32 = 0.5;
pub(super) const DEFAULT_OBSTACLE_MARGIN: f32 = 0.2;
/// Default number of sample points per cable segment.
pub const DEFAULT_RESOLUTION: u32 = 32;
/// Default slack factor (cable length / straight-line distance).
/// 1.0 = taut (straight line), values > 1.0 add sag.
pub const DEFAULT_SLACK: f32 = 1.2;

// Grid pathfinding
pub(super) const ASTAR_SEGMENT_SAMPLE_STEPS: u32 = 20;
pub(super) const COLLINEARITY_THRESHOLD: f32 = 0.999;
pub(super) const DEFAULT_ASTAR_MAX_CELLS: usize = 10_000;
pub(super) const ORTHOGONAL_SEGMENT_SAMPLE_STEPS: u32 = 10;
