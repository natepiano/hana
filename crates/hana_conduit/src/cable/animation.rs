//! [`RouteAnimation`]: blended re-route transitions.
//!
//! When a cable with `RouteAnimation` re-solves while its endpoints are at
//! rest (an obstacle moved through the route), `animate_routes` blends the
//! displayed route from the old route to the new [`SolvedRoute`] over a fixed
//! duration instead of jumping, pushing in-flight points out of obstacle
//! boxes so the cable sweeps around them. A solve whose anchors moved (the
//! endpoint is being dragged) shows immediately: blending a moving endpoint
//! reads as lag and corrupts the end tangents the plugs align to. Lead
//! segments never blend: they are kept verbatim from the solved route so lead
//! direction and jack alignment stay exact throughout a transition.

use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use bevy_kana::ToF32;
use bevy_kana::ToUsize;

use super::Cable;
use super::RouteObstacle;
use super::compute::ComputedCableGeometry;
use super::constants::DEFAULT_ROUTE_ANIMATION_SECONDS;
use super::constants::ROUTE_ANIMATION_LEAD_MATCH_DISTANCE;
use super::constants::ROUTE_ANIMATION_OBSTACLE_CLEARANCE;
use super::constants::ROUTE_ANIMATION_SNAP_DISTANCE;
use super::route_obstacle::resolve_obstacles;
use crate::routing::Anchor;
use crate::routing::CableGeometry;
use crate::routing::CableSegment;
use crate::routing::MIN_CABLE_SAMPLE_POINTS;
use crate::routing::MIN_SEGMENT_LENGTH;
use crate::routing::Obstacle;
use crate::routing::push_out_of_obstacles;

/// Blends a cable's displayed route from the old route to each newly solved
/// route instead of jumping. Only re-solves with endpoints at rest animate —
/// the first solve and any solve whose anchors moved (the endpoint is being
/// dragged) show immediately, so the cable always tracks the pointer exactly.
/// A transition runs over [`RouteAnimation::seconds`] with an ease-out curve,
/// pushing in-flight points out of obstacle boxes so it sweeps around
/// obstacles rather than through them, and always lands on the solver's
/// geometry — sharp route bends included — when the duration elapses.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component)]
pub struct RouteAnimation {
    /// Seconds a transition takes to land on the newly solved route.
    pub seconds: f32,
}

impl Default for RouteAnimation {
    fn default() -> Self {
        Self {
            seconds: DEFAULT_ROUTE_ANIMATION_SECONDS,
        }
    }
}

/// Latest solver output for an animated cable, with the anchors it was solved
/// from. `recompute_dirty_cables` writes this instead of
/// `ComputedCableGeometry` when the cable has a [`RouteAnimation`];
/// `animate_routes` blends the displayed geometry toward it. The anchors let
/// the animation recognize the geometry's lead segments and keep them out of
/// the blend.
#[derive(Component)]
pub(super) struct SolvedRoute {
    pub(super) geometry: CableGeometry,
    pub(super) start:    Anchor,
    pub(super) end:      Anchor,
}

/// Blend state for an animated cable's routed span (the polyline between lead
/// tips). `current` equals `target` once the transition has landed.
#[derive(Component)]
pub(super) struct DisplayedRoute {
    /// Start anchor of the solve `target` came from. A solve whose anchors
    /// differ is endpoint-driven and snaps instead of blending.
    start:   Anchor,
    /// End anchor of the solve `target` came from.
    end:     Anchor,
    /// Span polyline the active transition blends from.
    source:  Vec<Vec3>,
    /// Span polyline of the solved route the transition lands on.
    target:  Vec<Vec3>,
    /// Span polyline currently shown.
    current: Vec<Vec3>,
    /// Seconds since the active transition began.
    elapsed: f32,
}

