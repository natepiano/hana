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
pub use systems::compute_panel_layouts;
use systems::render_panel_gizmos;

use crate::layout::ForLayout;
use crate::layout::ForStandalone;
use crate::layout::TextProps;
use crate::render::TextRenderPlugin;
use crate::text::EMBEDDED_FONT;
use crate::text::FontRegistry;
use crate::text::MsdfAtlas;
use crate::text::create_parley_measurer;

/// Uploads the MSDF atlas pixel data to a GPU image at startup.
fn upload_atlas_to_gpu(mut atlas: ResMut<MsdfAtlas>, mut images: ResMut<Assets<Image>>) {
    atlas.upload_to_gpu(&mut images);
}

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

        // Initialize MSDF atlas and prepopulate ASCII glyphs.
        let mut atlas = MsdfAtlas::new();
        let ascii: String = (33_u8..=126).map(|c| c as char).collect();
        atlas.prepopulate(0, EMBEDDED_FONT, &ascii);

        app.insert_resource(registry)
            .insert_resource(measurer)
            .insert_resource(atlas)
            .register_type::<TextProps<ForLayout>>()
            .register_type::<TextProps<ForStandalone>>()
            .add_plugins(TextRenderPlugin)
            .init_gizmo_group::<DiegeticPanelGizmoGroup>()
            .add_systems(Startup, upload_atlas_to_gpu)
            .add_systems(Update, (compute_panel_layouts, render_panel_gizmos).chain());
    }
}
