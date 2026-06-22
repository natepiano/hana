//! Pure math routing module for cable geometry computation.
//!
//! This module depends only on `glam` (via `bevy::math`) — no Bevy ECS, no rendering.
//! It produces [`CableGeometry`] from a [`RouteRequest`] via the [`RouteSolver`] trait.

mod catenary;
mod constants;
mod geometry;
mod obstacle;
mod orthogonal;
mod pathfinding;
mod solver;
mod solver_selection;

pub use catenary::CatenarySolver;
pub use catenary::evaluate;
pub use catenary::sample_3d;
pub use catenary::solve_parameter;
pub use constants::DEFAULT_GRAVITY;
pub use constants::DEFAULT_RESOLUTION;
pub use constants::DEFAULT_SLACK;
pub(crate) use constants::MIN_CABLE_SAMPLE_POINTS;
pub(crate) use constants::MIN_SEGMENT_LENGTH;
pub use geometry::Anchor;
pub use geometry::CableGeometry;
pub use geometry::CableSegment;
pub use geometry::RouteRequest;
pub use obstacle::Obstacle;
pub use orthogonal::AxisOrder;
pub use orthogonal::OrthogonalPlanner;
pub use pathfinding::AStarPlanner;
pub use solver::CurveSolver;
pub use solver::DirectPlanner;
pub use solver::LinearSolver;
pub use solver::PathPlanner;
pub use solver::RouteSolver;
pub use solver::Router;
pub use solver_selection::CurveKind;
pub use solver_selection::PathStrategy;
pub use solver_selection::Solver;