impl DisplayedRoute {
    /// State landed on `solved_route`: no transition in flight.
    fn landed_on(solved_route: &SolvedRoute, span: Vec<Vec3>) -> Self {
        Self {
            start:   solved_route.start,
            end:     solved_route.end,
            source:  span.clone(),
            target:  span.clone(),
            current: span,
            elapsed: 0.0,
        }
    }
}

/// Outcome of one blend step toward the solved route.
enum Blend {
    /// Every sample is within [`ROUTE_ANIMATION_SNAP_DISTANCE`] of its goal;
    /// snap to the solved geometry.
    Converged,
    /// Still in transit toward the solved route.
    InFlight {
        /// The pure source→target interpolation, persisted as the next
        /// frame's span so obstacle push-out never feeds back into the blend
        /// and accumulates as jitter.
        blended:   Vec<Vec3>,
        /// `blended` with interior samples pushed out of obstacle boxes — the
        /// span polyline actually shown this frame.
        displayed: Vec<Vec3>,
    },
}

/// The solved route split into its lead segments (kept verbatim during a
/// transition) and the routed span polyline between the lead tips (the part
/// that blends).
struct RouteParts<'a> {
    start_lead: Option<&'a CableSegment>,
    end_lead:   Option<&'a CableSegment>,
    span:       Vec<Vec3>,
}

/// Blends each [`DisplayedRoute`] toward its cable's [`SolvedRoute`], writing
/// the in-flight geometry (or, once the transition lands, the solved geometry
/// itself) to `ComputedCableGeometry`.
pub(super) fn animate_routes(
    time: Res<Time>,
    mut cables: Query<(
        Entity,
        &Cable,
        &RouteAnimation,
        &SolvedRoute,
        Option<&mut DisplayedRoute>,
    )>,
    route_obstacles: Query<(Entity, &RouteObstacle, &GlobalTransform)>,
    children: Query<&Children>,
    aabbs: Query<&Aabb>,
    transforms: Query<&GlobalTransform>,
    mut commands: Commands,
) {
    // Resolved only when some cable is mid-transition this frame.
    let mut world_obstacles: Option<Vec<Obstacle>> = None;

    for (cable_entity, cable, route_animation, solved_route, displayed_route) in &mut cables {
        let route_parts = split_route(solved_route);

        // First solve for this cable: show it as-is. Animation applies to
        // re-routes only, so an initial drag-out never lags the pointer.
        let Some(mut displayed_route) = displayed_route else {
            commands.entity(cable_entity).insert((
                DisplayedRoute::landed_on(solved_route, route_parts.span),
                ComputedCableGeometry {
                    cable_geometry: Some(solved_route.geometry.clone()),
                },
            ));
            continue;
        };

        // An anchor moved: the endpoint is being dragged, and the route
        // tracks it exactly. Blending a moving endpoint reads as lag and
        // corrupts the end tangents the plugs align to; animation is for
        // re-routes whose endpoints are at rest.
        if displayed_route.start != solved_route.start || displayed_route.end != solved_route.end {
            *displayed_route = DisplayedRoute::landed_on(solved_route, route_parts.span);
            commands.entity(cable_entity).insert(ComputedCableGeometry {
                cable_geometry: Some(solved_route.geometry.clone()),
            });
            continue;
        }

        // A re-solve landed since last frame: restart the transition from
        // wherever the displayed span is now.
        if displayed_route.target != route_parts.span {
            displayed_route.source = displayed_route.current.clone();
            displayed_route.target = route_parts.span.clone();
            displayed_route.elapsed = 0.0;
        }

        if displayed_route.current == displayed_route.target {
            continue;
        }

        displayed_route.elapsed += time.delta_secs();
        let progress = displayed_route.elapsed / route_animation.seconds.max(f32::EPSILON);

        // Progress reaching 1 lands the transition unconditionally — a sample
        // parked against an obstacle face can never stall the animation. A
        // collapsed source span (the drag just left the jack) has no usable
        // blend source and lands immediately.
        if progress >= 1.0 || polyline_length(&displayed_route.source) < MIN_SEGMENT_LENGTH {
            displayed_route.current = displayed_route.target.clone();
            commands.entity(cable_entity).insert(ComputedCableGeometry {
                cable_geometry: Some(solved_route.geometry.clone()),
            });
            continue;
        }

        let world = world_obstacles.get_or_insert_with(|| {
            resolve_obstacles(&route_obstacles, &children, &aabbs, &transforms)
        });
        let obstacles: Vec<Obstacle> = cable
            .obstacles
            .iter()
            .copied()
            .chain(world.iter().copied())
            .collect();

        match blend_span(
            &displayed_route.source,
            &displayed_route.target,
            ease_out_cubic(progress),
            &obstacles,
        ) {
            Blend::Converged => {
                displayed_route.current = displayed_route.target.clone();
                commands.entity(cable_entity).insert(ComputedCableGeometry {
                    cable_geometry: Some(solved_route.geometry.clone()),
                });
            },
            Blend::InFlight { blended, displayed } => {
                displayed_route.current = blended;
                commands.entity(cable_entity).insert(ComputedCableGeometry {
                    cable_geometry: Some(in_flight_geometry(displayed, &route_parts, solved_route)),
                });
            },
        }
    }
}

