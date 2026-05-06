#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
//! Comprehensive tests for the `bevy_catenary` routing module.

use bevy::math::Vec3;
use bevy_catenary::AStarPlanner;
use bevy_catenary::Anchor;
use bevy_catenary::CableSegment;
use bevy_catenary::CatenarySolver;
use bevy_catenary::CurveSolver;
use bevy_catenary::DirectPlanner;
use bevy_catenary::LinearSolver;
use bevy_catenary::Obstacle;
use bevy_catenary::OrthogonalPlanner;
use bevy_catenary::PathPlanner;
use bevy_catenary::RouteRequest;
use bevy_catenary::RouteSolver;
use bevy_catenary::Router;
use bevy_catenary::evaluate;
use bevy_catenary::sample_3d;
use bevy_catenary::solve_parameter;
use bevy_kana::ToF32;
use bevy_kana::ToUsize;

const TOLERANCE: f32 = 0.01;

/// Helper to assert two `Vec3` values are approximately equal.
fn assert_vec3_approx(actual: impl Into<Vec3>, expected: impl Into<Vec3>, label: &str) {
    let actual: Vec3 = actual.into();
    let expected: Vec3 = expected.into();
    let dist = actual.distance(expected);
    assert!(
        dist < TOLERANCE,
        "{label}: expected {expected}, got {actual} (distance {dist})"
    );
}

// ─────────────────────────────────────────────────────────────────────
// Catenary math: `solve_parameter`
// ─────────────────────────────────────────────────────────────────────

#[test]
fn solve_parameter_converges_for_normal_case() {
    // Validate convergence through `sample_3d` which calls `solve_parameter` internally
    // with the correct horizontal/vertical decomposition. A moderate slack (1.3x chord)
    // produces a visible catenary sag, confirming Newton's method converged.
    let start = Vec3::new(-2.0, 2.0, 0.0);
    let end = Vec3::new(2.0, 2.0, 0.0);
    let gravity = Vec3::new(0.0, -1.0, 0.0);

    let segment = sample_3d(start, end, 1.3, gravity, 32);

    // If `solve_parameter` converged, we get a real catenary with sag.
    let mid_idx = segment.points.len() / 2;
    let mid_y = segment.points[mid_idx].y;
    assert!(
        mid_y < 2.0,
        "midpoint y ({mid_y}) should sag below chord y (2.0), indicating convergence"
    );
}

#[test]
fn solve_parameter_converges_for_asymmetric_endpoints() {
    // Asymmetric: endpoints at different heights, with enough slack for a catenary.
    let start = Vec3::new(0.0, 5.0, 0.0);
    let end = Vec3::new(6.0, 2.0, 0.0);
    let gravity = Vec3::new(0.0, -1.0, 0.0);

    let segment = sample_3d(start, end, 1.4, gravity, 32);

    // A converged catenary should sag below the straight line between endpoints.
    let mid_idx = segment.points.len() / 2;
    let mid_y = segment.points[mid_idx].y;
    let chord_mid_y = f32::midpoint(start.y, end.y);
    assert!(
        mid_y < chord_mid_y,
        "midpoint y ({mid_y}) should sag below chord midpoint y ({chord_mid_y})"
    );
}

#[test]
fn solve_parameter_returns_none_when_cable_shorter_than_straight_line() {
    // Straight-line distance = sqrt(16+9) = 5, cable length = 4 < 5
    let result = solve_parameter(4.0, 3.0, 4.0);
    assert!(
        result.is_none(),
        "should return None when cable is shorter than straight-line distance"
    );
}

#[test]
fn solve_parameter_returns_none_for_zero_horizontal_distance() {
    let result = solve_parameter(0.0, 5.0, 6.0);
    assert!(
        result.is_none(),
        "should return None for zero horizontal distance"
    );
}

