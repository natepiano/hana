use bevy::prelude::*;

use super::constants::PERPENDICULAR_AXIS_THRESHOLD;

/// Compute rotation-minimizing frames (parallel transport) along a curve.
pub(super) fn compute_rotation_minimizing_frames(
    points: &[Vec3],
    tangents: &[Vec3],
) -> Vec<(Vec3, Vec3)> {
    let point_count = points.len();
    if point_count == 0 {
        return Vec::new();
    }

    let mut frames = Vec::with_capacity(point_count);
    let first_tangent = tangents[0];
    let initial_normal = find_perpendicular(first_tangent);
    let initial_binormal = first_tangent.cross(initial_normal).normalize_or_zero();
    frames.push((initial_normal, initial_binormal));

    for i in 1..point_count {
        let (prev_normal, ..) = frames[i - 1];
        let prev_tangent = tangents[i - 1];
        let current_tangent = tangents[i];

        let edge_vector = points[i] - points[i - 1];
        let edge_length_squared = edge_vector.dot(edge_vector);
        if edge_length_squared < f32::EPSILON {
            frames.push(frames[i - 1]);
            continue;
        }

        let reflected_normal =
            prev_normal - (2.0 / edge_length_squared) * edge_vector.dot(prev_normal) * edge_vector;
        let reflected_tangent = prev_tangent
            - (2.0 / edge_length_squared) * edge_vector.dot(prev_tangent) * edge_vector;

        let tangent_delta = current_tangent - reflected_tangent;
        let tangent_delta_length_squared = tangent_delta.dot(tangent_delta);

        let next_normal = if tangent_delta_length_squared < f32::EPSILON {
            reflected_normal
        } else {
            reflected_normal
                - (2.0 / tangent_delta_length_squared)
                    * tangent_delta.dot(reflected_normal)
                    * tangent_delta
        }
        .normalize_or_zero();

        let next_binormal = current_tangent.cross(next_normal).normalize_or_zero();
        frames.push((next_normal, next_binormal));
    }

    frames
}

fn find_perpendicular(direction: Vec3) -> Vec3 {
    let candidate = if direction.x.abs() < PERPENDICULAR_AXIS_THRESHOLD {
        Vec3::X
    } else {
        Vec3::Y
    };
    direction.cross(candidate).normalize_or_zero()
}
