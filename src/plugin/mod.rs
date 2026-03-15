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

use crate::layout::ForLayout;
use crate::layout::ForStandalone;
use crate::layout::TextProps;
use crate::text::FontRegistry;
use crate::text::create_parley_measurer;

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
        // Initialize font registry and wire up parley-backed text measurement.
        let registry = FontRegistry::new();
        let measurer = DiegeticTextMeasurer(create_parley_measurer(
            registry.font_context(),
            registry.family_names(),
        ));

        app.insert_resource(registry)
            .insert_resource(measurer)
            .register_type::<TextProps<ForLayout>>()
            .register_type::<TextProps<ForStandalone>>()
            .init_gizmo_group::<DiegeticPanelGizmoGroup>()
            .add_systems(Update, (compute_panel_layouts, render_panel_gizmos).chain());
    }
}
