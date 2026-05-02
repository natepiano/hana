use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy_kana::ToF32;

use super::convex_hull;
use super::labels;
use super::labels::BoundsLabel;
use super::labels::MarginLabel;
use super::labels::MarginLabelParams;
use super::screen_space;
use super::screen_space::MarginBalance;
use crate::components::CurrentFitTarget;
use crate::components::FitOverlay;
use crate::constants::TOLERANCE;
use crate::fit::Edge;
use crate::projection;
use crate::projection::CameraBasis;
use crate::projection::ProjectionMode;
use crate::projection::ScreenSpaceBounds;

/// Gizmo config group for fit target visualization (screen-aligned overlay).
/// Toggle by inserting/removing the `FitOverlay` component on the camera entity.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub(super) struct FitTargetGizmo;

/// Configuration for fit target visualization colors and appearance.
#[derive(Resource, Reflect, Debug, Clone)]
#[reflect(Resource)]
pub struct FitTargetOverlayConfig {
    /// Color for the screen-aligned bounding rectangle.
    pub rectangle_color:  Color,
    /// Color for the silhouette convex hull.
    pub silhouette_color: Color,
    /// Color for balanced margins (left ≈ right, top ≈ bottom).
    pub balanced_color:   Color,
    /// Color for unbalanced margins.
    pub unbalanced_color: Color,
    /// Line width for gizmo rendering.
    pub line_width:       f32,
}

impl Default for FitTargetOverlayConfig {
    fn default() -> Self {
        Self {
            rectangle_color:  Color::srgb(1.0, 1.0, 0.0), // Yellow
            silhouette_color: Color::srgb(1.0, 0.5, 0.0), // Orange
            balanced_color:   Color::srgb(0.0, 1.0, 0.0), // Green
            unbalanced_color: Color::srgb(1.0, 0.0, 0.0), // Red
            line_width:       2.0,
        }
    }
}

/// Current screen-space margin percentages for the fit target.
/// Updated every frame by the visualization system.
/// Removed when fit target visualization is disabled.
#[derive(Component, Reflect, Debug, Default, Clone)]
#[reflect(Component)]
pub(super) struct FitMarginPercents {
    /// Left margin as a percentage of screen width.
    pub left:   f32,
    /// Right margin as a percentage of screen width.
    pub right:  f32,
    /// Top margin as a percentage of screen height.
    pub top:    f32,
    /// Bottom margin as a percentage of screen height.
    pub bottom: f32,
}

impl From<&ScreenSpaceBounds> for FitMarginPercents {
    fn from(bounds: &ScreenSpaceBounds) -> Self {
        let screen_width = 2.0 * bounds.half_extent_x;
        let screen_height = 2.0 * bounds.half_extent_y;
        Self {
            left:   (bounds.left_margin / screen_width) * 100.0,
            right:  (bounds.right_margin / screen_width) * 100.0,
            top:    (bounds.top_margin / screen_height) * 100.0,
            bottom: (bounds.bottom_margin / screen_height) * 100.0,
        }
    }
}

/// Calculates the color for an edge based on balance state.
const fn calculate_edge_color(
    edge: Edge,
    horizontal_balance: MarginBalance,
    vertical_balance: MarginBalance,
    config: &FitTargetOverlayConfig,
) -> Color {
    let balance = match edge {
        Edge::Left | Edge::Right => horizontal_balance,
        Edge::Top | Edge::Bottom => vertical_balance,
    };
    match balance {
        MarginBalance::Balanced => config.balanced_color,
        MarginBalance::Unbalanced => config.unbalanced_color,
    }
}

