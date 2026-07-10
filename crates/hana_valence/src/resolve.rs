//! Anchor relationship resolver.

use std::hash::Hash;

use bevy_ecs::entity::Entities;
use bevy_ecs::entity::Entity;
use bevy_ecs::hierarchy::ChildOf;
#[cfg(test)]
use bevy_ecs::schedule::IntoScheduleConfigs;
#[cfg(test)]
use bevy_ecs::schedule::Schedule;
use bevy_ecs::system::Query;
use bevy_ecs::system::ResMut;
#[cfg(test)]
use bevy_ecs::world::World;
use bevy_math::Vec3;
use bevy_platform::collections::HashMap;
use bevy_transform::prelude::GlobalTransform;
use bevy_transform::prelude::Transform;

use crate::AnchorId;
use crate::AnchorPoint;
use crate::AnchorPose;
#[cfg(test)]
use crate::AnchorSystems;
use crate::AnchoredTo;
use crate::AttachmentResolveAction;
use crate::AttachmentResolveCandidate;
use crate::AttachmentResolveDiagnostics;
use crate::AttachmentResolveReasons;
use crate::ResolvedAnchorGeometry;
use crate::ResolvedAnchorOffset;
use crate::ResolvedAnchorWorld;
use crate::resolve_attachments;

#[cfg(test)]
#[path = "../fixtures.rs"]
#[allow(
    dead_code,
    reason = "shared geometry fixtures; the resolver tests use a subset"
)]
mod fixtures;

const ORTHONORMAL_EPSILON: f32 = 1e-4;
#[cfg(test)]
const TEST_QUAD_HEIGHT: f32 = 1.0;
#[cfg(test)]
const TEST_QUAD_WIDTH: f32 = 2.0;
const UNIFORM_SCALE_EPSILON: f32 = 1e-4;

/// Resolver diagnostics for [`resolve_anchors`].
pub type ResolveDiagnostics = AttachmentResolveDiagnostics<ResolveSkip>;

/// Reason an anchor relationship did not resolve this frame.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ResolveSkip {
    /// The source entity lacks [`ResolvedAnchorGeometry`].
    MissingSourceGeometry,
    /// The target entity lacks [`ResolvedAnchorGeometry`].
    MissingTargetGeometry,
    /// The source entity lacks either [`Transform`] or [`GlobalTransform`].
    MissingSourceTransform,
    /// The target entity lacks [`GlobalTransform`].
    MissingTargetTransform,
    /// The source or target geometry does not contain this anchor id.
    MissingAnchor(AnchorId),
    /// The target entity no longer exists.
    DespawnedTarget,
    /// The source has a transform parent with unsupported scale or shear.
    UnsupportedParentTransform,
    /// The source entity's current global scale is non-finite.
    NonFiniteScale,
    /// The source depends on a target that already skipped this frame.
    BlockedBySkippedDependency,
    /// The source participates in an anchor relationship cycle.
    Cycle,
    /// The source depends on a cycle in the anchor relationship graph.
    BlockedByCycle,
}

/// Resolves [`AnchoredTo`] relationships into local [`Transform`] values.
///
/// `resolve_anchors` is the only system in this crate that writes `Transform`
/// for entities carrying `AnchoredTo`. Drivers should write [`AnchorPose`] in
/// [`AnchorSystems::AnimatePose`](crate::AnchorSystems::AnimatePose), and
/// consumers should run this system in
/// [`AnchorSystems::Resolve`](crate::AnchorSystems::Resolve) before
/// transform propagation.
///
/// The system reads `GlobalTransform` values produced by the previous
/// propagation pass. It writes local transforms, so external same-frame reads
/// of anchored entities' `GlobalTransform` components are one frame stale until
/// the consumer runs `TransformSystems::Propagate`.
///
/// [`ResolvedAnchorWorld`] is recomputed every frame for entities carrying the
/// cache. Entities resolved in the current frame use the newly computed
/// `GlobalTransform` stored by `resolve_anchors`. Cache entries for entities
/// not resolved in the current frame use the `GlobalTransform` from the
/// previous propagation pass, matching every query read by the resolver because
/// [`AnchorSystems::Resolve`](crate::AnchorSystems::Resolve) runs before
/// `TransformSystems::Propagate`. The cache has the same freshness as the
/// resolve pass and is never change-detection-gated.
pub fn resolve_anchors(
    entities: &Entities,
    attachments: Query<(Entity, &AnchoredTo)>,
    geometry: Query<&ResolvedAnchorGeometry>,
    globals: Query<&GlobalTransform>,
    poses: Query<&AnchorPose>,
    offsets: Query<&ResolvedAnchorOffset>,
    parents: Query<&ChildOf>,
    mut transforms: Query<&mut Transform>,
    mut anchor_worlds: Query<(
        Entity,
        &ResolvedAnchorGeometry,
        Option<&GlobalTransform>,
        &mut ResolvedAnchorWorld,
    )>,
    mut diagnostics: ResMut<ResolveDiagnostics>,
) {
    let candidates = classify_candidates(entities, &attachments, &geometry, &globals, &transforms);
    let mut resolved_globals = HashMap::default();
    resolve_attachments(candidates, resolve_reasons(), &mut diagnostics, |action| {
        handle_action(
            &geometry,
            &globals,
            &poses,
            &offsets,
            &parents,
            &mut transforms,
            &mut resolved_globals,
            action,
        )
    });
    refresh_anchor_world_cache(&mut anchor_worlds, &resolved_globals);
}

