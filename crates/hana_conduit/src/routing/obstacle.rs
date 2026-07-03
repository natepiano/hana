//! `Obstacle`, `PointContainment`, `Blockage`, `is_point_in_any_obstacle`, and
//! `is_segment_blocked`.

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
    pub(super) position:     Vec3,
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

    /// Move a point inside this box out through its nearest face, landing
    /// `clearance` metres beyond the face. Points already outside pass through
    /// unchanged.
    fn push_out(&self, point: Vec3, clearance: f32) -> Vec3 {
        match self.point_containment(point, 0.0) {
            PointContainment::Outside => point,
            PointContainment::Inside => {
                let min = self.position - self.half_extents;
                let max = self.position + self.half_extents;
                let exits = [
                    (
                        point.x - min.x,
                        Vec3::new(min.x - clearance, point.y, point.z),
                    ),
                    (
                        max.x - point.x,
                        Vec3::new(max.x + clearance, point.y, point.z),
                    ),
                    (
                        point.y - min.y,
                        Vec3::new(point.x, min.y - clearance, point.z),
                    ),
                    (
                        max.y - point.y,
                        Vec3::new(point.x, max.y + clearance, point.z),
                    ),
                    (
                        point.z - min.z,
                        Vec3::new(point.x, point.y, min.z - clearance),
                    ),
                    (
                        max.z - point.z,
                        Vec3::new(point.x, point.y, max.z + clearance),
                    ),
                ];
                exits
                    .into_iter()
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map_or(point, |(_, exit)| exit)
            },
        }
    }
}

/// Move `point` outside every [`Obstacle`] box it falls inside, exiting each
/// through its nearest face plus `clearance`. Used by route animation to keep
/// an in-flight cable from sweeping through the boxes its target route avoids.
#[must_use]
pub(crate) fn push_out_of_obstacles(point: Vec3, obstacles: &[Obstacle], clearance: f32) -> Vec3 {
    obstacles.iter().fold(point, |current, obstacle| {
        obstacle.push_out(current, clearance)
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    const CLEARANCE: f32 = 0.1;

    #[test]
    fn push_out_moves_inside_point_through_nearest_face() {
        let obstacle = Obstacle::new(Vec3::ONE, Vec3::ZERO);
        let pushed = push_out_of_obstacles(Vec3::new(0.9, 0.0, 0.0), &[obstacle], CLEARANCE);
        assert_eq!(pushed, Vec3::new(1.0 + CLEARANCE, 0.0, 0.0));
    }

    #[test]
    fn push_out_leaves_outside_point_unchanged() {
        let obstacle = Obstacle::new(Vec3::ONE, Vec3::ZERO);
        let point = Vec3::new(2.0, 0.0, 0.0);
        assert_eq!(push_out_of_obstacles(point, &[obstacle], CLEARANCE), point);
    }
}
