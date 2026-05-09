//! Catenary curve math and solver.
//!
//! The catenary curve `y = a * cosh(x/a)` describes how a cable hangs under gravity
//! between two fixed points. This module provides standalone math functions and a
//! [`CatenarySolver`] that implements [`RouteSolver`] and [`CurveSolver`].
//!
//! # 3D Catenary Approach
//!
//! 1. Project the problem into a 2D plane containing both endpoints and the gravity vector
//! 2. Solve the 2D catenary for parameter `a` using Newton's method
//! 3. Sample points along the 2D curve
//! 4. Map sampled points back to 3D

use bevy::math::Vec3;
use bevy::reflect::Reflect;
use bevy_kana::ToF32;
use bevy_kana::ToUsize;

use super::constants::DEFAULT_GRAVITY;
use super::constants::DEFAULT_RESOLUTION;
use super::constants::DEFAULT_SLACK;
use super::constants::MAX_NEWTON_ITERATIONS;
use super::constants::MIN_CABLE_SAMPLE_POINTS;
use super::constants::MIN_CATENARY_PARAM;
use super::constants::MIN_SEGMENT_LENGTH;
use super::constants::NEAR_TAUT_INITIAL_GUESS_MULTIPLIER;
use super::constants::NEAR_ZERO_GRAVITY_THRESHOLD;
use super::constants::NEWTON_TOLERANCE;
use super::constants::STRAIGHT_LINE_THRESHOLD;
use super::geometry::CableGeometry;
use super::geometry::CableSegment;
use super::geometry::RouteRequest;
use super::solver::CurveSolver;
use super::solver::RouteSolver;

/// Evaluate the catenary function: `a * cosh(x / a)`.
#[must_use]
pub fn evaluate(x: f32, a: f32) -> f32 { a * (x / a).cosh() }

/// Solve for the catenary parameter `a` given horizontal distance, vertical distance,
/// and cable length, using Newton's method.
///
/// The cable length `L` must satisfy `L > sqrt(h^2 + v^2)` (longer than the straight line).
///
/// Returns `None` if the problem is degenerate or Newton's method fails to converge.
#[must_use]
pub fn solve_parameter(
    horizontal_distance: f32,
    vertical_distance: f32,
    cable_length: f32,
) -> Option<f32> {
    let horizontal = horizontal_distance.abs();
    let vertical = vertical_distance;
    let length = cable_length;

    // Cable must be longer than straight-line distance
    let straight = horizontal.hypot(vertical);
    if length <= straight + MIN_SEGMENT_LENGTH {
        return None;
    }

    // If horizontal distance is near zero, degenerate to vertical hang
    if horizontal < MIN_SEGMENT_LENGTH {
        return None;
    }

    // We need to solve: L^2 - v^2 = (2a * sinh(h / (2a)))^2
    // Let f(a) = 2a * sinh(h/(2a)) - sqrt(L^2 - v^2)
    // Newton: a_{n+1} = a_n - f(a_n) / f'(a_n)
    let target = length.mul_add(length, -(vertical * vertical)).sqrt();

    // Initial guess using the large-`a` approximation:
    // 2a*sinh(h/(2a)) ≈ h + h³/(24a²) = target  →  a = h * sqrt(h / (24*(target - h)))
    // This is far more stable than `a = h` which puts us near a zero of f'(a).
    let excess = target - horizontal;
    let mut param = if excess > MIN_SEGMENT_LENGTH {
        horizontal * (horizontal / (24.0 * excess)).sqrt()
    } else {
        // target ≈ h means near-taut cable; start with a large `a`
        horizontal * NEAR_TAUT_INITIAL_GUESS_MULTIPLIER
    };

    for _ in 0..MAX_NEWTON_ITERATIONS {
        if param < MIN_CATENARY_PARAM {
            param = MIN_CATENARY_PARAM;
        }

        let half_horizontal_over_a = horizontal / (2.0 * param);
        let sinh = half_horizontal_over_a.sinh();
        let cosh = half_horizontal_over_a.cosh();

        let residual = (2.0 * param).mul_add(sinh, -target);
        // f'(a) = 2*sinh(h/(2a)) - (h/a)*cosh(h/(2a))
        let f_prime = 2.0f32.mul_add(sinh, -(horizontal / param) * cosh);

        if f_prime.abs() < f32::EPSILON {
            break;
        }

        let delta = residual / f_prime;
        param -= delta;

        if delta.abs() < NEWTON_TOLERANCE {
            return (param > MIN_CATENARY_PARAM).then_some(param);
        }
    }

    // Failed to converge — return current best if reasonable
    (param > MIN_CATENARY_PARAM).then_some(param)
}

