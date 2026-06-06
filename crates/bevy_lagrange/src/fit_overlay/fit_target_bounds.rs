use bevy::camera::NormalizedRenderTarget;
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::ui::UiTargetCamera;
use bevy::window::PrimaryWindow;

use super::constants::DEFAULT_OVERLAY_LINE_WIDTH;
use super::constants::OVERLAY_BALANCED_COLOR;
use super::constants::OVERLAY_RECTANGLE_COLOR;
use super::constants::OVERLAY_SILHOUETTE_COLOR;
use super::constants::OVERLAY_UNBALANCED_COLOR;
use super::constants::PERCENT_MULTIPLIER;
use super::context::FitOverlayCameraContext;
use super::context::FitOverlayEmptyReason;
use super::convex_hull;
use super::frame::FitOverlayFrame;
use super::frame::FitOverlayLayout;
use super::labels;
use super::labels::BoundsLabel;
use super::labels::MarginLabel;
use super::labels::MarginLabelParameters;
use super::lines;
use super::lines::FitOverlayLineContext;
use super::lines::FitOverlayLineMaterial;
use super::lines::FitOverlayLineMaterials;
use super::lines::FitOverlayLineQueryItem;
use super::lines::FitOverlayLineVisual;
use super::reconciliation;
use super::screen_space;
use super::screen_space::MarginBalance;
use super::visual::FitOverlayVisual;
use super::visual::FitOverlayVisualKind;
use crate::components::CurrentFitTarget;
use crate::components::FitOverlay;
use crate::constants::TOLERANCE;
use crate::fit::Edge;
use crate::projection;
use crate::projection::ScreenSpaceBounds;

/// Configuration for fit target overlay colors and line appearance.
///
/// This resource controls visual style only. It does not select render layers,
/// choose a camera, or override `Camera::order`. Configure visibility on the
/// camera that carries `FitOverlay`: generated retained line visuals copy that
/// camera's effective `RenderLayers` and render in normal Bevy camera passes
/// when camera and visual layers intersect. Overlay labels are Bevy UI nodes
/// targeted through `UiTargetCamera`.
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
    /// Line width for retained overlay line meshes, in viewport pixels.
    pub line_width:       f32,
}

