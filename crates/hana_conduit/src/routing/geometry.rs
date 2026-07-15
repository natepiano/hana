//! Route boundary types: `Anchor` connection points, `RouteRequest` inputs, and the
//! `CableSegment` / `CableGeometry` outputs that bridge route computation and rendering.

use std::iter;

use bevy::math::Dir3;
use bevy::math::Vec3;
use bevy_kana::ToF32;
use bevy_kana::ToUsize;

use super::constants::DEFAULT_RESOLUTION_SENTINEL;
use super::constants::MIN_CABLE_SAMPLE_POINTS;
use super::obstacle::Obstacle;

enum TangentSample {
    Only,
    First,
    Last,
    Interior,
}

impl TangentSample {
    const fn from_point_index(point_index: usize, point_count: usize) -> Self {
        match (point_count, point_index) {
            (0 | 1, _) => Self::Only,
            (_, 0) => Self::First,
            _ if point_index == point_count - 1 => Self::Last,
            _ => Self::Interior,
        }
    }
}

/// How a cable leaves an [`Anchor`] before the solver takes over.
///
/// This is the routing-layer, world-space form of a cable endpoint's exit
/// configuration — see `EndpointExit` in the `cable` module for the ECS-side
/// counterpart with a target-local axis.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AnchorExit {
    /// The solver routes directly from the anchor position.
    #[default]
    Unconstrained,
    /// The cable leaves along `direction` as a straight lead of `length`
    /// metres; the solver routes from the lead's tip.
    Lead {
        /// World-space direction the lead points along.
        direction: Dir3,
        /// Length of the straight lead, in metres.
        length:    f32,
    },
}

/// Where a cable connects to an object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    /// World-space position of the connection point.
    pub position: Vec3,
    /// How the cable leaves this anchor before the solver takes over.
    pub exit:     AnchorExit,
}

impl Anchor {
    /// World position of this anchor's lead tip, or `None` when the exit is
    /// [`AnchorExit::Unconstrained`].
    #[must_use]
    pub fn lead_tip(&self) -> Option<Vec3> {
        match self.exit {
            AnchorExit::Unconstrained => None,
            AnchorExit::Lead { direction, length } => Some(self.position + direction * length),
        }
    }
}

impl From<Vec3> for Anchor {
    fn from(position: Vec3) -> Self {
        Self {
            position,
            exit: AnchorExit::Unconstrained,
        }
    }
}

/// Everything a solver needs to compute a route.
#[derive(Clone, Debug)]
pub struct RouteRequest<'a> {
    /// Starting anchor of the cable.
    pub start:      Anchor,
    /// Ending anchor of the cable.
    pub end:        Anchor,
    /// Obstacles to route around (may be empty).
    pub obstacles:  &'a [Obstacle],
    /// Number of sample points per segment.
    pub resolution: u32,
}

impl RouteRequest<'_> {
    /// Returns the request's resolution if set, otherwise falls back to `default`.
    #[must_use]
    pub const fn effective_resolution(&self, default: u32) -> u32 {
        if self.resolution == DEFAULT_RESOLUTION_SENTINEL {
            default
        } else {
            self.resolution
        }
    }
}

/// A single continuous curve between two waypoints.
#[derive(Clone, Debug, Default)]
pub struct CableSegment {
    /// Sampled positions along the curve.
    pub points:      Vec<Vec3>,
    /// Unit tangent at each sample point.
    pub tangents:    Vec<Vec3>,
    /// Cumulative arc length at each sample point.
    pub arc_lengths: Vec<f32>,
    /// Total arc length of this segment.
    pub length:      f32,
}

impl CableSegment {
    /// Create a segment by evenly sampling `n` points along a straight line.
    #[must_use]
    pub fn straight_line(start: impl Into<Vec3>, end: impl Into<Vec3>, n: usize) -> Self {
        let start: Vec3 = start.into();
        let end: Vec3 = end.into();
        let n = n.max(MIN_CABLE_SAMPLE_POINTS.to_usize());
        let points: Vec<Vec3> = (0..n)
            .map(|i| {
                let t = i.to_f32() / (n - 1).to_f32();
                start.lerp(end, t)
            })
            .collect();
        points.into()
    }
}

impl From<Vec<Vec3>> for CableSegment {
    fn from(points: Vec<Vec3>) -> Self {
        if points.is_empty() {
            return Self::default();
        }

        let point_count = points.len();

        let tangents: Vec<Vec3> = points
            .iter()
            .enumerate()
            .map(|(point_index, _)| {
                match TangentSample::from_point_index(point_index, point_count) {
                    TangentSample::Only => Vec3::Y,
                    TangentSample::First => (points[1] - points[0]).normalize_or_zero(),
                    TangentSample::Last => {
                        (points[point_count - 1] - points[point_count - 2]).normalize_or_zero()
                    },
                    TangentSample::Interior => {
                        (points[point_index + 1] - points[point_index - 1]).normalize_or_zero()
                    },
                }
            })
            .collect();

        let mut cumulative = 0.0_f32;
        let arc_lengths: Vec<f32> = iter::once(0.0)
            .chain(points.windows(2).map(|pair| {
                cumulative += pair[0].distance(pair[1]);
                cumulative
            }))
            .collect();

        Self {
            points,
            tangents,
            arc_lengths,
            length: cumulative,
        }
    }
}

/// The complete geometry of a routed cable. This is the render-agnostic output
/// that bridges route computation and rendering.
#[derive(Clone, Debug, Default)]
pub struct CableGeometry {
    /// Curve segments between waypoints.
    pub segments:     Vec<CableSegment>,
    /// Total arc length across all segments.
    pub total_length: f32,
    /// Structural waypoints (start, intermediate bends, end).
    pub waypoints:    Vec<Vec3>,
}

impl CableGeometry {
    /// Build a `CableGeometry` from a list of segments and the waypoints that produced them.
    #[must_use]
    pub fn from_segments(segments: Vec<CableSegment>, waypoints: Vec<Vec3>) -> Self {
        let total_length = segments.iter().map(|s| s.length).sum();
        Self {
            segments,
            total_length,
            waypoints,
        }
    }

    /// Iterate over all sample points across all segments.
    pub fn all_points(&self) -> impl Iterator<Item = &Vec3> {
        self.segments.iter().flat_map(|s| &s.points)
    }
}