/// Sample points along a 3D catenary curve between `start` and `end`.
///
/// The curve sags in the direction of `gravity_direction` (should be normalized).
/// `slack` is the ratio of cable length to straight-line distance (1.0 = taut).
/// `resolution` is the number of sample points (minimum 2).
#[must_use]
pub fn sample_3d(
    start: impl Into<Vec3>,
    end: impl Into<Vec3>,
    slack: f32,
    gravity_direction: Vec3,
    resolution: u32,
) -> CableSegment {
    let start: Vec3 = start.into();
    let end: Vec3 = end.into();
    let n = resolution.max(MIN_CABLE_SAMPLE_POINTS).to_usize();
    let chord = end - start;
    let chord_length = chord.length();

    // Degenerate: endpoints are the same point
    if chord_length < MIN_SEGMENT_LENGTH {
        return CableSegment::from_points(vec![start; n]);
    }

    let clamped_slack = slack.max(1.0);
    let cable_length = chord_length * clamped_slack;
    let gravity_norm = gravity_direction.normalize_or_zero();

    // Near-taut cables degrade gracefully to a straight line
    if clamped_slack < STRAIGHT_LINE_THRESHOLD {
        return sample_straight_line(start, end, n);
    }

    // If gravity is zero, fall back to straight line
    if gravity_norm.length_squared() < NEAR_ZERO_GRAVITY_THRESHOLD {
        return sample_straight_line(start, end, n);
    }

    // Project the problem into a 2D plane:
    // - horizontal axis: along the chord direction, projected onto the plane perpendicular to
    //   gravity
    // - vertical axis: along gravity

    // Decompose chord into horizontal and vertical components
    let vertical_component = chord.dot(gravity_norm);
    let horizontal_vec = chord - vertical_component * gravity_norm;
    let horizontal_distance = horizontal_vec.length();

    // If cable is purely vertical, handle as a special case
    if horizontal_distance < MIN_SEGMENT_LENGTH {
        return sample_vertical_hang(start, end, gravity_norm, cable_length, n);
    }

    let horizontal_axis = horizontal_vec / horizontal_distance;

    // Solve for catenary parameter
    let Some(catenary_a) = solve_parameter(horizontal_distance, vertical_component, cable_length)
    else {
        return sample_parabolic_fallback(start, end, gravity_norm, clamped_slack, n);
    };

    // The 2D catenary: y = a * cosh((x - x_offset) / a) + y_offset
    // With boundary conditions at x=0 (start) and x=h (end)

    // Find the horizontal offset of the catenary's lowest point
    // x_offset = h/2 - a * arcsinh(v / (2a * sinh(h/(2a))))
    let half_horizontal = horizontal_distance / 2.0;
    let sinh_half_horizontal_over_a = (half_horizontal / catenary_a).sinh();
    let x_offset = if sinh_half_horizontal_over_a.abs() > f32::EPSILON {
        catenary_a.mul_add(
            -(vertical_component / (2.0 * catenary_a * sinh_half_horizontal_over_a)).asinh(),
            half_horizontal,
        )
    } else {
        half_horizontal
    };

    // y_offset positions the curve so it passes through the start point
    let y_at_start = catenary_a * ((0.0 - x_offset) / catenary_a).cosh();
    let y_offset = -y_at_start;

    // y_2d at the end point — needed to separate the linear height change from sag
    let y_2d_end = catenary_a.mul_add(
        ((horizontal_distance - x_offset) / catenary_a).cosh(),
        y_offset,
    );

    // Sample points along the 2D catenary and map back to 3D
    let points: Vec<Vec3> = (0..n)
        .map(|i| {
            let t = i.to_f32() / (n - 1).to_f32();
            let x_2d = t * horizontal_distance;
            let y_2d = catenary_a.mul_add(((x_2d - x_offset) / catenary_a).cosh(), y_offset);

            // `y_2d` encodes two things: the linear height change between endpoints
            // and the catenary sag. We need them mapped with opposite signs:
            //   - linear part: along gravity (preserves endpoint positions)
            //   - sag part: against gravity (cable hangs downward)
            let y_linear = t * y_2d_end;
            let y_sag = y_2d - y_linear;
            start + x_2d * horizontal_axis + (y_linear - y_sag) * gravity_norm
        })
        .collect();

    CableSegment::from_points(points)
}

/// Fallback for degenerate cases: straight line between two points.
fn sample_straight_line(start: Vec3, end: Vec3, n: usize) -> CableSegment {
    CableSegment::straight_line(start, end, n)
}