#[test]
fn solve_parameter_returns_none_when_cable_equals_straight_line() {
    // Cable exactly equals straight-line distance (no slack)
    let result = solve_parameter(3.0, 4.0, 5.0);
    assert!(
        result.is_none(),
        "should return None when cable length equals straight-line distance"
    );
}

// ─────────────────────────────────────────────────────────────────────
// Catenary math: `sample_3d`
// ─────────────────────────────────────────────────────────────────────

#[test]
fn sample_3d_returns_correct_number_of_points() {
    let start = Vec3::new(-2.0, 2.0, 0.0);
    let end = Vec3::new(2.0, 2.0, 0.0);
    let gravity = Vec3::new(0.0, -1.0, 0.0);
    let resolution = 20;

    let segment = sample_3d(start, end, 1.3, gravity, resolution);
    assert_eq!(
        segment.points.len(),
        resolution.to_usize(),
        "should return exactly {resolution} points"
    );
}

#[test]
fn sample_3d_endpoints_match_start_and_end() {
    let start = Vec3::new(-3.0, 1.0, 0.0);
    let end = Vec3::new(3.0, 1.0, 0.0);
    let gravity = Vec3::new(0.0, -1.0, 0.0);

    let segment = sample_3d(start, end, 1.3, gravity, 32);

    let first = *segment.points.first().expect("points should not be empty");
    let last = *segment.points.last().expect("points should not be empty");

    assert_vec3_approx(first, start, "first point should match start");
    assert_vec3_approx(last, end, "last point should match end");
}

#[test]
fn sample_3d_slack_one_gives_nearly_straight_line() {
    let start = Vec3::new(0.0, 5.0, 0.0);
    let end = Vec3::new(4.0, 5.0, 0.0);
    let gravity = Vec3::new(0.0, -1.0, 0.0);

    let segment = sample_3d(start, end, 1.0, gravity, 32);

    // With slack=1.0, the solver falls back to a parabolic curve with minimal sag.
    // Every point should be very close to the straight line (within a small tolerance
    // that accounts for the fallback's minimum sag factor).
    let nearly_straight_tolerance = 0.15;
    for (i, point) in segment.points.iter().enumerate() {
        let t = i.to_f32() / (segment.points.len() - 1).to_f32();
        let expected = start.lerp(end, t);
        let dist = point.distance(expected);
        assert!(
            dist < nearly_straight_tolerance,
            "point {i}: expected ~{expected:?}, got {point} (distance {dist})"
        );
    }
}

#[test]
fn sample_3d_high_slack_produces_visible_sag() {
    let start = Vec3::new(-3.0, 5.0, 0.0);
    let end = Vec3::new(3.0, 5.0, 0.0);
    let gravity = Vec3::new(0.0, -1.0, 0.0);

    let segment = sample_3d(start, end, 1.5, gravity, 32);

    // The midpoint of the chord is at y=5. With heavy sag, the cable's midpoint
    // should be below that line.
    let mid_idx = segment.points.len() / 2;
    let mid_y = segment.points[mid_idx].y;
    let chord_mid_y = f32::midpoint(start.y, end.y);

    assert!(
        mid_y < chord_mid_y - 0.1,
        "midpoint y ({mid_y}) should be significantly below chord midpoint y ({chord_mid_y})"
    );
}

#[test]
fn sample_3d_arc_lengths_are_monotonically_increasing() {
    let start = Vec3::new(-2.0, 3.0, 1.0);
    let end = Vec3::new(2.0, 1.0, -1.0);
    let gravity = Vec3::new(0.0, -1.0, 0.0);

    let segment = sample_3d(start, end, 1.3, gravity, 32);

    for i in 1..segment.arc_lengths.len() {
        assert!(
            segment.arc_lengths[i] >= segment.arc_lengths[i - 1],
            "arc length at index {i} ({}) should be >= previous ({})",
            segment.arc_lengths[i],
            segment.arc_lengths[i - 1]
        );
    }
}