fn classify_candidates(
    entities: &Entities,
    attachments: &Query<(Entity, &AnchoredTo)>,
    geometry: &Query<&ResolvedAnchorGeometry>,
    globals: &Query<&GlobalTransform>,
    transforms: &Query<&mut Transform>,
) -> Vec<AttachmentResolveCandidate<ResolveSkip>> {
    attachments
        .iter()
        .map(|(source, attachment)| (source, *attachment))
        .map(|(source, attachment)| {
            let target = attachment.target();
            match validate_candidate(entities, geometry, globals, transforms, source, attachment) {
                Ok(()) => AttachmentResolveCandidate::Active {
                    source,
                    target,
                    attachment,
                },
                Err(reason) => AttachmentResolveCandidate::Skipped {
                    source,
                    target,
                    reason,
                },
            }
        })
        .collect()
}

fn validate_candidate(
    entities: &Entities,
    geometry: &Query<&ResolvedAnchorGeometry>,
    globals: &Query<&GlobalTransform>,
    transforms: &Query<&mut Transform>,
    source: Entity,
    attachment: AnchoredTo,
) -> Result<(), ResolveSkip> {
    let target = attachment.target();
    if !entities.contains_spawned(target) {
        return Err(ResolveSkip::DespawnedTarget);
    }
    if !geometry.contains(source) {
        return Err(ResolveSkip::MissingSourceGeometry);
    }
    if !geometry.contains(target) {
        return Err(ResolveSkip::MissingTargetGeometry);
    }
    if !transforms.contains(source) || !globals.contains(source) {
        return Err(ResolveSkip::MissingSourceTransform);
    }
    if !globals.contains(target) {
        return Err(ResolveSkip::MissingTargetTransform);
    }
    Ok(())
}

const fn resolve_reasons() -> AttachmentResolveReasons<ResolveSkip> {
    AttachmentResolveReasons {
        blocked_by_skipped_dependency: ResolveSkip::BlockedBySkippedDependency,
        cycle:                         ResolveSkip::Cycle,
        blocked_by_cycle:              ResolveSkip::BlockedByCycle,
    }
}

fn handle_action(
    geometry: &Query<&ResolvedAnchorGeometry>,
    globals: &Query<&GlobalTransform>,
    poses: &Query<&AnchorPose>,
    offsets: &Query<&ResolvedAnchorOffset>,
    parents: &Query<&ChildOf>,
    transforms: &mut Query<&mut Transform>,
    resolved_globals: &mut HashMap<Entity, GlobalTransform>,
    action: AttachmentResolveAction,
) -> Result<(), ResolveSkip> {
    match action {
        AttachmentResolveAction::Place {
            source,
            target,
            attachment,
        } => place_anchor(
            geometry,
            globals,
            poses,
            offsets,
            parents,
            transforms,
            resolved_globals,
            source,
            target,
            attachment,
        ),
        AttachmentResolveAction::Fallback { source: _ } => Ok(()),
    }
}

