//! Debug overlay system for fit target debugging.
//!
//! Provides screen-aligned boundary box and silhouette polygon visualization for the current
//! camera fit target. Uses Bevy's `GizmoConfigGroup` pattern (similar to `Avian3D`'s
//! `PhysicsGizmos`).

mod constants;
mod convex_hull;
mod labels;
mod screen_space;
mod systems;
mod types;

use bevy::prelude::*;
use types::FitTargetGizmo;
pub use types::FitTargetOverlayConfig;

use super::components::FitOverlay;

/// Plugin that enables fit target debug visualization.
pub(super) struct ZoomOverlayPlugin;

impl Plugin for ZoomOverlayPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::gizmos::GizmoPlugin>() {
            app.init_gizmo_group::<FitTargetGizmo>();
        }

        app.init_resource::<FitTargetOverlayConfig>()
            .add_observer(systems::on_remove_fit_visualization)
            .add_systems(
                Update,
                (
                    systems::sync_gizmo_render_layers,
                    systems::draw_fit_target_bounds,
                )
                    .chain()
                    .run_if(any_with_component::<FitOverlay>),
            );
    }
}
