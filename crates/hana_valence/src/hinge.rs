//! Edge-fold animation driver.

use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::change_detection::Mut;
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Component;
use bevy_ecs::prelude::ReflectComponent;
use bevy_ecs::system::Query;
use bevy_ecs::system::SystemChangeTick;
use bevy_math::Dir3;
use bevy_math::Quat;
use bevy_math::Vec3;
use bevy_reflect::Reflect;

use crate::AnchorId;
use crate::AnchorPose;
use crate::AnchoredTo;
use crate::Edge;
use crate::EdgeAxisError;
use crate::ResolvedAnchorGeometry;

/// Per-relation fold angle around an authored edge.
///
/// `Hinge` is a driver for [`AnchorPose`]. [`hinge_to_pose`] overwrites
/// `AnchorPose` every frame for entities carrying both components; remove
/// `Hinge` from an entity when another system should drive `AnchorPose`
/// directly.
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Hinge {
    /// Child-local endpoints whose direction is `end - start`.
    pub edge:  Edge,
    /// Fold angle in radians. Swapping [`Edge::start`] and [`Edge::end`] flips
    /// the fold direction for the same value.
    pub angle: f32,
}

impl Hinge {
    /// Returns the fold rotation about [`Hinge::edge`].
    ///
    /// # Errors
    ///
    /// Returns [`EdgeAxisError`] from [`Edge::axis`] when the edge cannot provide
    /// a usable rotation axis.
    pub fn rotation(&self, geometry: &ResolvedAnchorGeometry) -> Result<Quat, EdgeAxisError> {
        Ok(self.rotation_about(self.edge.axis(geometry)?))
    }

    fn rotation_about(&self, axis: Dir3) -> Quat { Quat::from_axis_angle(*axis, self.angle) }
}

/// External pivot line for a [`Hinge`].
///
/// `HingePivot` is optional. When present, [`hinge_to_pose`] converts the
/// hinge edge into the [`AnchoredTo::source_anchor`] tangent frame and writes
/// the translation needed to keep the pivot line fixed during rotation.
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component)]
pub struct HingePivot {
    /// Pivot-line offset from the source anchor, expressed in the source
    /// anchor's tangent frame at [`HingePivot::reference_angle`].
    pub offset:          Vec3,
    /// Hinge angle in radians at which pivot translation is zero.
    pub reference_angle: f32,
}

