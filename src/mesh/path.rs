use bevy::prelude::*;

use crate::routing::CableGeometry;

/// Result of flattening all geometry segments into a single continuous polyline.
pub(super) struct FlattenedGeometry {
    pub(super) points:      Vec<Vec3>,
    pub(super) tangents:    Vec<Vec3>,
    pub(super) arc_lengths: Vec<f32>,
}

/// Flatten all geometry segments into one continuous polyline, deduplicating boundaries.
pub(super) fn flatten_geometry(geometry: &CableGeometry) -> FlattenedGeometry {
    let mut points = Vec::new();
    let mut tangents = Vec::new();
    let mut arc_lengths = Vec::new();
    let mut arc_offset = 0.0_f32;

    for segment in &geometry.segments {
        if segment.points.len() < 2 {
            arc_offset += segment.length;
            continue;
        }

        let start_idx = usize::from(!points.is_empty());
        for i in start_idx..segment.points.len() {
            points.push(segment.points[i]);
            tangents.push(segment.tangents[i]);
            arc_lengths.push(segment.arc_lengths[i] + arc_offset);
        }

        arc_offset += segment.length;
    }

    FlattenedGeometry {
        points,
        tangents,
        arc_lengths,
    }
}

/// Trim the start and/or end of a path by removing points within the trim distance.
pub(super) fn trim_path(
    points: &mut Vec<Vec3>,
    tangents: &mut Vec<Vec3>,
    arc_lengths: &mut Vec<f32>,
    trim_start: f32,
    trim_end: f32,
) {
    let total = *arc_lengths.last().unwrap_or(&0.0);

    if trim_start > 0.0 && points.len() >= 2 {
        let cut = arc_lengths
            .iter()
            .position(|&arc_length| arc_length >= trim_start)
            .unwrap_or(0);
        if cut > 0 && cut < points.len() {
            let prev = cut - 1;
            let segment_length = arc_lengths[cut] - arc_lengths[prev];
            if segment_length > f32::EPSILON {
                let t = (trim_start - arc_lengths[prev]) / segment_length;
                let new_point = points[prev].lerp(points[cut], t);
                let new_tangent = tangents[prev].lerp(tangents[cut], t).normalize_or_zero();
                points[prev] = new_point;
                tangents[prev] = new_tangent;
                arc_lengths[prev] = trim_start;
            }
            points.drain(..prev);
            tangents.drain(..prev);
            arc_lengths.drain(..prev);
        }
    }

    if trim_end > 0.0 && points.len() >= 2 {
        let end_boundary = total - trim_end;
        let mut cut = points.len();
        for i in (0..points.len()).rev() {
            if arc_lengths[i] <= end_boundary {
                cut = i + 1;
                break;
            }
        }
        if cut > 0 && cut < points.len() {
            let segment_length = arc_lengths[cut] - arc_lengths[cut - 1];
            if segment_length > f32::EPSILON {
                let t = (end_boundary - arc_lengths[cut - 1]) / segment_length;
                let new_point = points[cut - 1].lerp(points[cut], t);
                let new_tangent = tangents[cut - 1].lerp(tangents[cut], t).normalize_or_zero();
                points[cut] = new_point;
                tangents[cut] = new_tangent;
                arc_lengths[cut] = end_boundary;
            }
            points.truncate(cut + 1);
            tangents.truncate(cut + 1);
            arc_lengths.truncate(cut + 1);
        }
    }
}

/// Recompute tangents from path positions using segment directions.
pub(super) fn recompute_tangents(points: &[Vec3]) -> Vec<Vec3> {
    let point_count = points.len();
    if point_count == 0 {
        return Vec::new();
    }
    if point_count == 1 {
        return vec![Vec3::Z];
    }

    let mut tangents = Vec::with_capacity(point_count);
    tangents.push((points[1] - points[0]).normalize_or_zero());

    for i in 1..point_count - 1 {
        let incoming_direction = (points[i] - points[i - 1]).normalize_or_zero();
        let outgoing_direction = (points[i + 1] - points[i]).normalize_or_zero();
        tangents.push((incoming_direction + outgoing_direction).normalize_or_zero());
    }

    tangents.push((points[point_count - 1] - points[point_count - 2]).normalize_or_zero());
    tangents
}
