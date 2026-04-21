//! Debug overlay system for fit target debugging.
//!
//! Provides screen-aligned boundary box and silhouette polygon visualization for the current
//! camera fit target. Uses Bevy's `GizmoConfigGroup` pattern (similar to `Avian3D`'s
//! `PhysicsGizmos`).

mod constants;
mod convex_hull;
mod fit_target_bounds;
mod labels;
mod screen_space;

use bevy::prelude::*;
use fit_target_bounds::FitTargetGizmo;
pub use fit_target_bounds::FitTargetOverlayConfig;

use super::components::FitOverlay;

/// Plugin that enables fit target debug visualization.
pub(super) struct ZoomOverlayPlugin;

impl Plugin for ZoomOverlayPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::gizmos::GizmoPlugin>() {
            app.init_gizmo_group::<FitTargetGizmo>();
        }

        app.init_resource::<FitTargetOverlayConfig>()
            .add_observer(fit_target_bounds::on_remove_fit_visualization)
            .add_systems(
                Update,
                (
                    fit_target_bounds::sync_gizmo_render_layers,
                    fit_target_bounds::draw_fit_target_bounds,
                )
                    .chain()
                    .run_if(any_with_component::<FitOverlay>),
            );
    }
}