#[test]
fn sample_3d_symmetric_sag_for_symmetric_endpoints() {
    // Start and end at same height, equidistant from origin along X.
    let start = Vec3::new(-4.0, 3.0, 0.0);
    let end = Vec3::new(4.0, 3.0, 0.0);
    let gravity = Vec3::new(0.0, -1.0, 0.0);

    let segment = sample_3d(start, end, 1.4, gravity, 33);
    let n = segment.points.len();

    // Compare point i and point (n-1-i): they should be mirror images in X.
    for i in 0..n / 2 {
        let left = segment.points[i];
        let right = segment.points[n - 1 - i];

        // Y values should be approximately equal (symmetric sag).
        let y_diff = (left.y - right.y).abs();
        assert!(
            y_diff < TOLERANCE,
            "points {i} and {} should have symmetric Y: left.y={}, right.y={}, diff={y_diff}",
            n - 1 - i,
            left.y,
            right.y
        );

        // X values should be mirror images around 0.
        let x_sum = (left.x + right.x).abs();
        assert!(
            x_sum < TOLERANCE,
            "points {i} and {} should have symmetric X: left.x={}, right.x={}, sum={x_sum}",
            n - 1 - i,
            left.x,
            right.x
        );
    }
}

// ─────────────────────────────────────────────────────────────────────
// Catenary math: `evaluate`
// ─────────────────────────────────────────────────────────────────────

#[test]
fn evaluate_at_zero_returns_a() {
    // cosh(0) = 1, so evaluate(0, a) = a * 1 = a
    let a = 2.5;
    let result = evaluate(0.0, a);
    assert!(
        (result - a).abs() < TOLERANCE,
        "evaluate(0, {a}) should equal {a}, got {result}"
    );
}

#[test]
fn evaluate_is_symmetric() {
    let a = 3.0;
    let x = 1.5;
    let y_pos = evaluate(x, a);
    let y_neg = evaluate(-x, a);
    assert!(
        (y_pos - y_neg).abs() < TOLERANCE,
        "evaluate should be symmetric: evaluate({x}, {a})={y_pos}, evaluate(-{x}, {a})={y_neg}"
    );
}

#[test]
fn evaluate_at_known_value() {
    // a * cosh(x/a) for a=1, x=0 => 1*cosh(0) = 1
    assert!((evaluate(0.0, 1.0) - 1.0).abs() < TOLERANCE);

    // a=1, x=1 => cosh(1) ~= 1.5431
    let expected = 1.0_f32.cosh();
    let result = evaluate(1.0, 1.0);
    assert!(
        (result - expected).abs() < TOLERANCE,
        "evaluate(1, 1) should be ~{expected}, got {result}"
    );
}

// ─────────────────────────────────────────────────────────────────────
// `CableSegment`
// ─────────────────────────────────────────────────────────────────────

#[test]
fn cable_segment_from_points_computes_correct_arc_lengths() {
    let points = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(1.0, 1.0, 1.0),
    ];
    let segment = CableSegment::from_points(points);

    // Arc lengths: 0, 1, 2, 3
    assert_eq!(segment.arc_lengths.len(), 4);
    assert!((segment.arc_lengths[0] - 0.0).abs() < TOLERANCE);
    assert!((segment.arc_lengths[1] - 1.0).abs() < TOLERANCE);
    assert!((segment.arc_lengths[2] - 2.0).abs() < TOLERANCE);
    assert!((segment.arc_lengths[3] - 3.0).abs() < TOLERANCE);
    assert!(
        (segment.length - 3.0).abs() < TOLERANCE,
        "total length should be 3.0, got {}",
        segment.length
    );
}

#[test]
fn cable_segment_from_points_single_point() {
    let segment = CableSegment::from_points(vec![Vec3::new(1.0, 2.0, 3.0)]);

    assert_eq!(segment.points.len(), 1);
    assert_eq!(segment.tangents.len(), 1);
    assert_eq!(segment.arc_lengths.len(), 1);
    assert!(
        (segment.length - 0.0).abs() < TOLERANCE,
        "single point segment should have zero length"
    );
    // Single point should have default tangent Vec3::Y
    assert_vec3_approx(segment.tangents[0], Vec3::Y, "single point tangent");
}

