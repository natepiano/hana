//! Arrangement and net closure tests for `hana_valence`.

use std::time::Duration;

use bevy::app::App;
use bevy::app::PostUpdate;
use bevy::ecs::schedule::ApplyDeferred;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::Schedule;
use bevy::ecs::world::World;
use bevy::prelude::Component;
use bevy::prelude::Entity;
use bevy::prelude::GlobalTransform;
use bevy::prelude::Transform;
use bevy::prelude::Vec3;
use bevy::time::Time;
use bevy::time::Virtual;
use hana_valence::Accordion;
use hana_valence::AnchorId;
use hana_valence::AnchorPose;
use hana_valence::AnchorSystems;
use hana_valence::AnchoredTo;
use hana_valence::Edge;
use hana_valence::FoldAngles;
use hana_valence::FoldCommand;
use hana_valence::FoldCommandEvent;
use hana_valence::FoldDirection;
use hana_valence::FoldEndpoint;
use hana_valence::FoldMember;
use hana_valence::FoldPattern;
use hana_valence::FoldPlugin;
use hana_valence::FoldSequence;
use hana_valence::FoldStage;
use hana_valence::Hinge;
use hana_valence::Member;
use hana_valence::QuadTiling;
use hana_valence::ResolveDiagnostics;
use hana_valence::ResolvedAnchorGeometry;
use hana_valence::TilingRule;
use hana_valence::apply_member_placements;
use hana_valence::assign_member_indices;
use hana_valence::drive_arrangement_hinges;
use hana_valence::hinge_to_pose;
use hana_valence::resolve_anchors;

#[path = "../fixtures.rs"]
#[allow(
    dead_code,
    reason = "shared geometry fixtures; this test uses a subset"
)]
mod fixtures;

use fixtures::QUAD_BOTTOM_EDGE;
use fixtures::QUAD_LEFT_EDGE;
use fixtures::QUAD_RIGHT_EDGE;
use fixtures::QUAD_TOP_EDGE;

const ASSERT_EPSILON: f32 = 1e-4;
const BOX_FACE_SIDE: f32 = 1.0;
const BOX_FOLD_ANGLE: f32 = -core::f32::consts::FRAC_PI_2;
const BOX_HALF_SIDE: f32 = BOX_FACE_SIDE / 2.0;
const BOX_RESOLVE_FRAMES: usize = 2;
const FIVE_THIRDS: f32 = 5.0 / 3.0;
const FOLD_STEP_SECONDS: f32 = 1.0;
const FOUR_TRIANGLE_MEMBERS: usize = 4;
const GROUPED_MEMBER_COUNT: usize = 2;
const TRIANGLE_HEIGHT: f32 = 0.866_025_4;
const TRIANGLE_REST_FLIP: f32 = core::f32::consts::PI;
const TRIANGLE_SIDE: f32 = 1.0;
const TRIANGLE_TWO_THIRDS: f32 = 2.0 / 3.0;

#[derive(Component)]
struct TriangleTiling;

impl TilingRule for TriangleTiling {
    fn next_edge(&self, index: usize) -> (Edge, Edge) {
        let edge = fixtures::triangle_edge(index);
        (edge, edge)
    }

    fn edge_anchor(&self, edge: Edge) -> Option<AnchorId> { fixtures::triangle_edge_anchor(edge) }

    fn rest_delta(&self, _: usize) -> f32 { TRIANGLE_REST_FLIP }
}

#[derive(Component)]
struct BoxFoldTarget {
    angle: f32,
}

#[derive(Clone, Copy)]
struct BoxFaces {
    center: Entity,
    east:   Entity,
    lid:    Entity,
    north:  Entity,
    south:  Entity,
    west:   Entity,
}

