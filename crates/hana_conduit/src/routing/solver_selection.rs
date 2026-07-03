#![allow(
    clippy::used_underscore_binding,
    reason = "false positive on enum variant fields"
)]

//! Enum-based solver selection for cables.
//!
//! Replaces `Box<dyn RouteSolver>` with `Solver`, `PathStrategy`, and `CurveKind`.
//! `RouteSolver`, `PathPlanner`, and `CurveSolver` remain as internal implementation
//! details.

use bevy::math::Vec3;
use bevy::reflect::Reflect;
use bevy_kana::ToUsize;

use super::catenary::CatenarySolver;
use super::constants::DEFAULT_RESOLUTION;
use super::constants::DEFAULT_RESOLUTION_SENTINEL;
use super::constants::MIN_CABLE_SAMPLE_POINTS;
use super::constants::MIN_SEGMENT_LENGTH;
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
    AStar {
        /// Voxel size of the search grid, in metres.
        grid_size: f32,
        /// Clearance kept around obstacles, in metres.
        margin:    f32,
    },
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
    ///
    /// Anchors with an [`AnchorExit::Lead`](super::geometry::AnchorExit) exit
    /// contribute a straight lead segment first; the solver routes between the
    /// lead tips. This runs here, above the individual solvers, so every path
    /// strategy and curve kind honors leads.
    #[must_use]
    pub fn solve(&self, request: &RouteRequest) -> CableGeometry {
        let start_tip = request.start.lead_tip();
        let end_tip = request.end.lead_tip();
        let span_start = start_tip.unwrap_or(request.start.position);
        let span_end = end_tip.unwrap_or(request.end.position);

        // Leads that leave the tips closer than `MIN_SEGMENT_LENGTH` (e.g. a
        // drag hovering next to the source jack) collapse the routed span;
        // route the bare anchor positions instead.
        if span_start.distance(span_end) < MIN_SEGMENT_LENGTH {
            return self.solve_span(request.start.position, request.end.position, request);
        }

        let span = self.solve_span(span_start, span_end, request);
        wrap_with_leads(span, request)
    }

    /// Route between two bare positions, ignoring anchor exits.
    fn solve_span(&self, start: Vec3, end: Vec3, request: &RouteRequest) -> CableGeometry {
        let span_request = RouteRequest {
            start:      start.into(),
            end:        end.into(),
            obstacles:  request.obstacles,
            resolution: request.resolution,
        };

        match self {
            Self::Catenary(catenary) => catenary.solve(&span_request),
            Self::Linear => LinearSolver.solve(&span_request),
            Self::Routed {
                path_strategy,
                curve_kind,
                resolution,
            } => {
                let waypoints = path_strategy.plan(start, end, span_request.obstacles);
                let default_resolution = if *resolution == DEFAULT_RESOLUTION_SENTINEL {
                    DEFAULT_RESOLUTION
                } else {
                    *resolution
                };
                let resolution = span_request.effective_resolution(default_resolution);

                let segments: Vec<CableSegment> = waypoints
                    .windows(2)
                    .map(|pair| curve_kind.solve_segment(pair[0], pair[1], resolution))
                    .collect();

                CableGeometry::from_segments(segments, waypoints)
            },
        }
    }
}

/// Wrap a routed span with the straight lead segments declared by the request's
/// anchors, extending the waypoint list back out to the true anchor positions.
fn wrap_with_leads(span: CableGeometry, request: &RouteRequest) -> CableGeometry {
    let start_tip = request.start.lead_tip();
    let end_tip = request.end.lead_tip();
    if start_tip.is_none() && end_tip.is_none() {
        return span;
    }

    let mut segments = Vec::with_capacity(span.segments.len() + 2);
    let mut waypoints = Vec::with_capacity(span.waypoints.len() + 2);
    if let Some(tip) = start_tip {
        segments.push(CableSegment::straight_line(
            request.start.position,
            tip,
            MIN_CABLE_SAMPLE_POINTS.to_usize(),
        ));
        waypoints.push(request.start.position);
    }
    segments.extend(span.segments);
    waypoints.extend(span.waypoints);
    if let Some(tip) = end_tip {
        segments.push(CableSegment::straight_line(
            tip,
            request.end.position,
            MIN_CABLE_SAMPLE_POINTS.to_usize(),
        ));
        waypoints.push(request.end.position);
    }

    CableGeometry::from_segments(segments, waypoints)
}

impl PathStrategy {
    /// Find waypoints from `start` to `end`, routing around `obstacles`.
    fn plan(&self, start: Vec3, end: Vec3, obstacles: &[Obstacle]) -> Vec<Vec3> {
        match self {
            Self::Direct => DirectPlanner.plan(start, end, obstacles),
            Self::Orthogonal => OrthogonalPlanner::new().plan(start, end, obstacles),
            Self::AStar { grid_size, margin } => AStarPlanner::new()
                .with_grid_size(*grid_size)
                .with_margin(*margin)
                .plan(start, end, obstacles),
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