/// Split the solved geometry into lead segments and the routed span polyline.
/// Leads are recognized by matching the first/last segment's endpoints against
/// the anchor position and lead tip that `wrap_with_leads` built them from.
fn split_route(solved_route: &SolvedRoute) -> RouteParts<'_> {
    let segments = &solved_route.geometry.segments;
    let multi_segment = segments.len() >= 2;
    let start_lead = segments.first().filter(|segment| {
        multi_segment
            && solved_route
                .start
                .lead_tip()
                .is_some_and(|tip| is_lead(segment, solved_route.start.position, tip))
    });
    let end_lead = segments.last().filter(|segment| {
        multi_segment
            && solved_route
                .end
                .lead_tip()
                .is_some_and(|tip| is_lead(segment, tip, solved_route.end.position))
    });

    let span_segments = &segments
        [usize::from(start_lead.is_some())..segments.len() - usize::from(end_lead.is_some())];
    let mut span: Vec<Vec3> = span_segments
        .iter()
        .flat_map(|segment| &segment.points)
        .copied()
        .collect();
    span.dedup();

    // No routed span left between the leads: blend the whole polyline instead.
    if span.len() < 2 {
        return RouteParts {
            start_lead: None,
            end_lead:   None,
            span:       polyline(&solved_route.geometry),
        };
    }
    RouteParts {
        start_lead,
        end_lead,
        span,
    }
}

/// Whether `segment` is the straight lead running `from` → `to` that the
/// solver's lead wrapping added around the routed span.
fn is_lead(segment: &CableSegment, from: Vec3, to: Vec3) -> bool {
    let (Some(&first), Some(&last)) = (segment.points.first(), segment.points.last()) else {
        return false;
    };
    first.distance_squared(from)
        < ROUTE_ANIMATION_LEAD_MATCH_DISTANCE * ROUTE_ANIMATION_LEAD_MATCH_DISTANCE
        && last.distance_squared(to)
            < ROUTE_ANIMATION_LEAD_MATCH_DISTANCE * ROUTE_ANIMATION_LEAD_MATCH_DISTANCE
}

