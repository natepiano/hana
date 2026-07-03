//! `RouteObstacle` — declares an entity as a cable-routing obstacle — and
//! `resolve_obstacles`, which snapshots every tagged entity into the routing
//! layer's world-space [`Obstacle`] boxes at recompute time.

use bevy::camera::primitives::Aabb;
use bevy::prelude::*;

use crate::routing::Obstacle;

/// Declares that cables route around this entity.
///
/// The component stores no position: each recompute reads the entity's live
/// `GlobalTransform` through `resolve_obstacles` and produces a fresh
/// world-space [`Obstacle`] snapshot for the solver, so there is no stored
/// copy of the transform to go stale.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub enum RouteObstacle {
    /// Bounds derived from the render [`Aabb`]s of the entity and its
    /// descendants — covers entities whose meshes live on child entities.
    #[default]
    FromRenderAabb,
    /// Explicit local half-extents centered on the entity, for entities whose
    /// visual bounds differ from the bounds cables should respect.
    HalfExtents(Vec3),
}

/// World-space bounds accumulated from transformed box corners.
struct WorldExtents {
    min: Vec3,
    max: Vec3,
}

impl WorldExtents {
    fn include(&mut self, corner: Vec3) {
        self.min = self.min.min(corner);
        self.max = self.max.max(corner);
    }
}

impl From<Vec3> for WorldExtents {
    fn from(corner: Vec3) -> Self {
        Self {
            min: corner,
            max: corner,
        }
    }
}

impl From<WorldExtents> for Obstacle {
    fn from(extents: WorldExtents) -> Self {
        let half_extents = (extents.max - extents.min) / 2.0;
        Self::new(half_extents, (extents.min + extents.max) / 2.0)
    }
}

/// Snapshot every [`RouteObstacle`] entity into a world-space [`Obstacle`] box.
/// Entities whose bounds cannot be resolved (no render [`Aabb`] anywhere in
/// their tree yet) contribute nothing.
pub(super) fn resolve_obstacles(
    route_obstacles: &Query<(Entity, &RouteObstacle, &GlobalTransform)>,
    children: &Query<&Children>,
    aabbs: &Query<&Aabb>,
    transforms: &Query<&GlobalTransform>,
) -> Vec<Obstacle> {
    route_obstacles
        .iter()
        .filter_map(|(entity, route_obstacle, transform)| {
            match route_obstacle {
                RouteObstacle::FromRenderAabb => {
                    render_extents(entity, children, aabbs, transforms)
                },
                RouteObstacle::HalfExtents(half_extents) => {
                    let mut extents = None;
                    include_world_corners(&mut extents, -*half_extents, *half_extents, transform);
                    extents
                },
            }
            .map(Obstacle::from)
        })
        .collect()
}

/// Merge the render [`Aabb`]s of `entity` and its descendants into world-space
/// extents.
fn render_extents(
    entity: Entity,
    children: &Query<&Children>,
    aabbs: &Query<&Aabb>,
    transforms: &Query<&GlobalTransform>,
) -> Option<WorldExtents> {
    let mut extents = None;
    for candidate in std::iter::once(entity).chain(children.iter_descendants(entity)) {
        let (Ok(aabb), Ok(transform)) = (aabbs.get(candidate), transforms.get(candidate)) else {
            continue;
        };
        include_world_corners(
            &mut extents,
            aabb.min().into(),
            aabb.max().into(),
            transform,
        );
    }
    extents
}

/// Transform the 8 corners of a local-space box into world space and merge
/// them into `extents`, so a rotated box still yields a conservative
/// axis-aligned world box.
fn include_world_corners(
    extents: &mut Option<WorldExtents>,
    min: Vec3,
    max: Vec3,
    transform: &GlobalTransform,
) {
    let corners = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, max.y, max.z),
    ];
    for corner in corners {
        let world_corner = transform.transform_point(corner);
        match extents {
            Some(extents) => extents.include(world_corner),
            None => *extents = Some(WorldExtents::from(world_corner)),
        }
    }
}