fn place_anchor(
    geometry: &Query<&ResolvedAnchorGeometry>,
    globals: &Query<&GlobalTransform>,
    poses: &Query<&AnchorPose>,
    offsets: &Query<&ResolvedAnchorOffset>,
    parents: &Query<&ChildOf>,
    transforms: &mut Query<&mut Transform>,
    resolved_globals: &mut HashMap<Entity, GlobalTransform>,
    source: Entity,
    target: Entity,
    attachment: AnchoredTo,
) -> Result<(), ResolveSkip> {
    let placement = anchor_placement(
        geometry,
        globals,
        poses,
        offsets,
        parents,
        resolved_globals,
        source,
        target,
        attachment,
    )?;
    let Ok(mut transform) = transforms.get_mut(source) else {
        return Err(ResolveSkip::MissingSourceTransform);
    };
    *transform = placement.local_transform;
    resolved_globals.insert(source, placement.global_transform);
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AnchorPlacement {
    global_transform: GlobalTransform,
    local_transform:  Transform,
}

fn anchor_placement(
    geometry: &Query<&ResolvedAnchorGeometry>,
    globals: &Query<&GlobalTransform>,
    poses: &Query<&AnchorPose>,
    offsets: &Query<&ResolvedAnchorOffset>,
    parents: &Query<&ChildOf>,
    resolved_globals: &HashMap<Entity, GlobalTransform>,
    source: Entity,
    target: Entity,
    attachment: AnchoredTo,
) -> Result<AnchorPlacement, ResolveSkip> {
    let target_global = entity_global(globals, resolved_globals, target)
        .ok_or(ResolveSkip::MissingTargetTransform)?;
    let source_global = entity_global(globals, resolved_globals, source)
        .ok_or(ResolveSkip::MissingSourceTransform)?;
    let source_scale = source_global.to_scale_rotation_translation().0;
    if !source_scale.is_finite() {
        return Err(ResolveSkip::NonFiniteScale);
    }

    let target_point = anchor_point(geometry, target, attachment.target_anchor)?;
    let source_point = anchor_point(geometry, source, attachment.source_anchor)?;
    let pose = poses.get(source).copied().unwrap_or_default();
    let offset = offsets
        .get(source)
        .copied()
        .map_or(attachment.offset, |offset| offset.0);

    let target_world = target_global.transform_point(target_point.position);
    let base = target_global.rotation() * target_point.rotation();
    let rotation = base * pose.rotation * source_point.rotation().inverse();
    let translation = target_world + base * (offset + pose.translation)
        - rotation * (source_scale * source_point.position);
    let global_transform = GlobalTransform::from(Transform {
        translation,
        rotation,
        scale: source_scale,
    });
    let local_transform =
        local_transform_for(parents, globals, resolved_globals, source, global_transform)?;

    Ok(AnchorPlacement {
        global_transform,
        local_transform,
    })
}

fn entity_global(
    globals: &Query<&GlobalTransform>,
    resolved_globals: &HashMap<Entity, GlobalTransform>,
    entity: Entity,
) -> Option<GlobalTransform> {
    resolved_globals
        .get(&entity)
        .copied()
        .or_else(|| globals.get(entity).ok().copied())
}

fn anchor_point(
    geometry: &Query<&ResolvedAnchorGeometry>,
    entity: Entity,
    anchor_id: AnchorId,
) -> Result<AnchorPoint, ResolveSkip> {
    geometry
        .get(entity)
        .map_err(|_| ResolveSkip::MissingSourceGeometry)?
        .points
        .get(&anchor_id)
        .copied()
        .ok_or(ResolveSkip::MissingAnchor(anchor_id))
}

fn local_transform_for(
    parents: &Query<&ChildOf>,
    globals: &Query<&GlobalTransform>,
    resolved_globals: &HashMap<Entity, GlobalTransform>,
    source: Entity,
    global_transform: GlobalTransform,
) -> Result<Transform, ResolveSkip> {
    let Ok(parent) = parents.get(source) else {
        return Ok(global_transform.compute_transform());
    };
    let parent_global = entity_global(globals, resolved_globals, parent.parent())
        .ok_or(ResolveSkip::UnsupportedParentTransform)?;
    validate_supported_parent_transform(&parent_global)?;
    Ok(global_transform.reparented_to(&parent_global))
}

fn validate_supported_parent_transform(parent: &GlobalTransform) -> Result<(), ResolveSkip> {
    let affine = parent.affine();
    let x_axis = affine.transform_vector3(Vec3::X);
    let y_axis = affine.transform_vector3(Vec3::Y);
    let z_axis = affine.transform_vector3(Vec3::Z);
    let x_scale = x_axis.length();
    let y_scale = y_axis.length();
    let z_scale = z_axis.length();
    if !x_scale.is_finite()
        || !y_scale.is_finite()
        || !z_scale.is_finite()
        || x_scale <= ORTHONORMAL_EPSILON
        || y_scale <= ORTHONORMAL_EPSILON
        || z_scale <= ORTHONORMAL_EPSILON
    {
        return Err(ResolveSkip::UnsupportedParentTransform);
    }
    let average_scale = (x_scale + y_scale + z_scale) / 3.0;
    if (x_scale - average_scale).abs() > UNIFORM_SCALE_EPSILON
        || (y_scale - average_scale).abs() > UNIFORM_SCALE_EPSILON
        || (z_scale - average_scale).abs() > UNIFORM_SCALE_EPSILON
    {
        return Err(ResolveSkip::UnsupportedParentTransform);
    }

    let x_axis = x_axis / x_scale;
    let y_axis = y_axis / y_scale;
    let z_axis = z_axis / z_scale;
    if x_axis.dot(y_axis).abs() > ORTHONORMAL_EPSILON
        || x_axis.dot(z_axis).abs() > ORTHONORMAL_EPSILON
        || y_axis.dot(z_axis).abs() > ORTHONORMAL_EPSILON
        || x_axis.cross(y_axis).dot(z_axis) <= 0.0
    {
        return Err(ResolveSkip::UnsupportedParentTransform);
    }
    Ok(())
}

fn refresh_anchor_world_cache(
    anchor_worlds: &mut Query<(
        Entity,
        &ResolvedAnchorGeometry,
        Option<&GlobalTransform>,
        &mut ResolvedAnchorWorld,
    )>,
    resolved_globals: &HashMap<Entity, GlobalTransform>,
) {
    for (entity, geometry, global_transform, mut anchor_world) in anchor_worlds.iter_mut() {
        let Some(global_transform) = resolved_globals
            .get(&entity)
            .copied()
            .or_else(|| global_transform.copied())
        else {
            anchor_world.points.clear();
            continue;
        };
        anchor_world
            .points
            .retain(|anchor_id, _| geometry.points.contains_key(anchor_id));
        for (anchor_id, point) in &geometry.points {
            anchor_world
                .points
                .insert(*anchor_id, global_transform.transform_point(point.position));
        }
    }
}

/// Test helper that creates a world with resolver diagnostics installed.
#[cfg(test)]
pub(crate) fn world_with_diagnostics() -> World {
    let mut world = World::new();
    world.insert_resource(ResolveDiagnostics::default());
    world
}

/// Test helper that runs only [`resolve_anchors`].
#[cfg(test)]
pub(crate) fn run_resolve(world: &mut World) {
    let mut schedule = Schedule::default();
    schedule.add_systems(resolve_anchors.in_set(AnchorSystems::Resolve));
    schedule.run(world);
}

/// Test helper that spawns one flat quad with transform components.
#[cfg(test)]
pub(crate) fn spawn_quad(world: &mut World, transform: Transform) -> Entity {
    world
        .spawn((quad_geometry(), transform, GlobalTransform::from(transform)))
        .id()
}

/// Test helper that returns the canonical flat quad geometry.
#[cfg(test)]
pub(crate) fn quad_geometry() -> ResolvedAnchorGeometry {
    fixtures::quad_geometry(TEST_QUAD_WIDTH, TEST_QUAD_HEIGHT)
}

#[cfg(test)]
mod tests {
    use bevy_ecs::entity::Entity;
    use bevy_ecs::hierarchy::ChildOf;
    use bevy_ecs::prelude::Query;
    use bevy_ecs::schedule::IntoScheduleConfigs;
    use bevy_ecs::schedule::Schedule;
    use bevy_ecs::world::World;
    use bevy_math::Quat;
    use bevy_math::Vec3;
    use bevy_platform::collections::HashMap;
    use bevy_transform::prelude::GlobalTransform;
    use bevy_transform::prelude::Transform;

    use super::ResolveDiagnostics;
    use super::ResolveSkip;
    use super::quad_geometry;
    use super::resolve_anchors;
    use super::run_resolve;
    use super::spawn_quad;
    use super::world_with_diagnostics;
    use crate::AnchorId;
    use crate::AnchorPoint;
    use crate::AnchorPose;
    use crate::AnchorSystems;
    use crate::AnchoredTo;
    use crate::AttachmentResolveDiagnostics;
    use crate::ResolvedAnchorGeometry;
    use crate::ResolvedAnchorOffset;
    use crate::ResolvedAnchorWorld;

    const ASSERT_EPSILON: f32 = 1e-4;
    const CHAIN_LIFT: f32 = 1.0;
    const CHILD_SCALE: f32 = 0.5;
    const OFFSET_OVERRIDE_Y: f32 = 2.0;
    const OFFSET_TRACE_X: f32 = 0.25;
    const OFFSET_TRACE_Y: f32 = -0.5;
    const POSE_LIFT: f32 = 0.5;
    const QUAD_HEIGHT: f32 = 1.0;
    const QUAD_WIDTH: f32 = 2.0;
    const TARGET_ANCHOR_X: f32 = 2.0;
    const TARGET_ANCHOR_Y: f32 = 1.0;
    const TARGET_TRACE_X: f32 = 3.0;
    const TARGET_TRACE_Y: f32 = 3.0;

    #[test]
    fn two_quads_top_left_to_top_right() {
        let mut world = world_with_diagnostics();
        let target = spawn_quad(&mut world, Transform::default());
        let source = spawn_quad(&mut world, Transform::default());
        world.entity_mut(source).insert(AnchoredTo::new(
            target,
            AnchorId::Vertex(0),
            AnchorId::Vertex(1),
        ));

        run_resolve(&mut world);

        assert_anchor_matches(
            &world,
            source,
            AnchorId::Vertex(0),
            target,
            AnchorId::Vertex(1),
        );
    }

    #[test]
    fn offset_trace_applies_raw_offset_in_target_frame() {
        let mut world = world_with_diagnostics();
        let target = spawn_quad(
            &mut world,
            Transform::from_translation(Vec3::new(TARGET_TRACE_X, TARGET_TRACE_Y, 0.0)),
        );
        let source = spawn_quad(&mut world, Transform::default());
        world.entity_mut(source).insert((
            AnchoredTo::new(target, AnchorId::Center, AnchorId::Center).with_offset(Vec3::new(
                OFFSET_TRACE_X,
                OFFSET_TRACE_Y,
                0.0,
            )),
            ResolvedAnchorWorld::default(),
        ));

        run_resolve(&mut world);

        let expected = Vec3::new(
            TARGET_TRACE_X + OFFSET_TRACE_X,
            TARGET_TRACE_Y + OFFSET_TRACE_Y,
            0.0,
        );
        assert_close_vec3(
            world_anchor_point(&world, source, AnchorId::Center),
            expected,
        );
        assert_close_vec3(
            cached_anchor_point(&world, source, AnchorId::Center),
            expected,
        );
    }

    #[test]
    fn scale_and_parent_rotation_port_matches_world_anchor_expectation() {
        let mut world = world_with_diagnostics();
        let parent_transform =
            Transform::from_rotation(Quat::from_rotation_z(core::f32::consts::FRAC_PI_2));
        let parent = spawn_transform_only(&mut world, parent_transform);
        let target_transform = Transform::from_translation(Vec3::new(
            TARGET_ANCHOR_X + QUAD_WIDTH / 2.0,
            TARGET_ANCHOR_Y - QUAD_HEIGHT / 2.0,
            0.0,
        ));
        let target = spawn_quad(&mut world, target_transform);
        let source_transform = Transform::from_scale(Vec3::splat(CHILD_SCALE));
        let source_global = global_transform(&world, parent).mul_transform(source_transform);
        let source = world
            .spawn((
                quad_geometry(),
                source_transform,
                source_global,
                ChildOf(parent),
                AnchoredTo::new(target, AnchorId::Vertex(2), AnchorId::Vertex(0)),
            ))
            .id();

        run_resolve(&mut world);

        let actual = global_transform(&world, parent).mul_transform(transform(&world, source));
        let (scale, rotation, translation) = actual.to_scale_rotation_translation();
        assert_close_vec3(translation, Vec3::new(1.5, 1.25, 0.0));
        assert_close_quat(rotation, Quat::IDENTITY);
        assert_close_vec3(scale, Vec3::splat(CHILD_SCALE));
    }

    #[test]
    fn pose_written_in_animation_set_lands_this_frame() {
        let mut world = world_with_diagnostics();
        let target = spawn_quad(
            &mut world,
            Transform::from_translation(Vec3::new(TARGET_ANCHOR_X, TARGET_ANCHOR_Y, 0.0)),
        );
        let source = spawn_quad(&mut world, Transform::default());
        world.entity_mut(source).insert((
            AnchoredTo::new(target, AnchorId::Center, AnchorId::Center),
            AnchorPose::default(),
        ));
        let mut schedule = Schedule::default();
        schedule.configure_sets((AnchorSystems::AnimatePose, AnchorSystems::Resolve).chain());
        schedule.add_systems((
            lift_pose.in_set(AnchorSystems::AnimatePose),
            resolve_anchors.in_set(AnchorSystems::Resolve),
        ));

        schedule.run(&mut world);

        assert_close_vec3(
            world_anchor_point(&world, source, AnchorId::Center),
            world_anchor_point(&world, target, AnchorId::Center) + Vec3::Z * POSE_LIFT,
        );
    }

    #[test]
    fn frame_seating_preserves_pin_and_composes_source_frame_out() {
        let mut world = world_with_diagnostics();
        let target_frame = Quat::from_rotation_z(core::f32::consts::FRAC_PI_2);
        let source_frame = Quat::from_rotation_x(core::f32::consts::FRAC_PI_2);
        let pose_rotation = Quat::from_rotation_y(core::f32::consts::FRAC_PI_2);
        let target = spawn_geometry(
            &mut world,
            framed_geometry(AnchorId::Center, Vec3::ZERO, target_frame),
            Transform::default(),
        );
        let source = spawn_geometry(
            &mut world,
            framed_geometry(AnchorId::Vertex(0), Vec3::new(-1.0, 1.0, 0.0), source_frame),
            Transform::default(),
        );
        world.entity_mut(source).insert((
            AnchoredTo::new(target, AnchorId::Vertex(0), AnchorId::Center),
            AnchorPose {
                rotation:    pose_rotation,
                translation: Vec3::ZERO,
            },
        ));

        run_resolve(&mut world);

        let transform = transform(&world, source);
        assert_anchor_matches(
            &world,
            source,
            AnchorId::Vertex(0),
            target,
            AnchorId::Center,
        );
        assert_close_quat(
            transform.rotation * source_frame,
            target_frame * pose_rotation,
        );
    }

    #[test]
    fn wide_and_deep_tree_resolves_in_topological_order() {
        let mut world = world_with_diagnostics();
        let root = spawn_quad(&mut world, Transform::default());
        let first = spawn_center_anchor(&mut world, root, Vec3::X);
        let second = spawn_center_anchor(&mut world, first, Vec3::Y * CHAIN_LIFT);
        let third = spawn_center_anchor(&mut world, second, Vec3::Y * CHAIN_LIFT);
        let fourth = spawn_center_anchor(&mut world, third, Vec3::Y * CHAIN_LIFT);
        let fanout_offsets = [
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(-2.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            Vec3::new(0.0, -2.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(2.0, -1.0, 0.0),
            Vec3::new(1.0, -2.0, 0.0),
        ];
        let fanout = fanout_offsets
            .into_iter()
            .map(|offset| spawn_center_anchor(&mut world, root, offset))
            .collect::<Vec<_>>();

        run_resolve(&mut world);

        assert_close_vec3(
            world_anchor_point(&world, fourth, AnchorId::Center),
            Vec3::new(1.0, 3.0, 0.0),
        );
        assert_close_vec3(
            world_anchor_point(&world, fanout[fanout.len() - 1], AnchorId::Center),
            Vec3::new(1.0, -2.0, 0.0),
        );
        assert!(diagnostics(&world).is_empty());
    }

    #[test]
    fn resolved_anchor_offset_override_beats_anchored_to_offset() {
        let mut world = world_with_diagnostics();
        let target = spawn_quad(&mut world, Transform::default());
        let source = spawn_quad(&mut world, Transform::default());
        world.entity_mut(source).insert((
            AnchoredTo::new(target, AnchorId::Center, AnchorId::Center)
                .with_offset(Vec3::new(10.0, 0.0, 0.0)),
            ResolvedAnchorOffset(Vec3::new(0.0, OFFSET_OVERRIDE_Y, 0.0)),
        ));

        run_resolve(&mut world);

        assert_close_vec3(
            world_anchor_point(&world, source, AnchorId::Center),
            Vec3::new(0.0, OFFSET_OVERRIDE_Y, 0.0),
        );
    }

    #[test]
    fn non_uniform_transform_parent_records_skip() {
        let mut world = world_with_diagnostics();
        let parent =
            spawn_transform_only(&mut world, Transform::from_scale(Vec3::new(2.0, 1.0, 1.0)));
        let target = spawn_quad(&mut world, Transform::default());
        let source = world
            .spawn((
                quad_geometry(),
                Transform::default(),
                global_transform(&world, parent),
                ChildOf(parent),
                AnchoredTo::new(target, AnchorId::Center, AnchorId::Center),
            ))
            .id();

        run_resolve(&mut world);

        assert_eq!(transform(&world, source), Transform::default());
        assert_current_diagnostic(
            &world,
            source,
            target,
            ResolveSkip::UnsupportedParentTransform,
        );
    }

    fn lift_pose(mut poses: Query<&mut AnchorPose>) {
        for mut pose in &mut poses {
            pose.translation.z = POSE_LIFT;
        }
    }

    fn spawn_transform_only(world: &mut World, transform: Transform) -> Entity {
        world
            .spawn((transform, GlobalTransform::from(transform)))
            .id()
    }

    fn spawn_geometry(
        world: &mut World,
        geometry: ResolvedAnchorGeometry,
        transform: Transform,
    ) -> Entity {
        world
            .spawn((geometry, transform, GlobalTransform::from(transform)))
            .id()
    }

    fn spawn_center_anchor(world: &mut World, target: Entity, offset: Vec3) -> Entity {
        let source = spawn_quad(world, Transform::default());
        world.entity_mut(source).insert(
            AnchoredTo::new(target, AnchorId::Center, AnchorId::Center).with_offset(offset),
        );
        source
    }

    fn framed_geometry(anchor_id: AnchorId, position: Vec3, frame: Quat) -> ResolvedAnchorGeometry {
        ResolvedAnchorGeometry {
            points: HashMap::from_iter([(
                anchor_id,
                AnchorPoint {
                    position,
                    frame: Some(frame),
                },
            )]),
            edges:  Vec::new(),
        }
    }

    fn assert_anchor_matches(
        world: &World,
        source: Entity,
        source_anchor: AnchorId,
        target: Entity,
        target_anchor: AnchorId,
    ) {
        assert_close_vec3(
            world_anchor_point(world, source, source_anchor),
            world_anchor_point(world, target, target_anchor),
        );
    }

    fn assert_current_diagnostic(
        world: &World,
        source: Entity,
        target: Entity,
        reason: ResolveSkip,
    ) {
        assert!(diagnostics(world).current().any(|entry| {
            entry.source == source && entry.target == target && entry.reason == reason
        }));
    }

    fn diagnostics(world: &World) -> &AttachmentResolveDiagnostics<ResolveSkip> {
        world.resource::<ResolveDiagnostics>()
    }

    fn world_anchor_point(world: &World, entity: Entity, anchor_id: AnchorId) -> Vec3 {
        let transform = transform(world, entity);
        let point = geometry(world, entity)
            .points
            .get(&anchor_id)
            .copied()
            .unwrap_or_default();
        transform.translation + transform.rotation * (transform.scale * point.position)
    }

    fn cached_anchor_point(world: &World, entity: Entity, anchor_id: AnchorId) -> Vec3 {
        world
            .get::<ResolvedAnchorWorld>(entity)
            .and_then(|anchor_world| anchor_world.points.get(&anchor_id).copied())
            .unwrap_or(Vec3::NAN)
    }

    fn transform(world: &World, entity: Entity) -> Transform {
        world.get::<Transform>(entity).copied().unwrap_or_default()
    }

    fn global_transform(world: &World, entity: Entity) -> GlobalTransform {
        world
            .get::<GlobalTransform>(entity)
            .copied()
            .unwrap_or_default()
    }

    fn geometry(world: &World, entity: Entity) -> ResolvedAnchorGeometry {
        let Some(geometry) = world.get::<ResolvedAnchorGeometry>(entity) else {
            return ResolvedAnchorGeometry {
                points: HashMap::default(),
                edges:  Vec::new(),
            };
        };
        ResolvedAnchorGeometry {
            points: geometry.points.clone(),
            edges:  geometry.edges.clone(),
        }
    }

    fn assert_close_vec3(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).length() <= ASSERT_EPSILON,
            "actual {actual:?}, expected {expected:?}",
        );
    }

    fn assert_close_quat(actual: Quat, expected: Quat) {
        assert!(
            actual.dot(expected).abs() >= 1.0 - ASSERT_EPSILON,
            "actual {actual:?}, expected {expected:?}",
        );
    }
}