/// Creates the 4 corners of the screen-aligned boundary rectangle in world space.
fn create_screen_corners(
    bounds: &ScreenSpaceBounds,
    camera: &CameraBasis,
    avg_depth: f32,
    projection_mode: ProjectionMode,
) -> [Vec3; 4] {
    [
        screen_space::normalized_to_world(
            bounds.min_norm_x,
            bounds.min_norm_y,
            camera,
            avg_depth,
            projection_mode,
        ),
        screen_space::normalized_to_world(
            bounds.max_norm_x,
            bounds.min_norm_y,
            camera,
            avg_depth,
            projection_mode,
        ),
        screen_space::normalized_to_world(
            bounds.max_norm_x,
            bounds.max_norm_y,
            camera,
            avg_depth,
            projection_mode,
        ),
        screen_space::normalized_to_world(
            bounds.min_norm_x,
            bounds.max_norm_y,
            camera,
            avg_depth,
            projection_mode,
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
    camera: &CameraBasis,
    avg_depth: f32,
    projection_mode: ProjectionMode,
    color: Color,
) {
    let projected = convex_hull::project_vertices_to_2d(vertices, camera, projection_mode);
    let hull = convex_hull::convex_hull_2d(&projected);

    if hull.len() < 2 {
        return;
    }

    for i in 0..hull.len() {
        let next = (i + 1) % hull.len();
        let start = screen_space::normalized_to_world(
            hull[i].0,
            hull[i].1,
            camera,
            avg_depth,
            projection_mode,
        );
        let end = screen_space::normalized_to_world(
            hull[next].0,
            hull[next].1,
            camera,
            avg_depth,
            projection_mode,
        );
        gizmos.line(start, end, color);
    }
}

/// Camera-derived drawing parameters shared across margin/bounds rendering.
struct DrawContext<'a> {
    camera:          Entity,
    bounds:          &'a ScreenSpaceBounds,
    camera_basis:    &'a CameraBasis,
    avg_depth:       f32,
    projection_mode: ProjectionMode,
    viewport_size:   Option<Vec2>,
}

/// Draws margin lines from boundary edges to screen edges and updates margin labels.
/// Returns the set of edges that had visible margins.
fn draw_margin_lines_and_labels(
    commands: &mut Commands,
    gizmos: &mut Gizmos<FitTargetGizmo>,
    label_query: &mut Query<(Entity, &MarginLabel, &mut Text, &mut Node, &mut TextColor)>,
    ctx: &DrawContext,
    config: &FitTargetOverlayConfig,
) -> Vec<Edge> {
    let camera = ctx.camera;
    let bounds = ctx.bounds;
    let camera_basis = ctx.camera_basis;
    let avg_depth = ctx.avg_depth;
    let projection_mode = ctx.projection_mode;
    let viewport_size = ctx.viewport_size;
    let horizontal_balance = screen_space::horizontal_balance(bounds, TOLERANCE);
    let vertical_balance = screen_space::vertical_balance(bounds, TOLERANCE);

    let mut visible_edges: Vec<Edge> = Vec::new();

    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        let Some((boundary_x, boundary_y)) = screen_space::boundary_edge_center(bounds, edge)
        else {
            continue;
        };
        visible_edges.push(edge);

        let (screen_x, screen_y) = screen_space::screen_edge_center(bounds, edge);
        let boundary_pos = screen_space::normalized_to_world(
            boundary_x,
            boundary_y,
            camera_basis,
            avg_depth,
            projection_mode,
        );
        let screen_pos = screen_space::normalized_to_world(
            screen_x,
            screen_y,
            camera_basis,
            avg_depth,
            projection_mode,
        );

        let color = calculate_edge_color(edge, horizontal_balance, vertical_balance, config);
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

/// Observer that cleans up visualization state when `FitVisualization` is removed from a camera.
pub(super) fn on_remove_fit_visualization(
    trigger: On<Remove, FitOverlay>,
    mut commands: Commands,
    label_query: Query<(Entity, &MarginLabel)>,
    bounds_label_query: Query<(Entity, &BoundsLabel)>,
) {
    let camera = trigger.entity;

    // Clean up viewport margins from the camera entity.
    // `try_remove` silently skips if the entity was despawned this frame
    // (e.g. closing a secondary window triggers component removal during despawn).
    commands.entity(camera).try_remove::<FitMarginPercents>();

    // Clean up labels belonging to this camera
    for (entity, label) in &label_query {
        if label.camera == camera {
            commands.entity(entity).despawn();
        }
    }
    for (entity, label) in &bounds_label_query {
        if label.camera == camera {
            commands.entity(entity).despawn();
        }
    }
}

/// Syncs the gizmo render layers and line width with visualization-enabled cameras.
pub(super) fn sync_gizmo_render_layers(
    mut config_store: ResMut<GizmoConfigStore>,
    viz_config: Res<FitTargetOverlayConfig>,
    camera_query: Query<Option<&RenderLayers>, With<FitOverlay>>,
) {
    let (gizmo_config, _) = config_store.config_mut::<FitTargetGizmo>();
    gizmo_config.line.width = viz_config.line_width;
    gizmo_config.depth_bias = -1.0;

    // Apply render layers from the first visualization-enabled camera
    if let Some(Some(layers)) = camera_query.iter().next() {
        gizmo_config.render_layers = layers.clone();
    }
}

/// Draws screen-aligned bounds for all cameras with `FitVisualization`.
pub(super) fn draw_fit_target_bounds(
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
    for (camera, camera_component, camera_global, projection, current_target) in &camera_query {
        let Some((vertices, _)) = projection::extract_mesh_vertices(
            current_target.0,
            &children_query,
            &mesh_query,
            &global_transform_query,
            &meshes,
        ) else {
            continue;
        };

        draw_bounds_for_camera(
            &mut commands,
            &mut gizmos,
            &config,
            &BoundsCamera {
                entity: camera,
                camera_component,
                camera_global,
                projection,
            },
            &vertices,
            &mut label_query,
            &mut bounds_label_query,
        );
    }
}

/// Camera data for bounds visualization.
struct BoundsCamera<'a> {
    entity:           Entity,
    camera_component: &'a Camera,
    camera_global:    &'a GlobalTransform,
    projection:       &'a Projection,
}

/// Draws bounds visualization for a single camera/target pair.
fn draw_bounds_for_camera(
    commands: &mut Commands,
    gizmos: &mut Gizmos<FitTargetGizmo>,
    config: &FitTargetOverlayConfig,
    camera_data: &BoundsCamera,
    vertices: &[Vec3],
    label_query: &mut Query<(Entity, &MarginLabel, &mut Text, &mut Node, &mut TextColor)>,
    bounds_label_query: &mut Query<(Entity, &BoundsLabel, &mut Node), Without<MarginLabel>>,
) {
    let camera = camera_data.entity;
    let camera_component = camera_data.camera_component;
    let camera_global = camera_data.camera_global;
    let projection = camera_data.projection;

    let camera_basis = CameraBasis::from(camera_global);

    let Some(aspect_ratio) =
        projection::projection_aspect_ratio(projection, camera_component.logical_viewport_size())
    else {
        return;
    };

    let Some((bounds, depths)) =
        ScreenSpaceBounds::from_points(vertices, camera_global, projection, aspect_ratio)
    else {
        return;
    };

    let avg_depth = depths.sum / depths.count.to_f32();
    let projection_mode = match projection {
        Projection::Perspective(_) => Some(ProjectionMode::Perspective),
        Projection::Orthographic(_) => Some(ProjectionMode::Orthographic),
        Projection::Custom(_) => None,
    };
    let Some(projection_mode) = projection_mode else {
        return;
    };
    let viewport_size = camera_component.logical_viewport_size();

    // Update margin percentages on camera entity for BRP inspection.
    // `try_insert` silently skips if the entity was despawned this frame
    // (e.g. closing a secondary window while visualization is active).
    commands
        .entity(camera)
        .try_insert(FitMarginPercents::from(&bounds));

    // Bounding rectangle
    let corners = create_screen_corners(&bounds, &camera_basis, avg_depth, projection_mode);
    draw_rectangle(gizmos, &corners, config);

    // Silhouette convex hull
    draw_silhouette(
        gizmos,
        vertices,
        &camera_basis,
        avg_depth,
        projection_mode,
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
            commands,
            bounds_label_query,
            camera,
            labels::bounds_label_position(upper_left),
        );
    }

    // Margin lines + labels
    let draw_ctx = DrawContext {
        camera,
        bounds: &bounds,
        camera_basis: &camera_basis,
        avg_depth,
        projection_mode,
        viewport_size,
    };
    let visible_edges =
        draw_margin_lines_and_labels(commands, gizmos, label_query, &draw_ctx, config);

    // Remove stale margin labels for this camera
    cleanup_stale_margin_labels(commands, label_query, camera, &visible_edges);
}
