//! Routing traits and the `Router` compositor.
//!
//! Three traits define the routing abstraction:
//! - [`RouteSolver`] — the main interface (produces complete `CableGeometry`)
//! - [`PathPlanner`] — finds waypoints around obstacles
//! - [`CurveSolver`] — generates smooth geometry between two waypoints
//!
//! [`Router`] composes a `PathPlanner` and `CurveSolver` into a `RouteSolver`.

use bevy::math::Vec3;
use bevy_kana::ToUsize;

use super::constants::DEFAULT_RESOLUTION;
use super::constants::MIN_CABLE_SAMPLE_POINTS;
use super::geometry::CableGeometry;
use super::geometry::CableSegment;
use super::geometry::RouteRequest;
use super::obstacle::Obstacle;

/// The primary routing trait. Produces complete cable geometry from a route request.
///
/// Simple solvers (like `CatenarySolver`) implement this directly.
/// Composite solvers use [`Router`] to combine a [`PathPlanner`] with a [`CurveSolver`].
pub trait RouteSolver: Send + Sync {
    /// Compute the full cable geometry for the given request.
    fn solve(&self, request: &RouteRequest) -> CableGeometry;
}

/// Finds waypoints around obstacles. Returns an ordered list of points
/// that the cable should pass through (always includes start and end).
///
/// Trait methods take `Vec3` rather than `impl Into<Vec3>` because `PathPlanner`
/// must stay dyn-compatible (it is used as `Box<dyn PathPlanner>` in `Router`).
pub trait PathPlanner: Send + Sync {
    /// Plan a path from `start` to `end`, routing around `obstacles`.
    fn plan(&self, start: Vec3, end: Vec3, obstacles: &[Obstacle]) -> Vec<Vec3>;
}

/// Generates smooth curve geometry between two points.
/// No obstacle awareness — that's the planner's job.
///
/// Trait methods take `Vec3` rather than `impl Into<Vec3>` because `CurveSolver`
/// must stay dyn-compatible (it is used as `Box<dyn CurveSolver>` in `Router`).
pub trait CurveSolver: Send + Sync {
    /// Produce a `CableSegment` of smooth geometry between `start` and `end`.
    fn solve_segment(&self, start: Vec3, end: Vec3, resolution: u32) -> CableSegment;
}

/// Trivial planner that returns a direct path with no obstacle avoidance.
#[derive(Clone, Debug, Default)]
pub struct DirectPlanner;

impl PathPlanner for DirectPlanner {
    fn plan(&self, start: Vec3, end: Vec3, obstacles: &[Obstacle]) -> Vec<Vec3> {
        let _ = obstacles;
        vec![start, end]
    }
}

/// Trivial curve solver that produces a straight line between two points.
#[derive(Clone, Debug, Default)]
pub struct LinearSolver;

impl CurveSolver for LinearSolver {
    fn solve_segment(&self, start: Vec3, end: Vec3, resolution: u32) -> CableSegment {
        CableSegment::straight_line(
            start,
            end,
            resolution.max(MIN_CABLE_SAMPLE_POINTS).to_usize(),
        )
    }
}

impl RouteSolver for LinearSolver {
    fn solve(&self, request: &RouteRequest) -> CableGeometry {
        let segment = self.solve_segment(request.start, request.end, request.resolution);
        let waypoints = vec![request.start, request.end];
        CableGeometry::from_segments(vec![segment], waypoints)
    }
}

/// Composes a [`PathPlanner`] and [`CurveSolver`] into a [`RouteSolver`].
///
/// The `path_planner` finds waypoints, then the `curve_solver` fills each
/// waypoint-to-waypoint gap with smooth geometry.
pub struct Router {
    path_planner: Box<dyn PathPlanner>,
    curve_solver: Box<dyn CurveSolver>,
    resolution:   u32,
}

impl Router {
    /// Create a new `Router` composing a planner and curve solver.
    pub fn new(
        path_planner: impl PathPlanner + 'static,
        curve_solver: impl CurveSolver + 'static,
    ) -> Self {
        Self {
            path_planner: Box::new(path_planner),
            curve_solver: Box::new(curve_solver),
            resolution:   DEFAULT_RESOLUTION,
        }
    }

    /// Override the default sample resolution.
    #[must_use]
    pub const fn with_resolution(mut self, resolution: u32) -> Self {
        self.resolution = resolution;
        self
    }
}

impl RouteSolver for Router {
    fn solve(&self, request: &RouteRequest) -> CableGeometry {
        let waypoints = self
            .path_planner
            .plan(request.start, request.end, request.obstacles);
        let resolution = request.effective_resolution(self.resolution);
        let segments: Vec<CableSegment> = waypoints
            .windows(2)
            .map(|pair| {
                self.curve_solver
                    .solve_segment(pair[0], pair[1], resolution)
            })
            .collect();

        CableGeometry::from_segments(segments, waypoints)
    }
}