#[test]
fn cable_segment_from_points_empty() {
    let segment = CableSegment::from_points(vec![]);

    assert!(segment.points.is_empty());
    assert!(segment.tangents.is_empty());
    assert!(segment.arc_lengths.is_empty());
    assert!(
        (segment.length - 0.0).abs() < TOLERANCE,
        "empty segment should have zero length"
    );
}

// ─────────────────────────────────────────────────────────────────────
// Solvers: `DirectPlanner`
// ─────────────────────────────────────────────────────────────────────

#[test]
fn direct_planner_returns_start_and_end() {
    let planner = DirectPlanner;
    let start = Vec3::new(1.0, 2.0, 3.0);
    let end = Vec3::new(4.0, 5.0, 6.0);

    let waypoints = planner.plan(start, end, &[]);
    assert_eq!(waypoints.len(), 2);
    assert_vec3_approx(waypoints[0], start, "first waypoint");
    assert_vec3_approx(waypoints[1], end, "second waypoint");
}

#[test]
fn direct_planner_ignores_obstacles() {
    let planner = DirectPlanner;
    let start = Vec3::new(0.0, 0.0, 0.0);
    let end = Vec3::new(10.0, 0.0, 0.0);
    let obstacles = vec![Obstacle::new(Vec3::splat(1.0), Vec3::new(5.0, 0.0, 0.0))];

    let waypoints = planner.plan(start, end, &obstacles);
    assert_eq!(
        waypoints.len(),
        2,
        "`DirectPlanner` should ignore obstacles"
    );
}

// ─────────────────────────────────────────────────────────────────────
// Solvers: `LinearSolver`
// ─────────────────────────────────────────────────────────────────────

#[test]
fn linear_solver_returns_correct_number_of_points() {
    let solver = LinearSolver;
    let start = Vec3::new(0.0, 0.0, 0.0);
    let end = Vec3::new(5.0, 0.0, 0.0);
    let resolution = 16;

    let segment = solver.solve_segment(start, end, resolution);
    assert_eq!(
        segment.points.len(),
        resolution.to_usize(),
        "should return exactly {resolution} points"
    );
}

#[test]
fn linear_solver_produces_straight_line() {
    let solver = LinearSolver;
    let start = Vec3::new(0.0, 0.0, 0.0);
    let end = Vec3::new(6.0, 0.0, 0.0);

    let segment = solver.solve_segment(start, end, 10);

    for (i, point) in segment.points.iter().enumerate() {
        let t = i.to_f32() / (segment.points.len() - 1).to_f32();
        let expected = start.lerp(end, t);
        assert_vec3_approx(*point, expected, &format!("linear point {i}"));
    }
}

#[test]
fn linear_solver_as_route_solver() {
    let solver = LinearSolver;
    let request = RouteRequest {
        start:      Vec3::new(0.0, 0.0, 0.0),
        end:        Vec3::new(3.0, 4.0, 0.0),
        obstacles:  &[],
        resolution: 10,
    };

    let geometry = solver.solve(&request);
    assert_eq!(geometry.segments.len(), 1);
    assert_eq!(geometry.waypoints.len(), 2);
    assert_eq!(geometry.segments[0].points.len(), 10);

    let expected_length = request.start.distance(request.end);
    assert!(
        (geometry.total_length - expected_length).abs() < TOLERANCE,
        "total length should be ~{expected_length}, got {}",
        geometry.total_length
    );
}

// ─────────────────────────────────────────────────────────────────────
// Solvers: `Router` composition
// ─────────────────────────────────────────────────────────────────────