/// Handle purely vertical cables (start and end aligned with gravity).
fn sample_vertical_hang(
    start: Vec3,
    end: Vec3,
    gravity_norm: Vec3,
    cable_length: f32,
    n: usize,
) -> CableSegment {
    let vertical_distance = (end - start).dot(gravity_norm);
    let excess = cable_length - vertical_distance.abs();

    if excess < MIN_SEGMENT_LENGTH {
        return sample_straight_line(start, end, n);
    }

    // Vertical cable with slack forms a U-shape hanging down then back up
    // Midpoint hangs down by excess/2
    let midpoint = (start + end) / 2.0 + gravity_norm * (excess / 2.0);
    let half_n = n / 2;

    let points: Vec<Vec3> = (0..half_n)
        .map(|i| {
            let t = i.to_f32() / half_n.to_f32();
            start.lerp(midpoint, t)
        })
        .chain((0..(n - half_n)).map(|i| {
            let t = i.to_f32() / (n - half_n - 1).max(1).to_f32();
            midpoint.lerp(end, t)
        }))
        .collect();

    CableSegment::from_points(points)
}

/// Parabolic approximation when Newton's method fails to find a catenary parameter.
/// Uses `y = 4 * sag * t * (1 - t)` for a simple droop.
fn sample_parabolic_fallback(
    start: Vec3,
    end: Vec3,
    gravity_norm: Vec3,
    slack: f32,
    n: usize,
) -> CableSegment {
    let chord = end - start;
    let chord_length = chord.length();
    let sag = chord_length * (slack - 1.0).max(0.0) * 0.5;

    let points: Vec<Vec3> = (0..n)
        .map(|i| {
            let t = i.to_f32() / (n - 1).to_f32();
            let base = start + t * chord;
            let droop = 4.0 * sag * t * (1.0 - t);
            base + droop * gravity_norm
        })
        .collect();

    CableSegment::from_points(points)
}

/// Solver that computes catenary curves between cable endpoints.
///
/// Implements both [`CurveSolver`] (for use with [`Router`]) and [`RouteSolver`]
/// (for standalone use without obstacle avoidance).
#[derive(Clone, Debug, Reflect)]
pub struct CatenarySolver {
    /// Cable length / straight-line distance. Values > 1.0 add sag.
    pub slack:             f32,
    /// Gravity direction (not necessarily normalized; magnitude is ignored).
    pub gravity:           Vec3,
    /// Default sample resolution when not specified by the request.
    pub resolution:        u32,
    /// Additional slack applied when a cable endpoint detaches. `None` disables the bump.
    pub detach_slack_bump: Option<f32>,
}

impl Default for CatenarySolver {
    fn default() -> Self {
        Self {
            slack:             DEFAULT_SLACK,
            gravity:           DEFAULT_GRAVITY,
            resolution:        DEFAULT_RESOLUTION,
            detach_slack_bump: None,
        }
    }
}

impl CatenarySolver {
    /// Create a catenary solver with default parameters.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slack:             DEFAULT_SLACK,
            gravity:           DEFAULT_GRAVITY,
            resolution:        DEFAULT_RESOLUTION,
            detach_slack_bump: None,
        }
    }

    /// Set the slack factor.
    #[must_use]
    pub const fn with_slack(mut self, slack: f32) -> Self {
        self.slack = slack;
        self
    }

    /// Set the gravity vector.
    #[must_use]
    pub const fn with_gravity(mut self, gravity: Vec3) -> Self {
        self.gravity = gravity;
        self
    }

    /// Set the default sample resolution.
    #[must_use]
    pub const fn with_resolution(mut self, resolution: u32) -> Self {
        self.resolution = resolution;
        self
    }

    /// Configure extra slack to apply when any endpoint of the owning cable detaches.
    #[must_use]
    pub const fn with_detach_slack_bump(mut self, bump: f32) -> Self {
        self.detach_slack_bump = Some(bump);
        self
    }
}

impl CurveSolver for CatenarySolver {
    fn solve_segment(&self, start: Vec3, end: Vec3, resolution: u32) -> CableSegment {
        let gravity_direction = self.gravity.normalize_or_zero();
        sample_3d(start, end, self.slack, gravity_direction, resolution)
    }
}

impl RouteSolver for CatenarySolver {
    fn solve(&self, request: &RouteRequest) -> CableGeometry {
        let resolution = request.effective_resolution(self.resolution);
        let segment = self.solve_segment(request.start, request.end, resolution);
        let waypoints = vec![request.start, request.end];
        CableGeometry::from_segments(vec![segment], waypoints)
    }
}
