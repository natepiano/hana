use bevy::prelude::*;

use super::super::components::CurrentFitTarget;
use super::super::components::FitOverlay;
use super::super::fit;
use super::super::fit::Edge;
use super::super::support;
use super::super::support::CameraBasis;
use super::super::support::ScreenSpaceBounds;
use super::convex_hull;
use super::labels;
use super::labels::BoundsLabel;
use super::labels::MarginLabel;
use super::labels::MarginLabelParams;
use super::screen_space;
use super::types::FitTargetGizmo;
use super::types::FitTargetOverlayConfig;
use super::types::FitTargetViewportMargins;

/// Calculates the color for an edge based on balance state.
const fn calculate_edge_color(
    edge: Edge,
    h_balanced: bool,
    v_balanced: bool,
    config: &FitTargetOverlayConfig,
) -> Color {
    match edge {
        Edge::Left | Edge::Right => {
            if h_balanced {
                config.balanced_color
            } else {
                config.unbalanced_color
            }
        },
        Edge::Top | Edge::Bottom => {
            if v_balanced {
                config.balanced_color
            } else {
                config.unbalanced_color
            }
        },
    }
}

/// Creates the 4 corners of the screen-aligned boundary rectangle in world space.
fn create_screen_corners(
    bounds: &ScreenSpaceBounds,
    cam: &CameraBasis,
    avg_depth: f32,
    is_ortho: bool,
) -> [Vec3; 4] {
    [
        screen_space::normalized_to_world(
            bounds.min_norm_x,
            bounds.min_norm_y,
            cam,
            avg_depth,
            is_ortho,
        ),
        screen_space::normalized_to_world(
            bounds.max_norm_x,
            bounds.min_norm_y,
            cam,
            avg_depth,
            is_ortho,
        ),
        screen_space::normalized_to_world(
            bounds.max_norm_x,
            bounds.max_norm_y,
            cam,
            avg_depth,
            is_ortho,
        ),
        screen_space::normalized_to_world(
            bounds.min_norm_x,
            bounds.max_norm_y,
            cam,
            avg_depth,
            is_ortho,
        ),
    ]
}

/// Draws the boundary rectangle outline.
fn draw_rectangle(
    gizmos: &mut Gizmos<FitTargetGizmo>,
    corners: &[Vec3; 4],
    config: &FitTargetOverlayConfig,
) {
    for i in 0..4 {
        let next = (i + 1) % 4;
        gizmos.line(corners[i], corners[next], config.rectangle_color);
    }
}

/// Draws the silhouette polygon (convex hull of projected vertices) using gizmo lines.
fn draw_silhouette(
    gizmos: &mut Gizmos<FitTargetGizmo>,
    vertices: &[Vec3],
    cam: &CameraBasis,
    avg_depth: f32,
    is_ortho: bool,
    color: Color,
) {
    let projected = convex_hull::project_vertices_to_2d(vertices, cam, is_ortho);
    let hull = convex_hull::convex_hull_2d(&projected);

    if hull.len() < 2 {
        return;
    }

    for i in 0..hull.len() {
        let next = (i + 1) % hull.len();
        let start =
            screen_space::normalized_to_world(hull[i].0, hull[i].1, cam, avg_depth, is_ortho);
        let end =
            screen_space::normalized_to_world(hull[next].0, hull[next].1, cam, avg_depth, is_ortho);
        gizmos.line(start, end, color);
    }
}

/// Draws margin lines from boundary edges to screen edges and updates margin labels.
/// Returns the set of edges that had visible margins.
fn draw_margin_lines_and_labels(
    commands: &mut Commands,
    gizmos: &mut Gizmos<FitTargetGizmo>,
    label_query: &mut Query<(Entity, &MarginLabel, &mut Text, &mut Node, &mut TextColor)>,
    camera: Entity,
    bounds: &ScreenSpaceBounds,
    cam_basis: &CameraBasis,
    avg_depth: f32,
    is_ortho: bool,
    config: &FitTargetOverlayConfig,
    viewport_size: Option<Vec2>,
) -> Vec<Edge> {
    let h_balanced = screen_space::is_horizontally_balanced(bounds, fit::TOLERANCE);
    let v_balanced = screen_space::is_vertically_balanced(bounds, fit::TOLERANCE);

    let mut visible_edges: Vec<Edge> = Vec::new();

    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        let Some((boundary_x, boundary_y)) = screen_space::boundary_edge_center(bounds, edge)
        else {
            continue;
        };
        visible_edges.push(edge);

        let (screen_x, screen_y) = screen_space::screen_edge_center(bounds, edge);
        let boundary_pos = screen_space::normalized_to_world(
            boundary_x, boundary_y, cam_basis, avg_depth, is_ortho,
        );
        let screen_pos =
            screen_space::normalized_to_world(screen_x, screen_y, cam_basis, avg_depth, is_ortho);

        let color = calculate_edge_color(edge, h_balanced, v_balanced, config);
        gizmos.line(boundary_pos, screen_pos, color);

        let Some(vp) = viewport_size else {
            continue;
        };
        let percentage = screen_space::margin_percentage(bounds, edge);
        let text = format!("margin: {percentage:.3}%");
        let label_screen_pos = labels::calculate_label_pixel_position(edge, bounds, vp);

        labels::update_or_create_margin_label(
            commands,
            label_query,
            MarginLabelParams {
                camera,
                edge,
                text,
                color,
                screen_pos: label_screen_pos,
                viewport_size: vp,
            },
        );
    }

    visible_edges
}

