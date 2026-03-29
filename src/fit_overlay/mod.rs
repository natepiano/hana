//! Debug overlay system for fit target debugging.
//!
//! Provides screen-aligned boundary box and silhouette polygon visualization for the current
//! camera fit target. Uses Bevy's `GizmoConfigGroup` pattern (similar to `Avian3D`'s
//! `PhysicsGizmos`).

mod convex_hull;
mod labels;
mod screen_space;
mod systems;
mod types;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use labels::BoundsLabel;
use labels::MarginLabel;
pub use types::FitTargetGizmo;
pub use types::FitTargetOverlayConfig;
use types::FitTargetViewportMarginPcts;

use super::components::FitOverlay;

/// Plugin that enables fit target debug visualization.
pub struct ZoomOverlayPlugin;

impl Plugin for ZoomOverlayPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::gizmos::GizmoPlugin>() {
            app.init_gizmo_group::<FitTargetGizmo>();
        }

        app.init_resource::<FitTargetOverlayConfig>()
            .add_observer(on_remove_fit_visualization)
            .add_systems(
                Update,
                (sync_gizmo_render_layers, systems::draw_fit_target_bounds)
                    .chain()
                    .run_if(any_with_component::<FitOverlay>),
            );
    }
}

/// Observer that cleans up visualization state when `FitVisualization` is removed from a camera.
fn on_remove_fit_visualization(
    trigger: On<Remove, FitOverlay>,
    mut commands: Commands,
    label_query: Query<(Entity, &MarginLabel)>,
    bounds_label_query: Query<(Entity, &BoundsLabel)>,
) {
    let camera = trigger.entity;

    // Clean up viewport margins from the camera entity.
    // `try_remove` silently skips if the entity was despawned this frame
    // (e.g. closing a secondary window triggers component removal during despawn).
    commands
        .entity(camera)
        .try_remove::<FitTargetViewportMarginPcts>();

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
fn sync_gizmo_render_layers(
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