#[test]
fn router_composes_planner_and_curve_solver() {
    let router = Router::new(DirectPlanner, LinearSolver);
    let request = RouteRequest {
        start:      Vec3::new(0.0, 0.0, 0.0),
        end:        Vec3::new(5.0, 0.0, 0.0),
        obstacles:  &[],
        resolution: 20,
    };

    let geometry = router.solve(&request);

    // `DirectPlanner` produces [start, end] => one segment
    assert_eq!(geometry.segments.len(), 1);
    assert_eq!(geometry.waypoints.len(), 2);
    assert_eq!(geometry.segments[0].points.len(), 20);
}

#[test]
fn router_with_catenary_solver() {
    let router = Router::new(DirectPlanner, CatenarySolver::new().with_slack(1.3));
    let request = RouteRequest {
        start:      Vec3::new(-3.0, 2.0, 0.0),
        end:        Vec3::new(3.0, 2.0, 0.0),
        obstacles:  &[],
        resolution: 32,
    };

    let geometry = router.solve(&request);

    assert_eq!(geometry.segments.len(), 1);
    assert_eq!(geometry.segments[0].points.len(), 32);

    // Catenary should sag below the chord line
    let mid_idx = geometry.segments[0].points.len() / 2;
    let mid_y = geometry.segments[0].points[mid_idx].y;
    assert!(
        mid_y < 2.0,
        "catenary midpoint y ({mid_y}) should sag below chord (2.0)"
    );
}

#[test]
fn router_with_custom_resolution() {
    let router = Router::new(DirectPlanner, LinearSolver).with_resolution(64);
    let request = RouteRequest {
        start:      Vec3::new(0.0, 0.0, 0.0),
        end:        Vec3::X * 5.0,
        obstacles:  &[],
        resolution: 0, // use router's default
    };

    let geometry = router.solve(&request);
    assert_eq!(
        geometry.segments[0].points.len(),
        64,
        "should use the router's custom resolution when request.resolution is 0"
    );
}

// ─────────────────────────────────────────────────────────────────────
// Solvers: `CatenarySolver` trait implementations
// ─────────────────────────────────────────────────────────────────────

#[test]
fn catenary_solver_implements_curve_solver() {
    let solver = CatenarySolver::new().with_slack(1.2);
    let start = Vec3::new(-2.0, 3.0, 0.0);
    let end = Vec3::new(2.0, 3.0, 0.0);

    let segment = solver.solve_segment(start, end, 16);

    assert_eq!(segment.points.len(), 16);
    assert_vec3_approx(segment.points[0], start, "curve solver start");
    assert_vec3_approx(*segment.points.last().unwrap(), end, "curve solver end");
}

#[test]
fn catenary_solver_implements_route_solver() {
    let solver = CatenarySolver::new().with_slack(1.3);
    let request = RouteRequest {
        start:      Vec3::new(-3.0, 5.0, 0.0),
        end:        Vec3::new(3.0, 5.0, 0.0),
        obstacles:  &[],
        resolution: 24,
    };

    let geometry = solver.solve(&request);

    assert_eq!(geometry.segments.len(), 1);
    assert_eq!(geometry.waypoints.len(), 2);
    assert_eq!(geometry.segments[0].points.len(), 24);
    assert!(
        geometry.total_length > 0.0,
        "total length should be positive"
    );
}

#[test]
fn catenary_solver_custom_gravity() {
    // Gravity pointing in +Z instead of -Y
    let solver = CatenarySolver::new()
        .with_slack(1.3)
        .with_gravity(Vec3::new(0.0, 0.0, -9.81));

    let start = Vec3::new(-3.0, 0.0, 5.0);
    let end = Vec3::new(3.0, 0.0, 5.0);

    let segment = solver.solve_segment(start, end, 32);

    // Cable should sag in -Z direction
    let mid_idx = segment.points.len() / 2;
    let mid_z = segment.points[mid_idx].z;
    assert!(
        mid_z < 5.0,
        "midpoint z ({mid_z}) should sag below chord z (5.0) with -Z gravity"
    );
}

