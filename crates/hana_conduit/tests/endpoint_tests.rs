#![allow(
    clippy::unwrap_used,
    reason = "tests should panic on unexpected values"
)]
//! Integration tests for entity-based cable endpoints.
//!
//! Uses `MinimalPlugins` for headless testing: no window, no renderer.

use bevy::app::App;
use bevy::asset::AssetPlugin;
use bevy::gizmos::GizmoPlugin;
use bevy::math::Vec3;
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use bevy::transform::TransformPlugin;
use hana_conduit::AttachedTo;
use hana_conduit::Cable;
use hana_conduit::CableEnd;
use hana_conduit::CableEndpoint;
use hana_conduit::CatenaryPlugin;
use hana_conduit::CatenarySolver;
use hana_conduit::ComputedCableGeometry;
use hana_conduit::CurveKind;
use hana_conduit::DEFAULT_SLACK;
use hana_conduit::DetachPolicy;
use hana_conduit::Obstacle;
use hana_conduit::PathStrategy;
use hana_conduit::RouteAnimation;
use hana_conduit::RouteObstacle;
use hana_conduit::Solver;

/// Spawn a world-attached cable and return the cable entity.
fn spawn_world_cable(app: &mut App, start: Vec3, end: Vec3) -> Entity {
    let cable = app
        .world_mut()
        .spawn(Cable {
            solver:     Solver::Catenary(CatenarySolver::new().with_slack(DEFAULT_SLACK)),
            obstacles:  vec![],
            resolution: 0,
        })
        .id();

    app.world_mut()
        .spawn((CableEndpoint::new(CableEnd::Start, start), ChildOf(cable)));

    app.world_mut()
        .spawn((CableEndpoint::new(CableEnd::End, end), ChildOf(cable)));
    cable
}

fn build_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(MeshPlugin);
    app.add_plugins(TransformPlugin);
    app.add_plugins(GizmoPlugin);
    app.add_plugins(CatenaryPlugin);
    app
}

#[test]
fn world_attached_cable_computes_geometry() {
    let mut app = build_test_app();
    let cable = spawn_world_cable(
        &mut app,
        Vec3::new(-3.0, 2.0, 0.0),
        Vec3::new(3.0, 2.0, 0.0),
    );

    app.update();

    let computed_cable_geometry = app.world().get::<ComputedCableGeometry>(cable).unwrap();
    assert!(
        computed_cable_geometry.cable_geometry.is_some(),
        "Cable should have computed geometry after one update"
    );

    let cable_geometry = computed_cable_geometry.cable_geometry.as_ref().unwrap();
    assert!(
        !cable_geometry.segments.is_empty(),
        "Geometry should have at least one segment"
    );
    assert!(
        cable_geometry.total_length > 0.0,
        "Cable should have positive length"
    );
}

#[test]
fn entity_attached_cable_follows_target() {
    let mut app = build_test_app();

    // Spawn a target entity with a transform
    let target = app
        .world_mut()
        .spawn(Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)))
        .id();

    // Spawn cable with one entity-attached endpoint
    let cable = app
        .world_mut()
        .spawn(Cable {
            solver:     Solver::Catenary(CatenarySolver::new().with_slack(DEFAULT_SLACK)),
            obstacles:  vec![],
            resolution: 0,
        })
        .id();

    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::Start, Vec3::new(0.5, 0.0, 0.0)),
        AttachedTo(target),
        ChildOf(cable),
    ));
    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::End, Vec3::new(-3.0, 2.0, 0.0)),
        ChildOf(cable),
    ));

    // First update: compute initial geometry
    app.update();

    let computed_cable_geometry = app.world().get::<ComputedCableGeometry>(cable).unwrap();
    assert!(
        computed_cable_geometry.cable_geometry.is_some(),
        "Entity-attached cable should compute geometry"
    );

    // Move the target
    app.world_mut()
        .get_mut::<Transform>(target)
        .unwrap()
        .translation = Vec3::new(10.0, 0.0, 0.0);

    // Update to propagate transform and recompute
    app.update();
    // Second update may be needed for transform propagation
    app.update();

    let computed_cable_geometry = app.world().get::<ComputedCableGeometry>(cable).unwrap();
    let cable_geometry = computed_cable_geometry.cable_geometry.as_ref().unwrap();

    // The cable should have been recomputed with the new target position.
    // Start point should be near (10.0 + 0.5, 0.0, 0.0) = (10.5, 0, 0)
    let first_point = cable_geometry.segments[0].points[0];
    assert!(
        (first_point.x - 10.5).abs() < 1.0,
        "Start point should follow moved target, got {first_point}"
    );
}

