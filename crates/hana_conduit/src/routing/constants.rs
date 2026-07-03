//! Named constants for the routing module. No magic values.

use bevy::math::Vec3;

// catenary solver
pub(super) const MAX_NEWTON_ITERATIONS: u32 = 50;
/// Minimum sample points required to represent a cable segment.
pub(crate) const MIN_CABLE_SAMPLE_POINTS: u32 = 2;
pub(super) const MIN_CATENARY_PARAM: f32 = 1e-4;
pub(crate) const MIN_SEGMENT_LENGTH: f32 = 0.001;
/// Minimum slack factor for a taut cable.
pub(super) const MIN_TAUT_CABLE_SLACK: f32 = 1.0;
/// Initial Newton guess multiplier for near-taut cables where the standard
/// approximation degenerates.
pub(super) const NEAR_TAUT_INITIAL_GUESS_MULTIPLIER: f32 = 10.0;
/// Threshold below which gravity is considered zero (compared against `length_squared`).
pub(super) const NEAR_ZERO_GRAVITY_THRESHOLD: f32 = 0.5;
pub(super) const NEWTON_TOLERANCE: f32 = 1e-6;
pub(super) const STRAIGHT_LINE_THRESHOLD: f32 = 1.005;

// defaults
/// Default gravity direction and magnitude.
pub const DEFAULT_GRAVITY: Vec3 = Vec3::new(0.0, -9.81, 0.0);
pub(super) const DEFAULT_GRID_SIZE: f32 = 0.5;
pub(super) const DEFAULT_OBSTACLE_MARGIN: f32 = 0.2;
/// Default number of sample points per cable segment.
pub const DEFAULT_RESOLUTION: u32 = 32;
pub(super) const DEFAULT_RESOLUTION_SENTINEL: u32 = 0;
/// Default slack factor (cable length / straight-line distance).
/// 1.0 = taut (straight line), values > 1.0 add sag.
pub const DEFAULT_SLACK: f32 = 1.2;

// grid pathfinding
/// Chebyshev radius, in cells, searched for a clear cell when a route
/// endpoint's quantized cell lands inside an obstacle.
pub(super) const ASTAR_CLEAR_CELL_SEARCH_RADIUS: i32 = 3;
pub(super) const ASTAR_SEGMENT_SAMPLE_STEPS: u32 = 20;
/// Sample points per grid cell of segment length when testing whether a
/// shortcut between two route waypoints stays clear of obstacles. Scaling by
/// length keeps long shortcuts from stepping over thin obstacles.
pub(super) const ASTAR_SHORTCUT_SAMPLES_PER_CELL: f32 = 2.0;
pub(super) const COLLINEARITY_THRESHOLD: f32 = 0.999;
pub(super) const DEFAULT_ASTAR_MAX_CELLS: usize = 10_000;

// orthogonal routing
pub(super) const AXIS_X_INDEX: usize = 0;
pub(super) const AXIS_Y_INDEX: usize = 1;
pub(super) const AXIS_Z_INDEX: usize = 2;
pub(super) const HORIZONTAL_FIRST_AXIS_ORDERS: [[usize; 3]; 4] = [
    [AXIS_X_INDEX, AXIS_Z_INDEX, AXIS_Y_INDEX],
    [AXIS_X_INDEX, AXIS_Y_INDEX, AXIS_Z_INDEX],
    [AXIS_Y_INDEX, AXIS_X_INDEX, AXIS_Z_INDEX],
    [AXIS_Z_INDEX, AXIS_Y_INDEX, AXIS_X_INDEX],
];
pub(super) const OBSTACLE_CLEARANCE_MULTIPLIER: f32 = 2.0;
pub(super) const ORTHOGONAL_SEGMENT_SAMPLE_STEPS: u32 = 10;
pub(super) const VERTICAL_FIRST_AXIS_ORDERS: [[usize; 3]; 4] = [
    [AXIS_Y_INDEX, AXIS_X_INDEX, AXIS_Z_INDEX],
    [AXIS_X_INDEX, AXIS_Y_INDEX, AXIS_Z_INDEX],
    [AXIS_X_INDEX, AXIS_Z_INDEX, AXIS_Y_INDEX],
    [AXIS_Z_INDEX, AXIS_X_INDEX, AXIS_Y_INDEX],
];