/// One blend step: resample both span polylines to a common count, move each
/// sample `ease` of the way from source to goal, and pin the endpoints. The
/// displayed copy additionally pushes interior samples out of obstacle boxes.
fn blend_span(source: &[Vec3], target: &[Vec3], ease: f32, obstacles: &[Obstacle]) -> Blend {
    let sample_count = target.len().max(MIN_CABLE_SAMPLE_POINTS.to_usize());
    let target_samples = resample_polyline(target, sample_count);
    let source_samples = resample_polyline(source, sample_count);
    let last = sample_count - 1;

    let blended: Vec<Vec3> = source_samples
        .iter()
        .zip(&target_samples)
        .enumerate()
        .map(|(index, (from, goal))| {
            if index == 0 || index == last {
                *goal
            } else {
                from.lerp(*goal, ease)
            }
        })
        .collect();
    let displayed: Vec<Vec3> = blended
        .iter()
        .enumerate()
        .map(|(index, &point)| {
            if index == 0 || index == last {
                point
            } else {
                push_out_of_obstacles(point, obstacles, ROUTE_ANIMATION_OBSTACLE_CLEARANCE)
            }
        })
        .collect();

    let converged = displayed.iter().zip(&target_samples).all(|(point, goal)| {
        point.distance_squared(*goal)
            < ROUTE_ANIMATION_SNAP_DISTANCE * ROUTE_ANIMATION_SNAP_DISTANCE
    });
    if converged {
        Blend::Converged
    } else {
        Blend::InFlight { blended, displayed }
    }
}

/// Ease-out cubic: fast initial motion that decelerates into the target and
/// reaches it exactly at `progress == 1`.
fn ease_out_cubic(progress: f32) -> f32 {
    let remaining = 1.0 - progress.clamp(0.0, 1.0);
    1.0 - remaining.powi(3)
}

/// Geometry for an in-transit route: the solved route's lead segments verbatim
/// (endpoint alignment reads its end tangents from them) around one blended
/// span segment, with waypoints mirroring the solver's lead wrapping.
fn in_flight_geometry(
    span_points: Vec<Vec3>,
    route_parts: &RouteParts,
    solved_route: &SolvedRoute,
) -> CableGeometry {
    let mut segments = Vec::with_capacity(3);
    let mut waypoints = Vec::with_capacity(4);
    if let Some(lead) = route_parts.start_lead {
        segments.push(lead.clone());
        waypoints.push(solved_route.start.position);
    }
    waypoints.extend(span_points.first().copied());
    waypoints.extend(span_points.last().copied());
    segments.push(CableSegment::from(span_points));
    if let Some(lead) = route_parts.end_lead {
        segments.push(lead.clone());
        waypoints.push(solved_route.end.position);
    }
    CableGeometry::from_segments(segments, waypoints)
}

/// A [`CableGeometry`]'s sample points as one polyline, with the duplicate
/// points at segment junctions removed.
fn polyline(cable_geometry: &CableGeometry) -> Vec<Vec3> {
    let mut points: Vec<Vec3> = cable_geometry.all_points().copied().collect();
    points.dedup();
    points
}

/// Total arc length of a polyline.
fn polyline_length(points: &[Vec3]) -> f32 {
    points
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .sum()
}

/// Resample a polyline to `sample_count` points evenly spaced by arc length.
fn resample_polyline(points: &[Vec3], sample_count: usize) -> Vec<Vec3> {
    let Some(&first) = points.first() else {
        return Vec::new();
    };

    let mut total = 0.0_f32;
    let mut cumulative = Vec::with_capacity(points.len());
    cumulative.push(total);
    for pair in points.windows(2) {
        total += pair[0].distance(pair[1]);
        cumulative.push(total);
    }
    if total <= f32::EPSILON {
        return vec![first; sample_count];
    }

    let mut segment = 0;
    (0..sample_count)
        .map(|sample| {
            let goal = sample.to_f32() / (sample_count - 1).to_f32() * total;
            while segment + 2 < cumulative.len() && cumulative[segment + 1] < goal {
                segment += 1;
            }
            let span = cumulative[segment + 1] - cumulative[segment];
            let t = if span <= f32::EPSILON {
                0.0
            } else {
                (goal - cumulative[segment]) / span
            };
            points[segment].lerp(points[segment + 1], t)
        })
        .collect()
}
