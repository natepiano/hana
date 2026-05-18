use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use bevy_kana::ToF32;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use super::config::CableMeshConfig;
use super::constants::DEFAULT_ELBOW_ARM_FRACTION;
use super::constants::MAX_ARM_RATIO;
use super::constants::MIN_ELBOW_RINGS;
use super::path;
use crate::routing::CableGeometry;
use crate::routing::MIN_CABLE_SAMPLE_POINTS;

/// Resolve elbow arm lengths from per-elbow overrides or the global multiplier.
fn resolve_elbow_arms(
    config: &CableMeshConfig,
    elbow_idx: usize,
    fillet_start: Vec3,
    fillet_end: Vec3,
    max_arm: f32,
) -> (f32, f32) {
    config
        .elbow
        .arm_overrides
        .as_ref()
        .and_then(|overrides| overrides.get(elbow_idx))
        .map_or_else(
            || {
                let arm = (fillet_start.distance(fillet_end)
                    * DEFAULT_ELBOW_ARM_FRACTION
                    * config.elbow.arm_multiplier)
                    .min(max_arm);
                (arm, arm)
            },
            |&(first_arm_length, second_arm_length)| {
                (
                    first_arm_length.clamp(0.0, max_arm),
                    second_arm_length.clamp(0.0, max_arm),
                )
            },
        )
}

/// Metadata about a single elbow fillet, for visualization and interactive editing.
#[derive(Clone, Debug)]
pub struct ElbowMetadata {
    /// Fillet start point (on incoming straight section).
    pub fillet_start:         Vec3,
    /// First control point (along incoming direction from `fillet_start`).
    pub first_control_point:  Vec3,
    /// Second control point (along outgoing direction toward `fillet_end`).
    pub second_control_point: Vec3,
    /// Fillet end point (on outgoing straight section).
    pub fillet_end:           Vec3,
    /// Incoming segment direction at the elbow.
    pub incoming_direction:   Vec3,
    /// Outgoing segment direction at the elbow.
    pub outgoing_direction:   Vec3,
    /// Arm length for `first_control_point`.
    pub control1_arm:         f32,
    /// Arm length for `second_control_point`.
    pub control2_arm:         f32,
    /// Fillet reach distance.
    pub fillet_reach:         f32,
}

/// Pre-computed elbow detection parameters extracted from `CableMeshConfig`.
struct ElbowParams {
    angle_threshold_cos: f32,
    bend_radius:         f32,
    min_bend_radius:     f32,
}

impl From<&CableMeshConfig> for ElbowParams {
    fn from(config: &CableMeshConfig) -> Self {
        let tube_radius = config.tube.radius;
        Self {
            angle_threshold_cos: config.elbow.angle_threshold_deg.to_radians().cos(),
            bend_radius:         tube_radius * config.elbow.bend_radius_multiplier,
            min_bend_radius:     tube_radius * config.elbow.min_radius_multiplier,
        }
    }
}

/// Compute `ElbowMetadata` for a single corner point, if the bend is sharp enough.
fn compute_elbow_at_corner(
    incoming_direction: Vec3,
    outgoing_direction: Vec3,
    corner: Vec3,
    config: &CableMeshConfig,
    elbow_idx: usize,
    elbow_params: &ElbowParams,
) -> Option<ElbowMetadata> {
    let cos_angle = incoming_direction.dot(outgoing_direction).clamp(-1.0, 1.0);
    if cos_angle >= elbow_params.angle_threshold_cos {
        return None;
    }

    if elbow_params.bend_radius < elbow_params.min_bend_radius {
        return None;
    }

    let theta = cos_angle.acos();
    let half_theta = theta * 0.5;
    let fillet_reach = elbow_params.bend_radius * half_theta.tan();

    let fillet_start = corner - incoming_direction * fillet_reach;
    let fillet_end = corner + outgoing_direction * fillet_reach;
    let max_arm = fillet_reach * MAX_ARM_RATIO;
    let (control1_arm, control2_arm) =
        resolve_elbow_arms(config, elbow_idx, fillet_start, fillet_end, max_arm);
    let first_control_point = fillet_start + incoming_direction * control1_arm;
    let second_control_point = fillet_end - outgoing_direction * control2_arm;

    Some(ElbowMetadata {
        fillet_start,
        first_control_point,
        second_control_point,
        fillet_end,
        incoming_direction,
        outgoing_direction,
        control1_arm,
        control2_arm,
        fillet_reach,
    })
}