#[test]
fn triangle_strip_uses_tiling_rule_for_alternating_seats() {
    let mut world = world_with_diagnostics();
    let root = spawn_triangle(&mut world, Transform::default());
    world.entity_mut(root).insert((
        Accordion {
            fold:    0.0,
            lean:    core::f32::consts::FRAC_PI_3,
            pattern: FoldPattern::Accordion,
        },
        TriangleTiling,
    ));
    let members = spawn_triangle_members(&mut world, root, FOUR_TRIANGLE_MEMBERS);

    run_arrangement_schedule::<TriangleTiling>(&mut world);

    let half_side = TRIANGLE_SIDE / 2.0;
    // Rest centroids of the four members seated as a straight strip running down
    // and to the right; a full fold along each shared edge stacks them onto one
    // triangle.
    let expected = [
        Vec3::new(0.0, -TRIANGLE_HEIGHT * TRIANGLE_TWO_THIRDS, 0.0),
        Vec3::new(half_side, -TRIANGLE_HEIGHT, 0.0),
        Vec3::new(half_side, -TRIANGLE_HEIGHT * FIVE_THIRDS, 0.0),
        Vec3::new(TRIANGLE_SIDE, -TRIANGLE_HEIGHT * 2.0, 0.0),
    ];
    for (entity, expected_translation) in members.into_iter().zip(expected) {
        assert_vec3_close(transform(&world, entity).translation, expected_translation);
        assert_close(
            world.get::<Hinge>(entity).map_or(0.0, |hinge| hinge.angle),
            TRIANGLE_REST_FLIP,
        );
    }
}

#[test]
fn box_net_folds_closed_after_fixed_frames() {
    let mut world = world_with_diagnostics();
    let faces = spawn_box_net(&mut world);

    for _ in 0..BOX_RESOLVE_FRAMES {
        run_box_schedule(&mut world);
    }

    assert_vec3_close(transform(&world, faces.center).translation, Vec3::ZERO);
    assert_vec3_close(
        transform(&world, faces.north).translation,
        Vec3::new(0.0, BOX_HALF_SIDE, BOX_HALF_SIDE),
    );
    assert_vec3_close(
        transform(&world, faces.south).translation,
        Vec3::new(0.0, -BOX_HALF_SIDE, BOX_HALF_SIDE),
    );
    assert_vec3_close(
        transform(&world, faces.east).translation,
        Vec3::new(BOX_HALF_SIDE, 0.0, BOX_HALF_SIDE),
    );
    assert_vec3_close(
        transform(&world, faces.west).translation,
        Vec3::new(-BOX_HALF_SIDE, 0.0, BOX_HALF_SIDE),
    );
    assert_vec3_close(
        transform(&world, faces.lid).translation,
        Vec3::new(0.0, 0.0, BOX_FACE_SIDE),
    );

    assert_anchor_close(&world, faces.lid, QUAD_TOP_EDGE, faces.north, QUAD_TOP_EDGE);
    assert_anchor_close(
        &world,
        faces.lid,
        QUAD_BOTTOM_EDGE,
        faces.south,
        QUAD_BOTTOM_EDGE,
    );
    assert_anchor_close(
        &world,
        faces.lid,
        QUAD_RIGHT_EDGE,
        faces.east,
        QUAD_RIGHT_EDGE,
    );
    assert_anchor_close(
        &world,
        faces.lid,
        QUAD_LEFT_EDGE,
        faces.west,
        QUAD_LEFT_EDGE,
    );
}

