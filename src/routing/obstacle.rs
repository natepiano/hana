//! `Obstacle` `AABB` type and the point/segment blocking helpers that operate on it.

use bevy::math::Vec3;
use bevy::reflect::Reflect;
use bevy_kana::ToF32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PointContainment {
    Outside,
    Inside,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Blockage {
    Clear,
    Blocked,
}

/// An axis-aligned bounding box used as a routing obstacle.
#[derive(Clone, Copy, Debug, Reflect)]
pub struct Obstacle {
    pub(super) half_extents: Vec3,
    position:                Vec3,
}

impl Obstacle {
    /// Create an axis-aligned obstacle.
    #[must_use]
    pub fn new(half_extents: Vec3, position: impl Into<Vec3>) -> Self {
        Self {
            half_extents,
            position: position.into(),
        }
    }

    fn point_containment(&self, position: Vec3, margin: f32) -> PointContainment {
        let min = self.position - self.half_extents - Vec3::splat(margin);
        let max = self.position + self.half_extents + Vec3::splat(margin);
        if position.x >= min.x
            && position.x <= max.x
            && position.y >= min.y
            && position.y <= max.y
            && position.z >= min.z
            && position.z <= max.z
        {
            PointContainment::Inside
        } else {
            PointContainment::Outside
        }
    }
}

/// Check whether a point falls inside any obstacle's `AABB`, expanded by `margin`.
#[must_use]
pub(super) fn is_point_in_any_obstacle(
    position: Vec3,
    obstacles: &[Obstacle],
    margin: f32,
) -> PointContainment {
    for obstacle in obstacles {
        match obstacle.point_containment(position, margin) {
            PointContainment::Inside => return PointContainment::Inside,
            PointContainment::Outside => {},
        }
    }

    PointContainment::Outside
}

/// Check whether any obstacle intersects a line segment by sampling `steps`
/// evenly-spaced points.
#[must_use]
pub(super) fn is_segment_blocked(
    start: Vec3,
    end: Vec3,
    obstacles: &[Obstacle],
    margin: f32,
    steps: u32,
) -> Blockage {
    for i in 0..=steps {
        let t = i.to_f32() / steps.to_f32();
        let point = start.lerp(end, t);
        match is_point_in_any_obstacle(point, obstacles, margin) {
            PointContainment::Inside => return Blockage::Blocked,
            PointContainment::Outside => {},
        }
    }

    Blockage::Clear
}