impl Default for FitTargetOverlayConfig {
    fn default() -> Self {
        Self {
            rectangle_color:  OVERLAY_RECTANGLE_COLOR,
            silhouette_color: OVERLAY_SILHOUETTE_COLOR,
            balanced_color:   OVERLAY_BALANCED_COLOR,
            unbalanced_color: OVERLAY_UNBALANCED_COLOR,
            line_width:       DEFAULT_OVERLAY_LINE_WIDTH,
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
            left:   (bounds.left_margin / screen_width) * PERCENT_MULTIPLIER,
            right:  (bounds.right_margin / screen_width) * PERCENT_MULTIPLIER,
            top:    (bounds.top_margin / screen_height) * PERCENT_MULTIPLIER,
            bottom: (bounds.bottom_margin / screen_height) * PERCENT_MULTIPLIER,
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

/// Creates the 4 corners of the screen-aligned boundary rectangle in normalized space.
const fn rectangle_points(bounds: &ScreenSpaceBounds) -> [Vec2; 4] {
    [
        Vec2::new(bounds.min_normalized_x, bounds.min_normalized_y),
        Vec2::new(bounds.max_normalized_x, bounds.min_normalized_y),
        Vec2::new(bounds.max_normalized_x, bounds.max_normalized_y),
        Vec2::new(bounds.min_normalized_x, bounds.max_normalized_y),
    ]
}

fn silhouette_points(layout: &FitOverlayLayout) -> Vec<Vec2> {
    let projected = convex_hull::project_vertices_to_2d(
        &layout.vertices,
        &layout.camera_basis,
        layout.projection_mode,
    );
    convex_hull::convex_hull_2d(&projected)
        .into_iter()
        .map(|(x, y)| Vec2::new(x, y))
        .collect()
}

/// Camera-derived drawing parameters shared across margin/bounds rendering.
struct DrawContext<'a> {
    camera:        Entity,
    ui_camera:     Entity,
    bounds:        &'a ScreenSpaceBounds,
    viewport_size: Option<Vec2>,
}

/// Draws margin lines from boundary edges to screen edges and updates margin labels.
/// Returns the set of edges that had visible margins.
fn draw_margin_lines_and_labels(
    line_context: &mut FitOverlayLineContext<'_, '_, '_>,
    label_query: &mut Query<(
        Entity,
        &MarginLabel,
        &FitOverlayVisual,
        &mut Text,
        &mut Node,
        &mut TextColor,
        &mut UiTargetCamera,
    )>,
    draw_context: &DrawContext,
    config: &FitTargetOverlayConfig,
) -> Vec<Edge> {
    let camera = draw_context.camera;
    let bounds = draw_context.bounds;
    let viewport_size = draw_context.viewport_size;
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
        let color = calculate_edge_color(edge, horizontal_balance, vertical_balance, config);
        line_context.upsert_polyline(
            FitOverlayVisual {
                camera,
                kind: FitOverlayVisualKind::MarginLine { edge },
            },
            &[
                Vec2::new(boundary_x, boundary_y),
                Vec2::new(screen_x, screen_y),
            ],
            false,
            color,
            config.line_width,
        );

        let Some(viewport_size) = viewport_size else {
            continue;
        };
        let percentage = screen_space::margin_percentage(bounds, edge);
        let text = format!("margin: {percentage:.3}%");
        let label_screen_position =
            labels::calculate_label_pixel_position(edge, bounds, viewport_size);

        labels::update_or_create_margin_label(
            line_context.commands,
            label_query,
            MarginLabelParameters {
                camera,
                ui_camera: draw_context.ui_camera,
                edge,
                text,
                color,
                screen_position: label_screen_position,
                viewport_size,
            },
        );
    }

    visible_edges
}

/// Removes margin labels for edges no longer visible, scoped to a specific camera.
fn cleanup_stale_margin_labels(
    commands: &mut Commands,
    label_query: &Query<(
        Entity,
        &MarginLabel,
        &FitOverlayVisual,
        &mut Text,
        &mut Node,
        &mut TextColor,
        &mut UiTargetCamera,
    )>,
    camera: Entity,
    visible_edges: &[Edge],
) {
    for (entity, _, visual, _, _, _, _) in label_query {
        let FitOverlayVisualKind::MarginLabel { edge } = visual.kind else {
            continue;
        };

        if visual.camera == camera && !visible_edges.contains(&edge) {
            commands.entity(entity).despawn();
        }
    }
}

/// Observer that cleans up overlay state when `FitOverlay` is removed from a camera.
pub(super) fn on_remove_fit_visualization(
    trigger: On<Remove, FitOverlay>,
    mut commands: Commands,
    visual_query: Query<(Entity, &FitOverlayVisual)>,
) {
    let camera = trigger.entity;
    reconciliation::clear_camera_visuals(&mut commands, camera, &visual_query);
}

/// Draws screen-aligned bounds for all cameras with `FitOverlay`.
pub(super) fn draw_fit_target_bounds(
    mut commands: Commands,
    config: Res<FitTargetOverlayConfig>,
    mut line_materials: ResMut<FitOverlayLineMaterials>,
    camera_query: Query<
        (
            Entity,
            &Camera,
            &RenderTarget,
            Option<&RenderLayers>,
            &GlobalTransform,
            &Projection,
            Option<&CurrentFitTarget>,
        ),
        With<FitOverlay>,
    >,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    all_cameras: Query<(
        Entity,
        &Camera,
        &RenderTarget,
        Option<&Camera2d>,
        Option<&Camera3d>,
    )>,
    mesh_query: Query<&Mesh3d>,
    children_query: Query<&Children>,
    global_transform_query: Query<&GlobalTransform>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FitOverlayLineMaterial>>,
    mut label_query: Query<(
        Entity,
        &MarginLabel,
        &FitOverlayVisual,
        &mut Text,
        &mut Node,
        &mut TextColor,
        &mut UiTargetCamera,
    )>,
    visual_query: Query<(Entity, &FitOverlayVisual)>,
    line_query: Query<FitOverlayLineQueryItem, With<FitOverlayLineVisual>>,
    line_cleanup_query: Query<(Entity, &FitOverlayVisual), With<FitOverlayLineVisual>>,
    mut bounds_label_query: Query<
        (
            Entity,
            &BoundsLabel,
            &FitOverlayVisual,
            &mut Node,
            &mut UiTargetCamera,
        ),
        Without<MarginLabel>,
    >,
) {
    let primary_window = primary_window.single().ok();
    for (
        camera,
        camera_component,
        render_target,
        render_layers,
        camera_global,
        projection,
        current_target,
    ) in &camera_query
    {
        let frame = resolve_fit_overlay_frame(
            camera,
            camera_component,
            render_target,
            render_layers,
            primary_window,
            camera_global,
            projection,
            current_target,
            &mesh_query,
            &children_query,
            &global_transform_query,
            &meshes,
        );

        match frame {
            FitOverlayFrame::Visible(layout) => {
                let line_index = lines::retained_line_entities(&line_query);
                let mut line_context = FitOverlayLineContext {
                    commands:       &mut commands,
                    meshes:         &mut meshes,
                    materials:      &mut materials,
                    material_cache: &mut line_materials,
                    line_index:     &line_index,
                    layout:         &layout,
                };
                draw_bounds_for_camera(
                    &mut line_context,
                    &config,
                    &layout,
                    label_ui_camera(
                        layout.context.camera,
                        &layout.context.normalized_target,
                        primary_window,
                        all_cameras.iter(),
                    ),
                    &mut label_query,
                    &mut bounds_label_query,
                    &line_cleanup_query,
                );
            },
            FitOverlayFrame::Empty(reason) => {
                reconciliation::clear_empty_frame(&mut commands, camera, reason, &visual_query);
            },
        }
    }
}

fn resolve_fit_overlay_frame(
    camera: Entity,
    camera_component: &Camera,
    render_target: &RenderTarget,
    render_layers: Option<&RenderLayers>,
    primary_window: Option<Entity>,
    camera_global: &GlobalTransform,
    projection: &Projection,
    current_target: Option<&CurrentFitTarget>,
    mesh_query: &Query<&Mesh3d>,
    children_query: &Query<&Children>,
    global_transform_query: &Query<&GlobalTransform>,
    meshes: &Assets<Mesh>,
) -> FitOverlayFrame {
    let context = match FitOverlayCameraContext::resolve(
        camera,
        camera_component,
        render_target,
        render_layers,
        primary_window,
    ) {
        Ok(context) => context,
        Err(reason) => return FitOverlayFrame::Empty(reason),
    };

    let Some(current_target) = current_target else {
        return FitOverlayFrame::Empty(FitOverlayEmptyReason::MissingCurrentFitTarget);
    };

    let Some((vertices, _)) = projection::extract_mesh_vertices(
        current_target.0,
        children_query,
        mesh_query,
        global_transform_query,
        meshes,
    ) else {
        return FitOverlayFrame::Empty(FitOverlayEmptyReason::MissingMesh);
    };

    FitOverlayLayout::from_vertices(context, camera_global, projection, vertices)
}

/// Draws bounds visualization for a single camera/target pair.
fn draw_bounds_for_camera(
    line_context: &mut FitOverlayLineContext<'_, '_, '_>,
    config: &FitTargetOverlayConfig,
    layout: &FitOverlayLayout,
    ui_camera: Entity,
    label_query: &mut Query<(
        Entity,
        &MarginLabel,
        &FitOverlayVisual,
        &mut Text,
        &mut Node,
        &mut TextColor,
        &mut UiTargetCamera,
    )>,
    bounds_label_query: &mut Query<
        (
            Entity,
            &BoundsLabel,
            &FitOverlayVisual,
            &mut Node,
            &mut UiTargetCamera,
        ),
        Without<MarginLabel>,
    >,
    line_cleanup_query: &Query<(Entity, &FitOverlayVisual), With<FitOverlayLineVisual>>,
) {
    let camera = layout.context.camera;
    let bounds = &layout.bounds;
    let viewport_size = layout.viewport_size;
    let mut desired_line_kinds = Vec::new();

    bevy::log::trace!(
        "drawing fit overlay for {camera:?} with order {} and layers {:?}",
        layout.context.order,
        layout.context.layers
    );

    // Update margin percentages on camera entity for BRP inspection.
    // `try_insert` silently skips if the entity was despawned this frame
    // (e.g. closing a secondary window while visualization is active).
    line_context
        .commands
        .entity(camera)
        .try_insert(FitMarginPercents::from(bounds));

    let rectangle = rectangle_points(bounds);
    line_context.upsert_polyline(
        FitOverlayVisual {
            camera,
            kind: FitOverlayVisualKind::Rectangle,
        },
        &rectangle,
        true,
        config.rectangle_color,
        config.line_width,
    );
    desired_line_kinds.push(FitOverlayVisualKind::Rectangle);

    let silhouette = silhouette_points(layout);
    if silhouette.len() >= 2 {
        line_context.upsert_polyline(
            FitOverlayVisual {
                camera,
                kind: FitOverlayVisualKind::Silhouette,
            },
            &silhouette,
            true,
            config.silhouette_color,
            config.line_width,
        );
        desired_line_kinds.push(FitOverlayVisualKind::Silhouette);
    }

    // "Screen space bounds" label
    let upper_left = screen_space::norm_to_viewport(
        bounds.min_normalized_x,
        bounds.max_normalized_y,
        bounds.half_extent_x,
        bounds.half_extent_y,
        viewport_size,
    );
    labels::update_or_create_bounds_label(
        line_context.commands,
        bounds_label_query,
        camera,
        ui_camera,
        labels::bounds_label_position(upper_left),
    );

    // Margin lines + labels
    let draw_context = DrawContext {
        camera,
        ui_camera,
        bounds,
        viewport_size: Some(viewport_size),
    };
    let visible_edges =
        draw_margin_lines_and_labels(line_context, label_query, &draw_context, config);
    desired_line_kinds.extend(
        visible_edges
            .iter()
            .map(|&edge| FitOverlayVisualKind::MarginLine { edge }),
    );

    // Remove stale margin labels for this camera
    cleanup_stale_margin_labels(line_context.commands, label_query, camera, &visible_edges);
    lines::clear_stale_lines(
        line_context.commands,
        camera,
        &desired_line_kinds,
        line_cleanup_query,
    );
}

fn label_ui_camera<'a>(
    source_camera: Entity,
    source_target: &NormalizedRenderTarget,
    primary_window: Option<Entity>,
    cameras: impl IntoIterator<
        Item = (
            Entity,
            &'a Camera,
            &'a RenderTarget,
            Option<&'a Camera2d>,
            Option<&'a Camera3d>,
        ),
    >,
) -> Entity {
    // Bevy UI only extracts Camera2d/Camera3d views. The source camera remains
    // the fallback when it is the only suitable same-target UI camera.
    cameras
        .into_iter()
        .filter(|(_, camera, target, camera_2d, camera_3d)| {
            camera.is_active
                && target.normalize(primary_window).as_ref() == Some(source_target)
                && (camera_2d.is_some() || camera_3d.is_some())
        })
        .max_by_key(|(entity, camera, _, _, _)| (camera.order, *entity))
        .map_or(source_camera, |(entity, _, _, _, _)| entity)
}