impl HingePivot {
    fn pose(
        &self,
        hinge: &Hinge,
        axis: Dir3,
        attachment: Option<&AnchoredTo>,
        geometry: &ResolvedAnchorGeometry,
    ) -> Result<AnchorPose, HingePivotError> {
        if !self.offset.is_finite() || !self.reference_angle.is_finite() || !hinge.angle.is_finite()
        {
            return Err(HingePivotError::NonFiniteInput);
        }

        let attachment = attachment.ok_or(HingePivotError::MissingRelationship)?;
        let source_point = geometry.points.get(&attachment.source_anchor).ok_or(
            HingePivotError::MissingSourceAnchor(attachment.source_anchor),
        )?;
        let source_frame = source_point.rotation();
        if !source_frame.is_finite() || !source_frame.is_normalized() {
            return Err(HingePivotError::InvalidSourceFrame);
        }

        let axis_in_tangent_frame = Dir3::new(source_frame.inverse() * *axis)
            .map_err(|_| HingePivotError::InvalidSourceFrame)?;
        let rotation = hinge.rotation_about(axis_in_tangent_frame);
        let delta = hinge.angle - self.reference_angle;
        let pivot_rotation = Quat::from_axis_angle(*axis_in_tangent_frame, delta);
        let translation = self.offset - pivot_rotation * self.offset;

        Ok(AnchorPose {
            rotation,
            translation,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HingePivotError {
    MissingSourceGeometry,
    MissingRelationship,
    MissingSourceAnchor(AnchorId),
    InvalidSourceFrame,
    NonFiniteInput,
}

/// Writes hinge rotations and optional pivot translations into [`AnchorPose`].
///
/// Register this system in [`AnchorSystems::AnimatePose`](crate::AnchorSystems::AnimatePose)
/// before running [`resolve_anchors`](crate::resolve_anchors). Entity-local edge
/// or pivot failures skip only the failing entity and emit a tracing warning.
/// Without [`HingePivot`], this system preserves the original hinge behavior
/// and writes [`Vec3::ZERO`] to [`AnchorPose::translation`].
pub fn hinge_to_pose(
    system_tick: SystemChangeTick,
    mut hinges: Query<(
        Entity,
        &Hinge,
        Option<&HingePivot>,
        Option<&AnchoredTo>,
        Option<&ResolvedAnchorGeometry>,
        &mut AnchorPose,
    )>,
) {
    for (entity, hinge, pivot, attachment, geometry, mut pose) in &mut hinges {
        let Some(geometry) = geometry else {
            if pivot.is_some() {
                tracing::warn!(
                    entity = ?entity,
                    error = ?HingePivotError::MissingSourceGeometry,
                    "hinge pivot unavailable"
                );
            }
            continue;
        };
        let axis = match hinge.edge.axis(geometry) {
            Ok(axis) => axis,
            Err(error) => {
                tracing::warn!(
                    entity = ?entity,
                    error = ?error,
                    "hinge axis unavailable"
                );
                continue;
            },
        };
        let next_pose = match pivot {
            Some(pivot) => match pivot.pose(hinge, axis, attachment, geometry) {
                Ok(next_pose) => next_pose,
                Err(error) => {
                    tracing::warn!(
                        entity = ?entity,
                        error = ?error,
                        "hinge pivot unavailable"
                    );
                    continue;
                },
            },
            None => AnchorPose {
                rotation:    hinge.rotation_about(axis),
                translation: Vec3::ZERO,
            },
        };
        if !next_pose.rotation.is_finite() || !next_pose.translation.is_finite() {
            tracing::warn!(entity = ?entity, "hinge pose is non-finite");
            continue;
        }
        warn_if_pose_was_changed(entity, &pose, system_tick);
        *pose = next_pose;
    }
}

fn warn_if_pose_was_changed(entity: Entity, pose: &Mut<AnchorPose>, system_tick: SystemChangeTick) {
    #[cfg(debug_assertions)]
    {
        let last_run = system_tick.last_run();
        let this_run = system_tick.this_run();
        if pose.last_changed().is_newer_than(last_run, this_run)
            && pose.last_changed() != pose.added()
        {
            tracing::warn!(
                entity = ?entity,
                "hinge overwrote an AnchorPose changed earlier this frame"
            );
        }
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = entity;
        let _ = pose;
        let _ = system_tick;
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::entity::Entity;
    use bevy_ecs::prelude::Component;
    use bevy_ecs::prelude::Query;
    use bevy_ecs::schedule::IntoScheduleConfigs;
    use bevy_ecs::schedule::Schedule;
    use bevy_ecs::world::World;
    use bevy_math::Quat;
    use bevy_math::Vec3;
    use bevy_platform::collections::HashMap;
    use bevy_transform::prelude::GlobalTransform;
    use bevy_transform::prelude::Transform;

    use super::Hinge;
    use super::HingePivot;
    use super::hinge_to_pose;
    use crate::AnchorId;
    use crate::AnchorPoint;
    use crate::AnchorPose;
    use crate::AnchorSystems;
    use crate::AnchoredTo;
    use crate::Edge;
    use crate::EdgeAxisError;
    use crate::ResolvedAnchorGeometry;
    use crate::resolve;

    const ACCORDION_SIGN_PERIOD: usize = 2;
    const ACCORDION_TILES: usize = 5;
    const ASSERT_EPSILON: f32 = 1e-4;
    const FIRST_HINGED_ORDER: usize = ROOT_TILE_ORDER + 1;
    const FOLD_ANGLE: f32 = core::f32::consts::FRAC_PI_2;
    const HALF_TILE_HEIGHT: f32 = 0.5;
    const INTERMEDIATE_ANGLE: f32 = core::f32::consts::FRAC_PI_4;
    const NEGATIVE_FOLD_SIGN: f32 = -1.0;
    const POSITIVE_FOLD_SIGN: f32 = 1.0;
    const PIVOT_OFFSET: Vec3 = Vec3::new(0.0, 0.0, 0.25);
    const REFERENCE_ANGLE: f32 = core::f32::consts::FRAC_PI_4;
    const ROOT_TILE_ORDER: usize = 0;
    const TILE_WIDTH: f32 = 2.0;
    const UNCHANGED_POSE_TRANSLATION: Vec3 = Vec3::new(1.0, 2.0, 3.0);

    #[derive(Component)]
    struct TileOrder(usize);

    #[test]
    fn five_quad_accordion_folds_about_shared_edges() {
        let mut world = resolve::world_with_diagnostics();
        let tiles = spawn_accordion_tiles(&mut world);
        let mut animate_schedule = Schedule::default();
        animate_schedule.add_systems(
            (drive_accordion_hinges, hinge_to_pose)
                .chain()
                .in_set(AnchorSystems::AnimatePose),
        );

        animate_schedule.run(&mut world);
        resolve::run_resolve(&mut world);

        for (order, entity) in tiles.into_iter().enumerate() {
            assert_accordion_transform(&world, entity, order);
        }
    }

    #[test]
    fn degenerate_edge_skips_pose_write_without_non_finite_transform() {
        let mut world = resolve::world_with_diagnostics();
        let entity = world
            .spawn((
                degenerate_geometry(),
                Hinge {
                    edge:  top_edge(),
                    angle: FOLD_ANGLE,
                },
                unchanged_pose(),
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(hinge_to_pose.in_set(AnchorSystems::AnimatePose));

        schedule.run(&mut world);

        let rotation = world
            .get::<Hinge>(entity)
            .zip(world.get::<ResolvedAnchorGeometry>(entity))
            .map(|(hinge, geometry)| hinge.rotation(geometry));
        assert_eq!(rotation, Some(Err(EdgeAxisError::Degenerate)));
        assert_eq!(
            world.get::<AnchorPose>(entity).copied(),
            Some(unchanged_pose())
        );
        let transform = world.get::<Transform>(entity).copied().unwrap_or_default();
        assert!(transform.translation.is_finite());
        assert!(transform.rotation.is_finite());
        assert!(transform.scale.is_finite());
    }

    #[test]
    fn endpoint_swap_flips_fold_direction() {
        let geometry = resolve::quad_geometry();
        let forward = Hinge {
            edge:  top_edge(),
            angle: FOLD_ANGLE,
        }
        .rotation(&geometry);
        let swapped = Hinge {
            edge:  Edge {
                start: AnchorId::Vertex(1),
                end:   AnchorId::Vertex(0),
            },
            angle: FOLD_ANGLE,
        }
        .rotation(&geometry);

        assert_eq!(forward, Ok(Quat::from_rotation_x(FOLD_ANGLE)));
        assert_eq!(swapped, Ok(Quat::from_rotation_x(-FOLD_ANGLE)));
    }

    #[test]
    fn zero_pivot_keeps_translation_zero() {
        let mut world = resolve::world_with_diagnostics();
        let entity = spawn_pivot_quad(
            &mut world,
            FOLD_ANGLE,
            HingePivot {
                offset:          Vec3::ZERO,
                reference_angle: 0.0,
            },
            top_edge(),
        );

        run_hinge_driver(&mut world);

        assert_eq!(
            world.get::<AnchorPose>(entity).copied(),
            Some(AnchorPose {
                rotation:    Quat::from_rotation_x(FOLD_ANGLE),
                translation: Vec3::ZERO,
            })
        );
    }

    #[test]
    fn external_pivot_translates_at_intermediate_and_endpoint_angles() {
        let mut world = resolve::world_with_diagnostics();
        let angles = [INTERMEDIATE_ANGLE, FOLD_ANGLE];
        let entities = angles.map(|angle| {
            spawn_pivot_quad(
                &mut world,
                angle,
                HingePivot {
                    offset:          PIVOT_OFFSET,
                    reference_angle: 0.0,
                },
                top_edge(),
            )
        });

        run_hinge_driver(&mut world);

        for (entity, angle) in entities.into_iter().zip(angles) {
            let rotation = Quat::from_rotation_x(angle);
            assert_eq!(
                world.get::<AnchorPose>(entity).copied(),
                Some(AnchorPose {
                    rotation,
                    translation: PIVOT_OFFSET - rotation * PIVOT_OFFSET,
                })
            );
        }
    }

    #[test]
    fn nonzero_reference_angle_has_zero_translation_at_reference() {
        let mut world = resolve::world_with_diagnostics();
        let angles = [REFERENCE_ANGLE, REFERENCE_ANGLE + FOLD_ANGLE];
        let entities = angles.map(|angle| {
            spawn_pivot_quad(
                &mut world,
                angle,
                HingePivot {
                    offset:          PIVOT_OFFSET,
                    reference_angle: REFERENCE_ANGLE,
                },
                top_edge(),
            )
        });

        run_hinge_driver(&mut world);

        let reference_pose = pose(&world, entities[0]);
        assert_close_quat(
            reference_pose.rotation,
            Quat::from_rotation_x(REFERENCE_ANGLE),
        );
        assert_close_vec3(reference_pose.translation, Vec3::ZERO);

        let moved_pose = pose(&world, entities[1]);
        assert_close_quat(
            moved_pose.rotation,
            Quat::from_rotation_x(REFERENCE_ANGLE + FOLD_ANGLE),
        );
        assert_close_vec3(
            moved_pose.translation,
            PIVOT_OFFSET - Quat::from_rotation_x(FOLD_ANGLE) * PIVOT_OFFSET,
        );
    }

    #[test]
    fn non_identity_source_frame_keeps_world_pivot_invariant() {
        let mut world = resolve::world_with_diagnostics();
        let target_translation = Vec3::new(2.0, -1.0, 3.0);
        let target =
            resolve::spawn_quad(&mut world, Transform::from_translation(target_translation));
        let source_frame = Quat::from_rotation_z(FOLD_ANGLE);
        let source = world
            .spawn((
                framed_hinge_geometry(source_frame),
                Transform::default(),
                GlobalTransform::default(),
                AnchoredTo::new(target, AnchorId::Center, AnchorId::Center),
                AnchorPose::default(),
                HingePivot {
                    offset:          PIVOT_OFFSET,
                    reference_angle: REFERENCE_ANGLE,
                },
            ))
            .id();
        let reference_rotation = Quat::from_rotation_x(REFERENCE_ANGLE);
        let pivot_local = source_frame * (reference_rotation.inverse() * PIVOT_OFFSET);
        let angles = [
            REFERENCE_ANGLE,
            0.0,
            INTERMEDIATE_ANGLE + FOLD_ANGLE,
            core::f32::consts::PI,
        ];

        for angle in angles {
            world.entity_mut(source).insert(Hinge {
                edge: vertical_edge(),
                angle,
            });
            run_hinge_driver(&mut world);
            resolve::run_resolve(&mut world);

            let transform = world.get::<Transform>(source).copied().unwrap_or_default();
            assert_close_vec3(
                transform.transform_point(pivot_local),
                target_translation + PIVOT_OFFSET,
            );
            assert!(transform.translation.is_finite());
            assert!(transform.rotation.is_finite());
            assert!(transform.scale.is_finite());
        }
    }

    #[test]
    fn pivot_endpoint_swap_flips_rotation_and_translation() {
        let mut world = resolve::world_with_diagnostics();
        let pivot = HingePivot {
            offset:          PIVOT_OFFSET,
            reference_angle: 0.0,
        };
        let forward = spawn_pivot_quad(&mut world, FOLD_ANGLE, pivot, top_edge());
        let reversed = spawn_pivot_quad(
            &mut world,
            FOLD_ANGLE,
            pivot,
            Edge {
                start: AnchorId::Vertex(1),
                end:   AnchorId::Vertex(0),
            },
        );

        run_hinge_driver(&mut world);

        let forward_rotation = Quat::from_rotation_x(FOLD_ANGLE);
        let reversed_rotation = Quat::from_rotation_x(-FOLD_ANGLE);
        assert_eq!(
            pose(&world, forward),
            AnchorPose {
                rotation:    forward_rotation,
                translation: PIVOT_OFFSET - forward_rotation * PIVOT_OFFSET,
            }
        );
        assert_eq!(
            pose(&world, reversed),
            AnchorPose {
                rotation:    reversed_rotation,
                translation: PIVOT_OFFSET - reversed_rotation * PIVOT_OFFSET,
            }
        );
    }

    #[test]
    fn missing_pivot_relationship_or_source_data_skips_pose_write() {
        let mut world = resolve::world_with_diagnostics();
        let target = resolve::spawn_quad(&mut world, Transform::default());
        let without_relationship = resolve::spawn_quad(&mut world, Transform::default());
        world.entity_mut(without_relationship).insert((
            unchanged_pose(),
            Hinge {
                edge:  top_edge(),
                angle: FOLD_ANGLE,
            },
            HingePivot {
                offset:          PIVOT_OFFSET,
                reference_angle: 0.0,
            },
        ));
        let without_source_anchor = resolve::spawn_quad(&mut world, Transform::default());
        world.entity_mut(without_source_anchor).insert((
            AnchoredTo::new(target, AnchorId::EdgeMid(u32::MAX), AnchorId::Center),
            unchanged_pose(),
            Hinge {
                edge:  top_edge(),
                angle: FOLD_ANGLE,
            },
            HingePivot {
                offset:          PIVOT_OFFSET,
                reference_angle: 0.0,
            },
        ));
        let without_source_geometry = world
            .spawn((
                AnchoredTo::new(target, AnchorId::Center, AnchorId::Center),
                unchanged_pose(),
                Hinge {
                    edge:  top_edge(),
                    angle: FOLD_ANGLE,
                },
                HingePivot {
                    offset:          PIVOT_OFFSET,
                    reference_angle: 0.0,
                },
            ))
            .id();
        let invalid_source_frame = world
            .spawn((
                framed_hinge_geometry(Quat::from_xyzw(f32::NAN, 0.0, 0.0, 1.0)),
                AnchoredTo::new(target, AnchorId::Center, AnchorId::Center),
                unchanged_pose(),
                Hinge {
                    edge:  vertical_edge(),
                    angle: FOLD_ANGLE,
                },
                HingePivot {
                    offset:          PIVOT_OFFSET,
                    reference_angle: 0.0,
                },
            ))
            .id();

        run_hinge_driver(&mut world);

        assert_eq!(pose(&world, without_relationship), unchanged_pose());
        assert_eq!(pose(&world, without_source_anchor), unchanged_pose());
        assert_eq!(pose(&world, without_source_geometry), unchanged_pose());
        assert_eq!(pose(&world, invalid_source_frame), unchanged_pose());
    }

    #[test]
    fn degenerate_pivot_edge_skips_pose_write() {
        let mut world = resolve::world_with_diagnostics();
        let target = resolve::spawn_quad(&mut world, Transform::default());
        let entity = world
            .spawn((
                degenerate_geometry(),
                AnchoredTo::new(target, AnchorId::Vertex(0), AnchorId::Center),
                Hinge {
                    edge:  top_edge(),
                    angle: FOLD_ANGLE,
                },
                HingePivot {
                    offset:          PIVOT_OFFSET,
                    reference_angle: 0.0,
                },
                unchanged_pose(),
            ))
            .id();

        run_hinge_driver(&mut world);

        assert_eq!(pose(&world, entity), unchanged_pose());
    }

    #[test]
    fn non_finite_pivot_input_skips_pose_write_and_keeps_transform_finite() {
        let mut world = resolve::world_with_diagnostics();
        let entity = spawn_pivot_quad(
            &mut world,
            FOLD_ANGLE,
            HingePivot {
                offset:          Vec3::NAN,
                reference_angle: 0.0,
            },
            top_edge(),
        );

        run_hinge_driver(&mut world);

        assert_eq!(pose(&world, entity), unchanged_pose());
        let transform = world.get::<Transform>(entity).copied().unwrap_or_default();
        assert!(transform.translation.is_finite());
        assert!(transform.rotation.is_finite());
        assert!(transform.scale.is_finite());
    }

    fn drive_accordion_hinges(mut hinges: Query<(&TileOrder, &mut Hinge)>) {
        for (order, mut hinge) in &mut hinges {
            hinge.angle = crease_sign(order.0) * FOLD_ANGLE;
        }
    }

    const fn crease_sign(order: usize) -> f32 {
        if order.is_multiple_of(ACCORDION_SIGN_PERIOD) {
            NEGATIVE_FOLD_SIGN
        } else {
            POSITIVE_FOLD_SIGN
        }
    }

    fn spawn_accordion_tiles(world: &mut World) -> [Entity; ACCORDION_TILES] {
        let root = resolve::spawn_quad(world, Transform::default());
        let mut tiles = [root; ACCORDION_TILES];
        let mut parent = root;
        for (order, slot) in tiles.iter_mut().enumerate().skip(FIRST_HINGED_ORDER) {
            let entity = spawn_hinged_tile(world, parent, order);
            *slot = entity;
            parent = entity;
        }
        tiles
    }

    fn spawn_hinged_tile(world: &mut World, parent: Entity, order: usize) -> Entity {
        let entity = resolve::spawn_quad(world, Transform::default());
        world.entity_mut(entity).insert((
            AnchoredTo::new(parent, AnchorId::Vertex(0), AnchorId::Vertex(1)),
            AnchorPose::default(),
            Hinge {
                edge:  top_edge(),
                angle: 0.0,
            },
            TileOrder(order),
        ));
        entity
    }

    fn spawn_pivot_quad(world: &mut World, angle: f32, pivot: HingePivot, edge: Edge) -> Entity {
        let target = resolve::spawn_quad(world, Transform::default());
        let source = resolve::spawn_quad(world, Transform::default());
        world.entity_mut(source).insert((
            AnchoredTo::new(target, AnchorId::Center, AnchorId::Center),
            unchanged_pose(),
            Hinge { edge, angle },
            pivot,
        ));
        source
    }

    fn run_hinge_driver(world: &mut World) {
        let mut schedule = Schedule::default();
        schedule.add_systems(hinge_to_pose.in_set(AnchorSystems::AnimatePose));
        schedule.run(world);
    }

    fn framed_hinge_geometry(source_frame: Quat) -> ResolvedAnchorGeometry {
        ResolvedAnchorGeometry {
            points: HashMap::from_iter([
                (
                    AnchorId::Center,
                    AnchorPoint {
                        position: Vec3::ZERO,
                        frame:    Some(source_frame),
                    },
                ),
                (
                    AnchorId::Vertex(0),
                    AnchorPoint {
                        position: Vec3::NEG_Y,
                        frame:    None,
                    },
                ),
                (
                    AnchorId::Vertex(1),
                    AnchorPoint {
                        position: Vec3::Y,
                        frame:    None,
                    },
                ),
            ]),
            edges:  vec![vertical_edge()],
        }
    }

    fn degenerate_geometry() -> ResolvedAnchorGeometry {
        ResolvedAnchorGeometry {
            points: HashMap::from_iter([
                (
                    AnchorId::Vertex(0),
                    AnchorPoint {
                        position: Vec3::ZERO,
                        frame:    None,
                    },
                ),
                (
                    AnchorId::Vertex(1),
                    AnchorPoint {
                        position: Vec3::ZERO,
                        frame:    None,
                    },
                ),
            ]),
            edges:  vec![top_edge()],
        }
    }

    const fn top_edge() -> Edge {
        Edge {
            start: AnchorId::Vertex(0),
            end:   AnchorId::Vertex(1),
        }
    }

    const fn vertical_edge() -> Edge {
        Edge {
            start: AnchorId::Vertex(0),
            end:   AnchorId::Vertex(1),
        }
    }

    fn unchanged_pose() -> AnchorPose {
        AnchorPose {
            rotation:    Quat::from_rotation_y(FOLD_ANGLE),
            translation: UNCHANGED_POSE_TRANSLATION,
        }
    }

    fn pose(world: &World, entity: Entity) -> AnchorPose {
        world.get::<AnchorPose>(entity).copied().unwrap_or_default()
    }

    fn assert_accordion_transform(world: &World, entity: Entity, order: usize) {
        let translation = if order.is_multiple_of(ACCORDION_SIGN_PERIOD) {
            Vec3::new(tile_x(order), 0.0, 0.0)
        } else {
            Vec3::new(tile_x(order), HALF_TILE_HEIGHT, -HALF_TILE_HEIGHT)
        };
        let rotation = if order.is_multiple_of(ACCORDION_SIGN_PERIOD) {
            Quat::IDENTITY
        } else {
            Quat::from_rotation_x(FOLD_ANGLE)
        };
        assert_transform(world, entity, translation, rotation);
    }

    fn tile_x(order: usize) -> f32 {
        let mut x = 0.0;
        for _ in 0..order {
            x += TILE_WIDTH;
        }
        x
    }

    fn assert_transform(world: &World, entity: Entity, translation: Vec3, rotation: Quat) {
        let transform = world.get::<Transform>(entity).copied().unwrap_or_default();
        assert_close_vec3(transform.translation, translation);
        assert_close_quat(transform.rotation, rotation);
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
