//! Constants for the cable module: endpoint alignment and route animation.

// alignment
/// Dot-product threshold above which `on_endpoint_alignment_update` skips
/// writing back to `Transform`. Prevents an infinite recompute cycle of
/// `ComputedCableGeometry` -> `Transform` -> `GlobalTransform`.
pub(super) const ALIGNMENT_FEEDBACK_GUARD: f32 = 0.9999;

// route animation
/// Default seconds a `RouteAnimation` transition takes to land on the newly
/// solved route.
pub(super) const DEFAULT_ROUTE_ANIMATION_SECONDS: f32 = 0.4;
/// Distance within which a solved geometry's first/last segment endpoints
/// must match an anchor's position and lead tip to count as a lead segment,
/// in metres.
pub(super) const ROUTE_ANIMATION_LEAD_MATCH_DISTANCE: f32 = 1e-4;
/// Distance an in-flight route sample lands outside an obstacle face when
/// `push_out_of_obstacles` ejects it, in metres.
pub(super) const ROUTE_ANIMATION_OBSTACLE_CLEARANCE: f32 = 0.01;
/// Once every in-flight sample is within this distance of its goal on the
/// solved route, the animation snaps to the solved geometry, in metres.
pub(super) const ROUTE_ANIMATION_SNAP_DISTANCE: f32 = 0.002;
