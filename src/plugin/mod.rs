//! Bevy plugin for diegetic UI panels.
//!
//! Provides [`DiegeticUiPlugin`], which adds layout computation and optional
//! gizmo debug rendering for [`DiegeticPanel`] entities.

mod components;
mod systems;

use bevy::prelude::*;
pub use components::ComputedDiegeticPanel;
pub use components::DiegeticPanel;
pub use components::DiegeticTextMeasurer;
use systems::compute_panel_layouts;
use systems::render_panel_gizmos;

/// Gizmo group for diegetic panel debug wireframes.
///
/// Enable or disable via Bevy's [`GizmoConfigStore`].
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct DiegeticPanelGizmoGroup;

/// Plugin that adds diegetic UI panel support to a Bevy app.
///
/// Registers:
/// - [`DiegeticTextMeasurer`] resource (default monospace approximation, overridable).
/// - Layout computation system (runs in `Update`).
/// - Gizmo debug renderer (runs in `Update` after computation).
/// - [`DiegeticPanelGizmoGroup`] for controlling debug visibility.
pub struct DiegeticUiPlugin;

impl Plugin for DiegeticUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DiegeticTextMeasurer>()
            .init_gizmo_group::<DiegeticPanelGizmoGroup>()
            .add_systems(Update, (compute_panel_layouts, render_panel_gizmos).chain());
    }
}