// ─────────────────────────────────────────────────────────────────────
// A* Pathfinding
// ─────────────────────────────────────────────────────────────────────

#[test]
fn astar_routes_around_single_obstacle() {
    let planner = AStarPlanner::new().with_grid_size(0.5).with_margin(0.2);

    let start = Vec3::new(0.0, 0.0, 0.0);
    let end = Vec3::new(6.0, 0.0, 0.0);

    // Place an obstacle directly in the path
    let obstacles = vec![Obstacle::new(
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(3.0, 0.0, 0.0),
    )];

    let waypoints = planner.plan(start, end, &obstacles);

    // Should have more than 2 waypoints (routed around obstacle)
    assert!(
        waypoints.len() > 2,
        "should route around obstacle, got {} waypoints",
        waypoints.len()
    );

    // First and last waypoints should match start and end
    assert_vec3_approx(waypoints[0], start, "A* path start");
    assert_vec3_approx(*waypoints.last().unwrap(), end, "A* path end");
}

#[test]
fn astar_returns_direct_path_when_no_obstacles() {
    let planner = AStarPlanner::new();
    let start = Vec3::new(0.0, 0.0, 0.0);
    let end = Vec3::new(5.0, 0.0, 0.0);

    let waypoints = planner.plan(start, end, &[]);

    assert_eq!(
        waypoints.len(),
        2,
        "should return direct path with no obstacles"
    );
    assert_vec3_approx(waypoints[0], start, "direct path start");
    assert_vec3_approx(waypoints[1], end, "direct path end");
}

#[test]
fn astar_returns_direct_path_when_obstacle_does_not_block() {
    let planner = AStarPlanner::new().with_grid_size(0.5);
    let start = Vec3::new(0.0, 0.0, 0.0);
    let end = Vec3::new(5.0, 0.0, 0.0);

    // Obstacle far off to the side, not blocking the direct path
    let obstacles = vec![Obstacle::new(Vec3::splat(0.5), Vec3::new(2.5, 5.0, 5.0))];

    let waypoints = planner.plan(start, end, &obstacles);

    assert_eq!(
        waypoints.len(),
        2,
        "should return direct path when obstacle is not blocking"
    );
}

#[test]
fn astar_falls_back_to_direct_when_path_not_found() {
    // Use a very restrictive max_cells so A* gives up quickly
    let planner = AStarPlanner {
        grid_size: 0.1,
        margin:    0.2,
        max_cells: 5, // extremely limited search budget
    };

    let start = Vec3::new(0.0, 0.0, 0.0);
    let end = Vec3::new(100.0, 0.0, 0.0);

    // Large obstacle spanning a wide area
    let obstacles = vec![Obstacle::new(
        Vec3::new(50.0, 50.0, 50.0),
        Vec3::new(50.0, 0.0, 0.0),
    )];

    let waypoints = planner.plan(start, end, &obstacles);

    // Should fall back to direct [start, end] since A* can't find a path
    assert_eq!(
        waypoints.len(),
        2,
        "should fall back to direct path when A* exhausts search budget"
    );
    assert_vec3_approx(waypoints[0], start, "fallback start");
    assert_vec3_approx(waypoints[1], end, "fallback end");
}

// ─────────────────────────────────────────────────────────────────────
// Orthogonal routing
// ─────────────────────────────────────────────────────────────────────

