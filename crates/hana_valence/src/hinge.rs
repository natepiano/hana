//! Edge-fold animation driver.

use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::change_detection::Mut;
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Component;
use bevy_ecs::prelude::ReflectComponent;
use bevy_ecs::system::Query;
use bevy_ecs::system::SystemChangeTick;
use bevy_math::Quat;
use bevy_math::Vec3;
use bevy_reflect::Reflect;

use crate::AnchorPose;
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
        Ok(Quat::from_axis_angle(
            *self.edge.axis(geometry)?,
            self.angle,
        ))
    }
}

/// Writes hinge rotations into [`AnchorPose`] components.
///
/// Register this system in [`AnchorSystems::AnimatePose`](crate::AnchorSystems::AnimatePose)
/// before running [`resolve_anchors`](crate::resolve_anchors). Entity-local edge
/// failures skip only the failing entity and emit a tracing warning.
pub fn hinge_to_pose(
    system_tick: SystemChangeTick,
    mut hinges: Query<(Entity, &Hinge, &ResolvedAnchorGeometry, &mut AnchorPose)>,
) {
    for (entity, hinge, geometry, mut pose) in &mut hinges {
        let rotation = match hinge.rotation(geometry) {
            Ok(rotation) => rotation,
            Err(error) => {
                tracing::warn!(
                    entity = ?entity,
                    error = ?error,
                    "hinge axis unavailable"
                );
                continue;
            },
        };
        warn_if_pose_was_changed(entity, &pose, system_tick);
        *pose = AnchorPose {
            rotation,
            translation: Vec3::ZERO,
        };
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
    const NEGATIVE_FOLD_SIGN: f32 = -1.0;
    const POSITIVE_FOLD_SIGN: f32 = 1.0;
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

    fn unchanged_pose() -> AnchorPose {
        AnchorPose {
            rotation:    Quat::from_rotation_y(FOLD_ANGLE),
            translation: UNCHANGED_POSE_TRANSLATION,
        }
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