#[test]
fn zero_length_cable_does_not_panic() {
    let mut app = build_test_app();

    let cable = app
        .world_mut()
        .spawn(Cable {
            solver:     Solver::Catenary(CatenarySolver::new().with_slack(DEFAULT_SLACK)),
            obstacles:  vec![],
            resolution: 0,
        })
        .id();

    // Both endpoints at the same position
    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::Start, Vec3::ZERO),
        ChildOf(cable),
    ));
    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::End, Vec3::ZERO),
        ChildOf(cable),
    ));

    // Should not panic
    app.update();

    let computed_cable_geometry = app.world().get::<ComputedCableGeometry>(cable).unwrap();
    assert!(
        computed_cable_geometry.cable_geometry.is_none(),
        "Zero-length cable should skip computation"
    );
}

#[test]
fn missing_target_does_not_panic() {
    let mut app = build_test_app();

    // Spawn a real target, then despawn it before the cable computes
    let target = app
        .world_mut()
        .spawn(Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)))
        .id();

    let cable = app
        .world_mut()
        .spawn(Cable {
            solver:     Solver::Catenary(CatenarySolver::new().with_slack(DEFAULT_SLACK)),
            obstacles:  vec![],
            resolution: 0,
        })
        .id();

    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::Start, Vec3::new(1.0, 0.0, 0.0)),
        AttachedTo(target),
        ChildOf(cable),
    ));
    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::End, Vec3::new(-1.0, 0.0, 0.0)),
        ChildOf(cable),
    ));

    // Despawn the target before the first update
    app.world_mut().despawn(target);

    // Should not panic — falls back to raw offset
    app.update();

    let computed_cable_geometry = app.world().get::<ComputedCableGeometry>(cable).unwrap();
    assert!(
        computed_cable_geometry.cable_geometry.is_some(),
        "Cable with despawned target should still compute (using fallback offset)"
    );
}

#[test]
fn detach_policy_despawn_removes_cable() {
    let mut app = build_test_app();

    let target = app
        .world_mut()
        .spawn(Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)))
        .id();

    let cable = app
        .world_mut()
        .spawn(Cable {
            solver:     Solver::Catenary(CatenarySolver::new().with_slack(DEFAULT_SLACK)),
            obstacles:  vec![],
            resolution: 0,
        })
        .id();

    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::Start, Vec3::new(0.5, 0.0, 0.0))
            .with_detach_policy(DetachPolicy::Despawn),
        AttachedTo(target),
        ChildOf(cable),
    ));
    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::End, Vec3::new(-3.0, 2.0, 0.0)),
        ChildOf(cable),
    ));

    // Initial update
    app.update();
    assert!(
        app.world().get_entity(cable).is_ok(),
        "Cable should exist before target despawn"
    );

    // Despawn the target
    app.world_mut().despawn(target);

    // Update to trigger the OnRemove<AttachedTo> observer
    app.update();

    assert!(
        app.world().get_entity(cable).is_err(),
        "Cable should be despawned after target despawn with DetachPolicy::Despawn"
    );
}

#[test]
fn detach_policy_remain_keeps_cable() {
    let mut app = build_test_app();

    let target = app
        .world_mut()
        .spawn(Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)))
        .id();

    let cable = app
        .world_mut()
        .spawn(Cable {
            solver:     Solver::Catenary(CatenarySolver::new().with_slack(DEFAULT_SLACK)),
            obstacles:  vec![],
            resolution: 0,
        })
        .id();

    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::Start, Vec3::new(0.5, 0.0, 0.0)),
        AttachedTo(target),
        ChildOf(cable),
    ));
    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::End, Vec3::new(-3.0, 2.0, 0.0)),
        ChildOf(cable),
    ));

    // Initial update
    app.update();

    // Despawn the target
    app.world_mut().despawn(target);
    app.update();

    // Cable should still exist (Remain is the default)
    assert!(
        app.world().get_entity(cable).is_ok(),
        "Cable should survive target despawn with DetachPolicy::Remain"
    );

    let computed_cable_geometry = app.world().get::<ComputedCableGeometry>(cable).unwrap();
    assert!(
        computed_cable_geometry.cable_geometry.is_some(),
        "Cable should retain its last computed geometry"
    );
}

