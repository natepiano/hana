#![allow(
    clippy::used_underscore_binding,
    reason = "false positive on enum variant fields"
)]

//! Enum-based solver selection for cables.
//!
//! Replaces `Box<dyn RouteSolver>` with concrete enums that support `Clone`, `Reflect`,
//! and avoid heap allocation. The existing traits remain as internal implementation details.

use bevy::math::Vec3;
use bevy::reflect::Reflect;

use super::catenary::CatenarySolver;
use super::constants::DEFAULT_RESOLUTION;
use super::constants::DEFAULT_RESOLUTION_SENTINEL;
use super::geometry::CableGeometry;
use super::geometry::CableSegment;
use super::geometry::RouteRequest;
use super::obstacle::Obstacle;
use super::orthogonal::OrthogonalPlanner;
use super::pathfinding::AStarPlanner;
use super::solver::CurveSolver;
use super::solver::DirectPlanner;
use super::solver::LinearSolver;
use super::solver::PathPlanner;
use super::solver::RouteSolver;

/// Top-level solver selection for a cable.
#[derive(Clone, Debug, Reflect)]
pub enum Solver {
    /// Direct catenary curve between endpoints.
    Catenary(CatenarySolver),
    /// Straight line between endpoints.
    Linear,
    /// Path planner + curve solver composition.
    Routed {
        /// How to find waypoints around obstacles.
        path_strategy: PathStrategy,
        /// How to generate curves between waypoints.
        curve_kind:    CurveKind,
        /// Sample resolution per segment (0 = use solver default).
        resolution:    u32,
    },
}

/// Path planning strategy (finds waypoints around obstacles).
#[derive(Clone, Debug, Reflect)]
pub enum PathStrategy {
    /// No obstacle avoidance — direct path.
    Direct,
    /// Orthogonal (right-angle) routing.
    Orthogonal,
    /// A* grid-based pathfinding.
    AStar,
}

/// Curve generation strategy (fills between waypoints).
#[derive(Clone, Debug, Reflect)]
pub enum CurveKind {
    /// Catenary (hanging cable) curve.
    Catenary(CatenarySolver),
    /// Straight line segment.
    Linear,
}

impl Solver {
    /// Dispatch to the underlying solver implementation.
    #[must_use]
    pub fn solve(&self, request: &RouteRequest) -> CableGeometry {
        match self {
            Self::Catenary(catenary) => catenary.solve(request),
            Self::Linear => LinearSolver.solve(request),
            Self::Routed {
                path_strategy,
                curve_kind,
                resolution,
            } => {
                let waypoints = path_strategy.plan(request.start, request.end, request.obstacles);
                let default_resolution = if *resolution == DEFAULT_RESOLUTION_SENTINEL {
                    DEFAULT_RESOLUTION
                } else {
                    *resolution
                };
                let resolution = request.effective_resolution(default_resolution);

                let segments: Vec<CableSegment> = waypoints
                    .windows(2)
                    .map(|pair| curve_kind.solve_segment(pair[0], pair[1], resolution))
                    .collect();

                CableGeometry::from_segments(segments, waypoints)
            },
        }
    }
}

impl PathStrategy {
    /// Find waypoints from `start` to `end`, routing around `obstacles`.
    fn plan(&self, start: Vec3, end: Vec3, obstacles: &[Obstacle]) -> Vec<Vec3> {
        match self {
            Self::Direct => DirectPlanner.plan(start, end, obstacles),
            Self::Orthogonal => OrthogonalPlanner::new().plan(start, end, obstacles),
            Self::AStar => AStarPlanner::new().plan(start, end, obstacles),
        }
    }
}

impl CurveKind {
    /// Generate a curve segment between two waypoints.
    fn solve_segment(&self, start: Vec3, end: Vec3, resolution: u32) -> CableSegment {
        match self {
            Self::Catenary(catenary) => catenary.solve_segment(start, end, resolution),
            Self::Linear => LinearSolver.solve_segment(start, end, resolution),
        }
    }
}