/// Removes margin labels for edges no longer visible, scoped to a specific camera.
fn cleanup_stale_margin_labels(
    commands: &mut Commands,
    label_query: &Query<(Entity, &MarginLabel, &mut Text, &mut Node, &mut TextColor)>,
    camera: Entity,
    visible_edges: &[Edge],
) {
    for (entity, label, _, _, _) in label_query {
        if label.camera == camera && !visible_edges.contains(&label.edge) {
            commands.entity(entity).despawn();
        }
    }
}

/// Draws screen-aligned bounds for all cameras with `FitVisualization`.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::type_complexity
)]
pub fn draw_fit_target_bounds(
    mut commands: Commands,
    mut gizmos: Gizmos<FitTargetGizmo>,
    config: Res<FitTargetOverlayConfig>,
    camera_query: Query<
        (
            Entity,
            &Camera,
            &GlobalTransform,
            &Projection,
            &CurrentFitTarget,
        ),
        With<FitOverlay>,
    >,
    mesh_query: Query<&Mesh3d>,
    children_query: Query<&Children>,
    global_transform_query: Query<&GlobalTransform>,
    meshes: Res<Assets<Mesh>>,
    mut label_query: Query<(Entity, &MarginLabel, &mut Text, &mut Node, &mut TextColor)>,
    mut bounds_label_query: Query<(Entity, &BoundsLabel, &mut Node), Without<MarginLabel>>,
) {
    for (camera, cam, cam_global, projection, current_target) in &camera_query {
        let Some((vertices, _)) = support::extract_mesh_vertices(
            current_target.0,
            &children_query,
            &mesh_query,
            &global_transform_query,
            &meshes,
        ) else {
            continue;
        };

        let cam_basis = CameraBasis::from_global_transform(cam_global);

        let Some(aspect_ratio) =
            support::projection_aspect_ratio(projection, cam.logical_viewport_size())
        else {
            continue;
        };

        let Some((bounds, depths)) =
            ScreenSpaceBounds::from_points(&vertices, cam_global, projection, aspect_ratio)
        else {
            continue;
        };

        #[allow(clippy::cast_precision_loss)]
        let avg_depth = depths.depth_sum / depths.point_count as f32;
        let is_ortho = matches!(projection, Projection::Orthographic(_));
        let viewport_size = cam.logical_viewport_size();

        // Update margin percentages on camera entity for BRP inspection.
        // `try_insert` silently skips if the entity was despawned this frame
        // (e.g. closing a secondary window while visualization is active).
        commands
            .entity(camera)
            .try_insert(FitTargetViewportMargins::from_bounds(&bounds));

        // Bounding rectangle
        let corners = create_screen_corners(&bounds, &cam_basis, avg_depth, is_ortho);
        draw_rectangle(&mut gizmos, &corners, &config);

        // Silhouette convex hull
        draw_silhouette(
            &mut gizmos,
            &vertices,
            &cam_basis,
            avg_depth,
            is_ortho,
            config.silhouette_color,
        );

        // "Screen space bounds" label
        if let Some(vp) = viewport_size {
            let upper_left = screen_space::norm_to_viewport(
                bounds.min_norm_x,
                bounds.max_norm_y,
                bounds.half_extent_x,
                bounds.half_extent_y,
                vp,
            );
            labels::update_or_create_bounds_label(
                &mut commands,
                &mut bounds_label_query,
                camera,
                labels::bounds_label_position(upper_left),
            );
        }

        // Margin lines + labels
        let visible_edges = draw_margin_lines_and_labels(
            &mut commands,
            &mut gizmos,
            &mut label_query,
            camera,
            &bounds,
            &cam_basis,
            avg_depth,
            is_ortho,
            &config,
            viewport_size,
        );

        // Remove stale margin labels for this camera
        cleanup_stale_margin_labels(&mut commands, &label_query, camera, &visible_edges);
    }
}