#[test]
fn detach_policy_remain_preserves_world_position() {
    let mut app = build_test_app();

    let target = app
        .world_mut()
        .spawn(Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)))
        .id();

    let cable = app
        .world_mut()
        .spawn(Cable {
            solver:     Solver::Catenary(CatenarySolver::new().with_slack(DEFAULT_SLACK)),
            obstacles:  vec![],
            resolution: 0,
        })
        .id();

    let detached_endpoint = app
        .world_mut()
        .spawn((
            CableEndpoint::new(CableEnd::Start, Vec3::new(0.5, 0.0, 0.0)),
            AttachedTo(target),
            ChildOf(cable),
        ))
        .id();
    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::End, Vec3::new(-3.0, 2.0, 0.0)),
        ChildOf(cable),
    ));

    app.update();

    app.world_mut().despawn(target);
    app.update();

    let endpoint = app.world().get::<CableEndpoint>(detached_endpoint).unwrap();
    assert_eq!(
        endpoint.offset,
        Vec3::new(5.5, 0.0, 0.0),
        "Remain should convert the local offset to the last resolved world position"
    );
}

#[test]
fn catenary_detach_slack_bump_increases_slack_on_detach() {
    let mut app = build_test_app();

    let target = app
        .world_mut()
        .spawn(Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)))
        .id();

    let cable = app
        .world_mut()
        .spawn(Cable {
            solver:     Solver::Catenary(
                CatenarySolver::new()
                    .with_slack(DEFAULT_SLACK)
                    .with_detach_slack_bump(0.35),
            ),
            obstacles:  vec![],
            resolution: 0,
        })
        .id();

    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::Start, Vec3::new(0.5, 0.0, 0.0)),
        AttachedTo(target),
        ChildOf(cable),
    ));
    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::End, Vec3::new(-3.0, 2.0, 0.0)),
        ChildOf(cable),
    ));

    app.update();

    app.world_mut().despawn(target);
    app.update();
    app.update();

    let cable = app.world().get::<Cable>(cable).unwrap();
    assert!(
        matches!(&cable.solver, Solver::Catenary(_)),
        "test setup should keep the cable on a catenary solver, got {:?}",
        cable.solver
    );
    if let Solver::Catenary(catenary_solver) = &cable.solver {
        assert!(
            (catenary_solver.slack - (DEFAULT_SLACK + 0.35)).abs() < f32::EPSILON,
            "detach_slack_bump should add its value to slack once on detach (got {})",
            catenary_solver.slack
        );
    }
}

#[test]
fn route_obstacle_entity_diverts_routed_cable() {
    let mut app = build_test_app();

    // A box straddling the straight line between the endpoints.
    let obstacle = app
        .world_mut()
        .spawn((
            RouteObstacle::HalfExtents(Vec3::splat(0.5)),
            Transform::default(),
        ))
        .id();

    let cable = app
        .world_mut()
        .spawn(Cable {
            solver:     Solver::Routed {
                path_strategy: PathStrategy::AStar {
                    grid_size: 0.25,
                    margin:    0.1,
                },
                curve_kind:    CurveKind::Linear,
                resolution:    0,
            },
            obstacles:  vec![],
            resolution: 0,
        })
        .id();
    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::Start, Vec3::new(-2.0, 0.0, 0.0)),
        ChildOf(cable),
    ));
    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::End, Vec3::new(2.0, 0.0, 0.0)),
        ChildOf(cable),
    ));

    app.update();

    let geometry = routed_waypoints(&mut app, cable);
    assert!(
        geometry.len() > 2,
        "route should divert around the RouteObstacle entity, got {} waypoints",
        geometry.len()
    );

    // Moving the obstacle out of the way re-queues the cable via
    // `queue_obstacle_changes` and the route straightens.
    app.world_mut()
        .entity_mut(obstacle)
        .insert(Transform::from_translation(Vec3::new(0.0, 50.0, 0.0)));
    app.update();
    app.update();

    let geometry = routed_waypoints(&mut app, cable);
    assert_eq!(
        geometry.len(),
        2,
        "route should straighten after the RouteObstacle moves away"
    );
}

