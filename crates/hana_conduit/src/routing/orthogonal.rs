//! Orthogonal routing — axis-aligned cable paths with 90-degree bends.

use bevy::math::Vec3;

use super::constants::DEFAULT_OBSTACLE_MARGIN;
use super::constants::HORIZONTAL_FIRST_AXIS_ORDERS;
use super::constants::OBSTACLE_CLEARANCE_MULTIPLIER;
use super::constants::ORTHOGONAL_SEGMENT_SAMPLE_STEPS;
use super::constants::VERTICAL_FIRST_AXIS_ORDERS;
use super::obstacle;
use super::obstacle::Blockage;
use super::obstacle::Obstacle;
use super::solver::PathPlanner;

/// Whether to route vertically or horizontally first.
#[derive(Clone, Debug, Default)]
pub enum AxisOrder {
    /// Route horizontally (X/Z) before vertically (Y).
    #[default]
    HorizontalFirst,
    /// Route vertically (Y) before horizontally (X/Z).
    VerticalFirst,
}

/// Plans axis-aligned cable paths with 90-degree bends.
///
/// Routes cables along the primary axes (X, Y, Z), choosing the most
/// direct orthogonal path from start to end. Avoids obstacles when present.
#[derive(Clone, Debug)]
pub struct OrthogonalPlanner {
    /// Clearance around obstacles.
    pub margin:     f32,
    /// Axis routing order.
    pub axis_order: AxisOrder,
}

impl OrthogonalPlanner {
    /// Create an orthogonal planner with default settings.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            margin:     DEFAULT_OBSTACLE_MARGIN,
            axis_order: AxisOrder::HorizontalFirst,
        }
    }

    /// Set the obstacle clearance margin.
    #[must_use]
    pub const fn with_margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }

    /// Prefer vertical-first routing.
    #[must_use]
    pub const fn vertical_first(mut self) -> Self {
        self.axis_order = AxisOrder::VerticalFirst;
        self
    }

    /// Check if an axis-aligned segment between two points is blocked.
    fn is_segment_blocked(&self, start: Vec3, end: Vec3, obstacles: &[Obstacle]) -> Blockage {
        obstacle::is_segment_blocked(
            start,
            end,
            obstacles,
            self.margin,
            ORTHOGONAL_SEGMENT_SAMPLE_STEPS,
        )
    }

    /// Build an axis-aligned path moving one axis at a time in the given order.
    /// Each step changes exactly one of X, Y, Z.
    fn axis_path(start: Vec3, end: Vec3, order: &[usize]) -> Vec<Vec3> {
        let delta = end - start;
        let axis_steps = [
            Vec3::new(delta.x, 0.0, 0.0),
            Vec3::new(0.0, delta.y, 0.0),
            Vec3::new(0.0, 0.0, delta.z),
        ];
        let mut current = start;
        let mut waypoints = vec![start];

        for &axis in order {
            let step = axis_steps[axis];

            if step.length_squared() > f32::EPSILON {
                current += step;
                waypoints.push(current);
            }
        }

        // Ensure we end exactly at the target
        if waypoints
            .last()
            .is_some_and(|last| last.distance(end) > f32::EPSILON)
        {
            waypoints.push(end);
        }

        waypoints
    }

    fn push_distinct_waypoint(waypoints: &mut Vec<Vec3>, waypoint: Vec3) {
        if waypoints
            .last()
            .is_none_or(|last| last.distance_squared(waypoint) > f32::EPSILON)
        {
            waypoints.push(waypoint);
        }
    }

    /// Generate a U-shaped orthogonal path with `start`, `below_start`,
    /// `below_corner`, `below_end`, and `end` waypoints.
    fn u_path(&self, start: Vec3, end: Vec3, obstacles: &[Obstacle]) -> Vec<Vec3> {
        // `offset` uses the largest `Obstacle::half_extents.max_element()` plus `margin`.
        let offset = self.margin.mul_add(
            OBSTACLE_CLEARANCE_MULTIPLIER,
            obstacles.iter().fold(0.0_f32, |acc, obstacle| {
                let extent = obstacle.half_extents.max_element();
                acc.max(extent)
            }),
        );

        // Build `below_start`, `below_corner`, and `below_end` below the route
        // endpoints and every obstacle's expanded lower face.
        let route_min_y = start.y.min(end.y);
        let obstacle_min_y = obstacles.iter().fold(route_min_y, |acc, obstacle| {
            acc.min(obstacle.position.y - obstacle.half_extents.y - self.margin)
        });
        let below = obstacle_min_y - offset;
        let below_start = Vec3::new(start.x, below, start.z);
        let below_corner = Vec3::new(end.x, below, start.z);
        let below_end = Vec3::new(end.x, below, end.z);

        let mut waypoints = Vec::with_capacity(5);
        for waypoint in [start, below_start, below_corner, below_end, end] {
            Self::push_distinct_waypoint(&mut waypoints, waypoint);
        }

        waypoints
    }

    /// Check if any segment of a multi-waypoint path is blocked.
    fn is_path_blocked(&self, waypoints: &[Vec3], obstacles: &[Obstacle]) -> Blockage {
        for pair in waypoints.windows(2) {
            match self.is_segment_blocked(pair[0], pair[1], obstacles) {
                Blockage::Blocked => return Blockage::Blocked,
                Blockage::Clear => {},
            }
        }

        Blockage::Clear
    }
}

impl Default for OrthogonalPlanner {
    fn default() -> Self {
        Self {
            margin:     DEFAULT_OBSTACLE_MARGIN,
            axis_order: AxisOrder::default(),
        }
    }
}

impl PathPlanner for OrthogonalPlanner {
    fn plan(&self, start: Vec3, end: Vec3, obstacles: &[Obstacle]) -> Vec<Vec3> {
        // `axis_order` selects `VERTICAL_FIRST_AXIS_ORDERS` or
        // `HORIZONTAL_FIRST_AXIS_ORDERS`.
        let orders = if matches!(self.axis_order, AxisOrder::VerticalFirst) {
            &VERTICAL_FIRST_AXIS_ORDERS
        } else {
            &HORIZONTAL_FIRST_AXIS_ORDERS
        };

        for order in orders {
            let path = Self::axis_path(start, end, order);
            if obstacles.is_empty() {
                return path;
            }

            match self.is_path_blocked(&path, obstacles) {
                Blockage::Clear => return path,
                Blockage::Blocked => {},
            }
        }

        // `Blockage::Blocked` for every axis path uses `u_path`.
        self.u_path(start, end, obstacles)
    }
}