#[test]
fn grouped_fold_stage_reaches_arrangement_endpoints_in_the_actuation_frame() {
    let mut app = App::new();
    app.insert_resource(Time::<Virtual>::default())
        .insert_resource(ResolveDiagnostics::default())
        .add_plugins(FoldPlugin)
        .configure_sets(
            PostUpdate,
            (AnchorSystems::AnimatePose, AnchorSystems::Resolve).chain(),
        )
        .add_systems(
            PostUpdate,
            (
                assign_member_indices,
                ApplyDeferred,
                apply_member_placements::<QuadTiling>,
                ApplyDeferred,
                drive_arrangement_hinges::<QuadTiling>,
            )
                .chain()
                .before(AnchorSystems::AnimatePose),
        )
        .add_systems(PostUpdate, hinge_to_pose.in_set(AnchorSystems::AnimatePose))
        .add_systems(PostUpdate, resolve_anchors.in_set(AnchorSystems::Resolve));

    let root = spawn_box_face(app.world_mut(), Transform::default());
    app.world_mut()
        .entity_mut(root)
        .insert((Accordion::default(), QuadTiling));
    let sequence = app
        .world_mut()
        .spawn(FoldSequence::new(FOLD_STEP_SECONDS).with_initial(FoldEndpoint::Unfolded))
        .id();
    let members = [FoldStage(0), FoldStage(0), FoldStage(1)].map(|stage| {
        let member = spawn_box_face(app.world_mut(), Transform::default());
        app.world_mut().entity_mut(member).insert((
            Member { arrangement: root },
            FoldMember::new(sequence, stage),
            FoldAngles {
                unfolded: 0.0,
                folded:   BOX_FOLD_ANGLE,
            },
        ));
        member
    });
    app.update();

    app.world_mut().trigger(FoldCommandEvent::new(
        sequence,
        FoldCommand::Step(FoldDirection::Folding),
    ));
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .advance_by(Duration::from_secs_f32(FOLD_STEP_SECONDS));
    app.update();

    for member in members.into_iter().take(GROUPED_MEMBER_COUNT) {
        assert_anchor_close(
            app.world(),
            member,
            QUAD_TOP_EDGE,
            app.world()
                .get::<AnchoredTo>(member)
                .map_or(root, AnchoredTo::target),
            QUAD_BOTTOM_EDGE,
        );
        assert_close(
            app.world()
                .get::<Hinge>(member)
                .map_or(0.0, |hinge| hinge.angle),
            BOX_FOLD_ANGLE,
        );
    }
    assert_vec3_close(
        anchor_position(app.world(), members[0], QUAD_BOTTOM_EDGE),
        anchor_position(app.world(), members[1], QUAD_TOP_EDGE),
    );
    assert_vec3_close(
        anchor_position(app.world(), members[0], QUAD_BOTTOM_EDGE),
        Vec3::new(0.0, -BOX_HALF_SIDE, BOX_FACE_SIDE),
    );
    assert_vec3_close(
        anchor_position(app.world(), members[1], QUAD_BOTTOM_EDGE),
        Vec3::new(0.0, BOX_HALF_SIDE, BOX_FACE_SIDE),
    );
    assert_close(
        app.world()
            .get::<Hinge>(members[2])
            .map_or(BOX_FOLD_ANGLE, |hinge| hinge.angle),
        0.0,
    );
}

fn run_arrangement_schedule<R: Component + TilingRule>(world: &mut World) {
    let mut schedule = Schedule::default();
    schedule.add_systems(
        (
            assign_member_indices,
            ApplyDeferred,
            apply_member_placements::<R>,
            ApplyDeferred,
            drive_arrangement_hinges::<R>,
            hinge_to_pose,
            resolve_anchors,
        )
            .chain(),
    );
    schedule.run(world);
}

fn run_box_schedule(world: &mut World) {
    let mut schedule = Schedule::default();
    schedule.add_systems((drive_box_hinges, hinge_to_pose, resolve_anchors).chain());
    schedule.run(world);
}

fn drive_box_hinges(mut hinges: bevy::prelude::Query<(&BoxFoldTarget, &mut Hinge)>) {
    for (target, mut hinge) in &mut hinges {
        hinge.angle = target.angle;
    }
}

fn spawn_triangle_members(world: &mut World, arrangement: Entity, count: usize) -> Vec<Entity> {
    (0..count)
        .map(|_| {
            let entity = spawn_triangle(world, Transform::default());
            world.entity_mut(entity).insert(Member { arrangement });
            entity
        })
        .collect()
}