/// Smooth sharp bends in the path using cubic Bezier fillets.
pub(super) fn insert_knee_rings(
    points: Vec<Vec3>,
    arc_lengths: Vec<f32>,
    config: &CableMeshConfig,
) -> (Vec<Vec3>, Vec<Vec3>, Vec<f32>) {
    let point_count = points.len();
    if point_count < MIN_CABLE_SAMPLE_POINTS.to_usize() {
        let tangents = path::recompute_tangents(&points);
        return (points, tangents, arc_lengths);
    }

    let elbow_params = ElbowParams::from(config);
    let rings_per_right_angle = config.elbow.rings_per_right_angle;
    let mut output_points = Vec::with_capacity(point_count * 2);
    let mut output_arc_lengths = Vec::with_capacity(point_count * 2);

    output_points.push(points[0]);
    output_arc_lengths.push(arc_lengths[0]);

    let mut elbow_idx = 0_usize;
    let mut i = 1;
    while i < point_count {
        let incoming_direction = (points[i] - points[i - 1]).normalize_or_zero();
        let Some(next_point) = points.get(i + 1).copied() else {
            output_points.push(points[i]);
            output_arc_lengths.push(arc_lengths[i]);
            i += 1;
            continue;
        };
        let outgoing_direction = (next_point - points[i]).normalize_or_zero();

        let Some(metadata) = compute_elbow_at_corner(
            incoming_direction,
            outgoing_direction,
            points[i],
            config,
            elbow_idx,
            &elbow_params,
        ) else {
            output_points.push(points[i]);
            output_arc_lengths.push(arc_lengths[i]);
            i += 1;
            continue;
        };
        elbow_idx += 1;

        while output_points.len() > 1 {
            let last = output_points[output_points.len() - 1];
            if (last - metadata.fillet_start).dot(incoming_direction) > 0.0 {
                output_points.pop();
                output_arc_lengths.pop();
            } else {
                break;
            }
        }

        let base_arc = output_arc_lengths.last().copied().unwrap_or(0.0);
        let distance_to_fillet_start = output_points
            .last()
            .map_or(0.0, |last| last.distance(metadata.fillet_start));
        output_points.push(metadata.fillet_start);
        output_arc_lengths.push(base_arc + distance_to_fillet_start);

        let theta = metadata
            .incoming_direction
            .dot(metadata.outgoing_direction)
            .clamp(-1.0, 1.0)
            .acos();
        let ring_count = ((theta / FRAC_PI_2) * rings_per_right_angle.to_f32())
            .ceil()
            .max(MIN_ELBOW_RINGS)
            .to_u32();

        let fillet_base_arc = output_arc_lengths[output_arc_lengths.len() - 1];
        let bezier_midpoint = 0.125
            * (metadata.fillet_start
                + 3.0 * metadata.first_control_point
                + 3.0 * metadata.second_control_point
                + metadata.fillet_end)
            - 0.0625 * (3.0 * metadata.fillet_start + metadata.fillet_end);
        let fillet_length = metadata
            .fillet_start
            .distance(bezier_midpoint)
            .mul_add(2.0, bezier_midpoint.distance(metadata.fillet_end) * 2.0);

        for k in 1..=ring_count {
            let t = k.to_f32() / ring_count.to_f32();
            let one_minus_t = 1.0 - t;
            let position = one_minus_t * one_minus_t * one_minus_t * metadata.fillet_start
                + 3.0 * one_minus_t * one_minus_t * t * metadata.first_control_point
                + 3.0 * one_minus_t * t * t * metadata.second_control_point
                + t * t * t * metadata.fillet_end;

            output_points.push(position);
            output_arc_lengths.push(fillet_base_arc + t * fillet_length);
        }

        i += 1;
        while i < point_count {
            if (points[i] - metadata.fillet_end).dot(outgoing_direction) < 0.0 {
                i += 1;
            } else {
                break;
            }
        }
    }

    let output_tangents = path::recompute_tangents(&output_points);
    (output_points, output_tangents, output_arc_lengths)
}

/// Compute elbow metadata for all fillet bends in the geometry.
#[must_use]
pub fn compute_elbow_metadata(
    geometry: &CableGeometry,
    config: &CableMeshConfig,
) -> Vec<ElbowMetadata> {
    let flat = path::flatten_geometry(geometry);
    let mut points = flat.points;
    let mut arc_lengths = flat.arc_lengths;

    if points.len() < 3 {
        return Vec::new();
    }

    if config.trim.start > 0.0 || config.trim.end > 0.0 {
        let mut tangents = path::recompute_tangents(&points);
        path::trim_path(
            &mut points,
            &mut tangents,
            &mut arc_lengths,
            config.trim.start,
            config.trim.end,
        );
    }

    let elbow_params = ElbowParams::from(config);
    let mut elbows = Vec::new();
    let mut elbow_idx = 0_usize;

    for i in 1..points.len() - 1 {
        let incoming_direction = (points[i] - points[i - 1]).normalize_or_zero();
        let outgoing_direction = (points[i + 1] - points[i]).normalize_or_zero();

        if let Some(metadata) = compute_elbow_at_corner(
            incoming_direction,
            outgoing_direction,
            points[i],
            config,
            elbow_idx,
            &elbow_params,
        ) {
            elbows.push(metadata);
            elbow_idx += 1;
        }
    }

    elbows
}