#[cfg(test)]
mod tests {
    use bevy::window::WindowRef;

    use super::*;

    #[test]
    fn label_ui_camera_uses_top_camera_on_same_primary_window() -> Result<(), &'static str> {
        let mut world = World::new();
        let primary_window = world.spawn_empty().id();
        let other_window_entity = world.spawn_empty().id();
        let source_camera = world.spawn_empty().id();
        let overlay_camera = world.spawn_empty().id();
        let inactive_camera = world.spawn_empty().id();
        let other_window_camera = world.spawn_empty().id();

        let source = camera(0, true);
        let overlay = camera(100, true);
        let inactive = camera(200, false);
        let other_window = camera(300, true);
        let source_3d = Camera3d::default();
        let overlay_3d = Camera3d::default();
        let inactive_3d = Camera3d::default();
        let other_window_3d = Camera3d::default();

        let source_target = RenderTarget::Window(WindowRef::Primary);
        let overlay_target = RenderTarget::Window(WindowRef::Entity(primary_window));
        let other_target = RenderTarget::Window(WindowRef::Entity(other_window_entity));
        let normalized_source_target = source_target
            .normalize(Some(primary_window))
            .ok_or("source target should normalize")?;
        let cameras = [
            (
                source_camera,
                &source,
                &source_target,
                None,
                Some(&source_3d),
            ),
            (
                overlay_camera,
                &overlay,
                &overlay_target,
                None,
                Some(&overlay_3d),
            ),
            (
                inactive_camera,
                &inactive,
                &overlay_target,
                None,
                Some(&inactive_3d),
            ),
            (
                other_window_camera,
                &other_window,
                &other_target,
                None,
                Some(&other_window_3d),
            ),
        ];