fn spawn_box_net(world: &mut World) -> BoxFaces {
    let center = spawn_box_face(world, Transform::default());
    let north = spawn_hinged_box_face(world, center, QUAD_BOTTOM_EDGE, QUAD_TOP_EDGE);
    let south = spawn_hinged_box_face(world, center, QUAD_TOP_EDGE, QUAD_BOTTOM_EDGE);
    let east = spawn_hinged_box_face(world, center, QUAD_LEFT_EDGE, QUAD_RIGHT_EDGE);
    let west = spawn_hinged_box_face(world, center, QUAD_RIGHT_EDGE, QUAD_LEFT_EDGE);
    let lid = spawn_hinged_box_face(world, north, QUAD_TOP_EDGE, QUAD_TOP_EDGE);
    BoxFaces {
        center,
        east,
        lid,
        north,
        south,
        west,
    }
}

fn spawn_hinged_box_face(
    world: &mut World,
    parent: Entity,
    source_edge: Edge,
    target_edge: Edge,
) -> Entity {
    let entity = spawn_box_face(world, Transform::default());
    let source_anchor = fixtures::quad_edge_anchor(source_edge);
    let target_anchor = fixtures::quad_edge_anchor(target_edge);
    assert!(
        source_anchor.is_some() && target_anchor.is_some(),
        "fixture edge constant must map to a quad anchor",
    );
    let (Some(source_anchor), Some(target_anchor)) = (source_anchor, target_anchor) else {
        return entity;
    };
    world.entity_mut(entity).insert((
        AnchoredTo::new(parent, source_anchor, target_anchor),
        AnchorPose::default(),
        BoxFoldTarget {
            angle: BOX_FOLD_ANGLE,
        },
        Hinge {
            edge:  source_edge,
            angle: 0.0,
        },
    ));
    entity
}

fn spawn_triangle(world: &mut World, transform: Transform) -> Entity {
    world
        .spawn((
            fixtures::triangle_geometry(),
            transform,
            GlobalTransform::from(transform),
        ))
        .id()
}

fn spawn_box_face(world: &mut World, transform: Transform) -> Entity {
    world
        .spawn((
            fixtures::quad_geometry(BOX_FACE_SIDE, BOX_FACE_SIDE),
            transform,
            GlobalTransform::from(transform),
        ))
        .id()
}

fn anchor_position(world: &World, entity: Entity, edge: Edge) -> Vec3 {
    let transform = transform(world, entity);
    let anchor = fixtures::quad_edge_anchor(edge);
    assert!(
        anchor.is_some(),
        "fixture edge constant must map to a quad anchor",
    );
    let local = anchor
        .and_then(|anchor| {
            world
                .get::<ResolvedAnchorGeometry>(entity)
                .and_then(|geometry| geometry.points.get(&anchor))
        })
        .map_or(Vec3::ZERO, |point| point.position);
    transform.transform_point(local)
}

fn transform(world: &World, entity: Entity) -> Transform {
    world.get::<Transform>(entity).copied().unwrap_or_default()
}

fn world_with_diagnostics() -> World {
    let mut world = World::new();
    world.insert_resource(ResolveDiagnostics::default());
    world
}

fn assert_anchor_close(
    world: &World,
    source: Entity,
    source_edge: Edge,
    target: Entity,
    target_edge: Edge,
) {
    assert_vec3_close(
        anchor_position(world, source, source_edge),
        anchor_position(world, target, target_edge),
    );
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= ASSERT_EPSILON,
        "actual {actual}, expected {expected}",
    );
}

fn assert_vec3_close(actual: Vec3, expected: Vec3) {
    assert!(
        actual.abs_diff_eq(expected, ASSERT_EPSILON),
        "actual {actual:?}, expected {expected:?}",
    );
}
