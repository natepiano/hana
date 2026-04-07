use bevy::prelude::*;
use bevy_kana::ScreenPosition;

use crate::fit::Edge;
use crate::support::CameraBasis;
use crate::support::ScreenSpaceBounds;

/// Returns true if horizontal margins are balanced.
pub fn is_horizontally_balanced(bounds: &ScreenSpaceBounds, tolerance: f32) -> bool {
    (bounds.left_margin - bounds.right_margin).abs() < tolerance
}

/// Returns true if vertical margins are balanced.
pub fn is_vertically_balanced(bounds: &ScreenSpaceBounds, tolerance: f32) -> bool {
    (bounds.top_margin - bounds.bottom_margin).abs() < tolerance
}

/// Returns the screen edges in normalized space: (left, right, top, bottom).
fn screen_edges_normalized(bounds: &ScreenSpaceBounds) -> (f32, f32, f32, f32) {
    (
        -bounds.half_extent_x,
        bounds.half_extent_x,
        bounds.half_extent_y,
        -bounds.half_extent_y,
    )
}

/// Returns the clamped vertical center of the bounds within the screen edges.
const fn clamped_center_y(bounds: &ScreenSpaceBounds, bottom_edge: f32, top_edge: f32) -> f32 {
    bounds
        .min_norm_y
        .max(bottom_edge)
        .midpoint(bounds.max_norm_y.min(top_edge))
}

/// Returns the clamped horizontal center of the bounds within the screen edges.
const fn clamped_center_x(bounds: &ScreenSpaceBounds, left_edge: f32, right_edge: f32) -> f32 {
    bounds
        .min_norm_x
        .max(left_edge)
        .midpoint(bounds.max_norm_x.min(right_edge))
}

/// Returns the center of a boundary edge in normalized space.
pub fn boundary_edge_center(bounds: &ScreenSpaceBounds, edge: Edge) -> Option<(f32, f32)> {
    let (left_edge, right_edge, top_edge, bottom_edge) = screen_edges_normalized(bounds);

    match edge {
        Edge::Left if bounds.min_norm_x > left_edge => Some((
            bounds.min_norm_x,
            clamped_center_y(bounds, bottom_edge, top_edge),
        )),
        Edge::Right if bounds.max_norm_x < right_edge => Some((
            bounds.max_norm_x,
            clamped_center_y(bounds, bottom_edge, top_edge),
        )),
        Edge::Top if bounds.max_norm_y < top_edge => Some((
            clamped_center_x(bounds, left_edge, right_edge),
            bounds.max_norm_y,
        )),
        Edge::Bottom if bounds.min_norm_y > bottom_edge => Some((
            clamped_center_x(bounds, left_edge, right_edge),
            bounds.min_norm_y,
        )),
        _ => None,
    }
}

/// Returns the center of a screen edge in normalized space.
pub fn screen_edge_center(bounds: &ScreenSpaceBounds, edge: Edge) -> (f32, f32) {
    let (left_edge, right_edge, top_edge, bottom_edge) = screen_edges_normalized(bounds);

    match edge {
        Edge::Left => (left_edge, clamped_center_y(bounds, bottom_edge, top_edge)),
        Edge::Right => (right_edge, clamped_center_y(bounds, bottom_edge, top_edge)),
        Edge::Top => (clamped_center_x(bounds, left_edge, right_edge), top_edge),
        Edge::Bottom => (clamped_center_x(bounds, left_edge, right_edge), bottom_edge),
    }
}

/// Converts normalized screen-space coordinates to world space.
///
/// For perspective, reverses the perspective divide by multiplying by `avg_depth`.
/// For orthographic, coordinates are already in world units — `avg_depth` is only
/// used for the forward component to position the gizmo plane.
pub fn normalized_to_world(
    norm_x: f32,
    norm_y: f32,
    cam: &CameraBasis,
    avg_depth: f32,
    is_ortho: bool,
) -> Vec3 {
    let (world_x, world_y) = if is_ortho {
        (norm_x, norm_y)
    } else {
        (norm_x * avg_depth, norm_y * avg_depth)
    };
    *cam.pos + cam.right * world_x + cam.up * world_y + cam.forward * avg_depth
}

/// Returns the margin percentage for a given edge.
/// Percentage represents how much of the screen width/height is margin.
pub fn margin_percentage(bounds: &ScreenSpaceBounds, edge: Edge) -> f32 {
    let screen_width = 2.0 * bounds.half_extent_x;
    let screen_height = 2.0 * bounds.half_extent_y;

    match edge {
        Edge::Left => (bounds.left_margin / screen_width) * 100.0,
        Edge::Right => (bounds.right_margin / screen_width) * 100.0,
        Edge::Top => (bounds.top_margin / screen_height) * 100.0,
        Edge::Bottom => (bounds.bottom_margin / screen_height) * 100.0,
    }
}

/// Converts a normalized screen-space coordinate to viewport pixels.
pub fn norm_to_viewport(
    norm_x: f32,
    norm_y: f32,
    half_extent_x: f32,
    half_extent_y: f32,
    viewport_size: Vec2,
) -> ScreenPosition {
    ScreenPosition::new(
        (norm_x / half_extent_x + 1.0) * 0.5 * viewport_size.x,
        (1.0 - norm_y / half_extent_y) * 0.5 * viewport_size.y,
    )
}