        assert_eq!(
            label_ui_camera(
                source_camera,
                &normalized_source_target,
                Some(primary_window),
                cameras
            ),
            overlay_camera
        );
        Ok(())
    }

    #[test]
    fn label_ui_camera_falls_back_to_ui_renderable_source_camera() -> Result<(), &'static str> {
        let mut world = World::new();
        let primary_window = world.spawn_empty().id();
        let source_camera = world.spawn_empty().id();
        let non_ui_camera = world.spawn_empty().id();

        let source = camera(0, true);
        let non_ui = camera(500, true);
        let source_3d = Camera3d::default();

        let target = RenderTarget::Window(WindowRef::Primary);
        let normalized_target = target
            .normalize(Some(primary_window))
            .ok_or("target should normalize")?;
        let cameras = [
            (source_camera, &source, &target, None, Some(&source_3d)),
            (non_ui_camera, &non_ui, &target, None, None),
        ];

        assert_eq!(
            label_ui_camera(
                source_camera,
                &normalized_target,
                Some(primary_window),
                cameras
            ),
            source_camera
        );
        Ok(())
    }

    #[test]
    fn label_ui_camera_skips_non_ui_render_camera_on_same_target() -> Result<(), &'static str> {
        let mut world = World::new();
        let primary_window = world.spawn_empty().id();
        let source_camera = world.spawn_empty().id();
        let non_ui_camera = world.spawn_empty().id();
        let overlay_camera = world.spawn_empty().id();

        let source = camera(0, true);
        let non_ui = camera(500, true);
        let overlay = camera(100, true);
        let source_3d = Camera3d::default();
        let overlay_3d = Camera3d::default();

        let target = RenderTarget::Window(WindowRef::Primary);
        let normalized_target = target
            .normalize(Some(primary_window))
            .ok_or("target should normalize")?;
        let cameras = [
            (source_camera, &source, &target, None, Some(&source_3d)),
            (non_ui_camera, &non_ui, &target, None, None),
            (overlay_camera, &overlay, &target, None, Some(&overlay_3d)),
        ];

        assert_eq!(
            label_ui_camera(
                source_camera,
                &normalized_target,
                Some(primary_window),
                cameras
            ),
            overlay_camera
        );
        Ok(())
    }

    fn camera(order: isize, is_active: bool) -> Camera {
        Camera {
            order,
            is_active,
            ..default()
        }
    }
}