#[test]
fn orthogonal_produces_axis_aligned_waypoints() {
    let planner = OrthogonalPlanner::new();
    let start = Vec3::new(0.0, 0.0, 0.0);
    let end = Vec3::new(5.0, 3.0, 0.0);

    let waypoints = planner.plan(start, end, &[]);

    // Every segment between consecutive waypoints should be axis-aligned:
    // exactly two of the three coordinate deltas should be zero (or all three
    // for coincident points).
    for pair in waypoints.windows(2) {
        let delta = pair[1] - pair[0];
        let non_zero_axes = [delta.x, delta.y, delta.z]
            .iter()
            .filter(|v| v.abs() > TOLERANCE)
            .count();
        assert!(
            non_zero_axes <= 1,
            "segment from {} to {} is not axis-aligned (delta={delta})",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn orthogonal_produces_horizontal_vertical_separated_waypoints_3d() {
    let planner = OrthogonalPlanner::new();
    let start = Vec3::new(0.0, 0.0, 0.0);
    let end = Vec3::new(4.0, 3.0, 2.0);

    let waypoints = planner.plan(start, end, &[]);

    // The `OrthogonalPlanner` treats the XZ plane as "horizontal" and Y as "vertical".
    // Each segment should either change Y exclusively (vertical) or change XZ
    // exclusively (horizontal), but not both simultaneously.
    for pair in waypoints.windows(2) {
        let delta = pair[1] - pair[0];
        let moves_y = delta.y.abs() > TOLERANCE;
        let moves_xz = delta.x.abs() > TOLERANCE || delta.z.abs() > TOLERANCE;
        assert!(
            !(moves_y && moves_xz),
            "segment from {} to {} moves in both Y and XZ simultaneously (delta={delta})",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn orthogonal_vertical_first_starts_with_y_move() {
    let planner = OrthogonalPlanner::new().vertical_first();
    let start = Vec3::new(0.0, 0.0, 0.0);
    let end = Vec3::new(5.0, 3.0, 0.0);

    let waypoints = planner.plan(start, end, &[]);

    assert!(
        waypoints.len() >= 3,
        "vertical-first L-path should have at least 3 waypoints"
    );

    // First segment should move in Y (vertical)
    let first_delta = waypoints[1] - waypoints[0];
    assert!(
        first_delta.x.abs() < TOLERANCE && first_delta.z.abs() < TOLERANCE,
        "first segment should be vertical, but delta is {first_delta}"
    );
    assert!(
        first_delta.y.abs() > TOLERANCE,
        "first segment should have non-zero Y movement"
    );
}

#[test]
fn orthogonal_routes_around_obstacle() {
    let planner = OrthogonalPlanner::new().with_margin(0.3);
    let start = Vec3::new(0.0, 0.0, 0.0);
    let end = Vec3::new(6.0, 0.0, 0.0);

    // Place obstacle blocking the direct L-path bend point
    let obstacles = vec![Obstacle::new(
        Vec3::new(2.0, 2.0, 2.0),
        Vec3::new(3.0, 0.0, 0.0),
    )];

    let waypoints = planner.plan(start, end, &obstacles);

    // Should have found an alternate path (not just start->end)
    assert!(
        waypoints.len() >= 2,
        "should produce a valid path around the obstacle"
    );

    // All segments should still be axis-aligned
    for pair in waypoints.windows(2) {
        let delta = pair[1] - pair[0];
        let non_zero_axes = [delta.x, delta.y, delta.z]
            .iter()
            .filter(|v| v.abs() > TOLERANCE)
            .count();
        assert!(
            non_zero_axes <= 1,
            "routed segment from {} to {} is not axis-aligned (delta={delta})",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn orthogonal_endpoints_preserved() {
    let planner = OrthogonalPlanner::new();
    let start = Vec3::new(1.0, 2.0, 3.0);
    let end = Vec3::new(7.0, 5.0, 1.0);

    let waypoints = planner.plan(start, end, &[]);

    assert_vec3_approx(waypoints[0], start, "orthogonal path start");
    assert_vec3_approx(*waypoints.last().unwrap(), end, "orthogonal path end");
}

// ─────────────────────────────────────────────────────────────────────
// `CableGeometry` construction
// ─────────────────────────────────────────────────────────────────────

#[test]
fn cable_geometry_all_points_iterates_across_segments() {
    let solver = LinearSolver;

    // Build geometry with multiple segments via an orthogonal planner + linear solver
    let router = Router::new(OrthogonalPlanner::new(), solver);
    let request = RouteRequest {
        start:      Vec3::new(0.0, 0.0, 0.0),
        end:        Vec3::new(5.0, 3.0, 0.0),
        obstacles:  &[],
        resolution: 10,
    };

    let geometry = router.solve(&request);

    let total_points: usize = geometry.segments.iter().map(|s| s.points.len()).sum();
    let all_points_count = geometry.all_points().count();

    assert_eq!(
        all_points_count, total_points,
        "`all_points()` should iterate over every point in every segment"
    );
}

#[test]
fn cable_geometry_total_length_is_sum_of_segments() {
    let solver = LinearSolver;
    let router = Router::new(OrthogonalPlanner::new(), solver);
    let request = RouteRequest {
        start:      Vec3::new(0.0, 0.0, 0.0),
        end:        Vec3::new(4.0, 3.0, 0.0),
        obstacles:  &[],
        resolution: 10,
    };

    let geometry = router.solve(&request);

    let sum: f32 = geometry.segments.iter().map(|s| s.length).sum();
    assert!(
        (geometry.total_length - sum).abs() < TOLERANCE,
        "total_length ({}) should equal sum of segment lengths ({sum})",
        geometry.total_length
    );
}

// ─────────────────────────────────────────────────────────────────────
// `Obstacle` and `Anchor` construction
// ─────────────────────────────────────────────────────────────────────

#[test]
fn anchor_constructors() {
    let a = Anchor::from(Vec3::new(1.0, 2.0, 3.0));
    assert_vec3_approx(a.position, Vec3::new(1.0, 2.0, 3.0), "anchor position");
    assert!(
        a.direction.is_none(),
        "default anchor should have no direction"
    );

    let b = Anchor::with_direction(Vec3::new(0.0, 0.0, 0.0), Vec3::Y);
    assert!(
        b.direction.is_some(),
        "directed anchor should have a direction"
    );
    assert_vec3_approx(b.direction.unwrap(), Vec3::Y, "anchor direction");
}

// ─────────────────────────────────────────────────────────────────────
// Integration: full pipeline A* + CatenarySolver
// ─────────────────────────────────────────────────────────────────────

#[test]
fn full_pipeline_astar_catenary() {
    let router = Router::new(
        AStarPlanner::new().with_grid_size(0.5),
        CatenarySolver::new().with_slack(1.2),
    );

    let obstacles = vec![Obstacle::new(
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(3.0, 0.0, 0.0),
    )];

    let request = RouteRequest {
        start:      Vec3::new(0.0, 0.0, 0.0),
        end:        Vec3::new(6.0, 0.0, 0.0),
        obstacles:  &obstacles,
        resolution: 16,
    };

    let geometry = router.solve(&request);

    assert!(
        !geometry.segments.is_empty(),
        "should produce at least one segment"
    );
    assert!(
        geometry.total_length > 0.0,
        "total length should be positive"
    );

    // All segments should have the requested resolution
    for (i, seg) in geometry.segments.iter().enumerate() {
        assert_eq!(seg.points.len(), 16, "segment {i} should have 16 points");
    }
}

#[test]
fn full_pipeline_orthogonal_linear() {
    let router = Router::new(OrthogonalPlanner::new(), LinearSolver);

    let request = RouteRequest {
        start:      Vec3::new(0.0, 0.0, 0.0),
        end:        Vec3::new(5.0, 3.0, 0.0),
        obstacles:  &[],
        resolution: 10,
    };

    let geometry = router.solve(&request);

    // `OrthogonalPlanner` produces an L-path with 3 waypoints => 2 segments
    assert_eq!(
        geometry.segments.len(),
        2,
        "L-path should produce 2 segments"
    );

    // Each segment should have the requested resolution
    for seg in &geometry.segments {
        assert_eq!(seg.points.len(), 10);
    }
}