#[test]
fn route_animation_sweeps_toward_reroute_instead_of_jumping() {
    let mut app = build_test_app();
    let cable = app
        .world_mut()
        .spawn((
            Cable {
                solver:     Solver::Routed {
                    path_strategy: PathStrategy::AStar {
                        grid_size: 0.5,
                        margin:    0.2,
                    },
                    curve_kind:    CurveKind::Linear,
                    resolution:    0,
                },
                obstacles:  vec![],
                resolution: 0,
            },
            RouteAnimation::default(),
        ))
        .id();
    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::Start, Vec3::new(-3.0, 0.0, 0.0)),
        ChildOf(cable),
    ));
    app.world_mut().spawn((
        CableEndpoint::new(CableEnd::End, Vec3::new(3.0, 0.0, 0.0)),
        ChildOf(cable),
    ));

    app.update();
    assert_eq!(
        routed_waypoints(&mut app, cable).len(),
        2,
        "first solve shows immediately: an unobstructed route is a straight line"
    );

    // Blocking the straight line re-solves to a detour with bends. An
    // in-flight animated route reports only its two pinned endpoints as
    // waypoints, so the waypoint count distinguishes transit from convergence.
    app.world_mut()
        .get_mut::<Cable>(cable)
        .unwrap()
        .obstacles
        .push(Obstacle::new(Vec3::ONE, Vec3::ZERO));
    app.update();
    assert_eq!(
        routed_waypoints(&mut app, cable).len(),
        2,
        "displayed route must ease toward the detour, not jump to it"
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while routed_waypoints(&mut app, cable).len() == 2 {
        assert!(
            std::time::Instant::now() < deadline,
            "animation never converged to the detour route"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
        app.update();
    }
    assert!(
        routed_waypoints(&mut app, cable).len() > 2,
        "converged route keeps the detour's bends"
    );
}

#[test]
fn route_animation_snaps_while_endpoint_drags() {
    let mut app = build_test_app();
    let cable = app
        .world_mut()
        .spawn((
            Cable {
                solver:     Solver::Routed {
                    path_strategy: PathStrategy::AStar {
                        grid_size: 0.5,
                        margin:    0.2,
                    },
                    curve_kind:    CurveKind::Linear,
                    resolution:    0,
                },
                obstacles:  vec![],
                resolution: 0,
            },
            RouteAnimation::default(),
        ))
        .id();
    let start = Vec3::new(-3.0, 0.0, 0.0);
    app.world_mut()
        .spawn((CableEndpoint::new(CableEnd::Start, start), ChildOf(cable)));
    let end = app
        .world_mut()
        .spawn((
            CableEndpoint::new(CableEnd::End, Vec3::new(3.0, 0.0, 0.0)),
            ChildOf(cable),
        ))
        .id();

    app.update();

    // Dragging the free end re-solves with a moved anchor: the displayed
    // route must be the fresh solve — a straight line to the new position —
    // not a blend lagging along the old line.
    let moved = Vec3::new(3.0, 2.0, 0.0);
    app.world_mut()
        .get_mut::<CableEndpoint>(end)
        .unwrap()
        .offset = moved;
    app.update();

    let geometry = app
        .world()
        .get::<ComputedCableGeometry>(cable)
        .unwrap()
        .cable_geometry
        .clone()
        .unwrap();
    let direction = (moved - start).normalize();
    let length = (moved - start).length();
    for &point in geometry.all_points() {
        let along = (point - start).dot(direction).clamp(0.0, length);
        let nearest = start + direction * along;
        assert!(
            point.distance(nearest) < 1e-3,
            "displayed route must snap to the fresh solve while the endpoint \
             drags; sample {point} strays from the straight line"
        );
    }
}

/// The routed waypoints of `cable`'s computed geometry.
fn routed_waypoints(app: &mut App, cable: Entity) -> Vec<Vec3> {
    app.world()
        .get::<ComputedCableGeometry>(cable)
        .unwrap()
        .cable_geometry
        .as_ref()
        .unwrap()
        .waypoints
        .clone()
}
