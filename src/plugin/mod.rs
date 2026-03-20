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
pub use systems::DiegeticPerfStats;
pub use systems::ShowTextGizmos;
pub(super) use systems::compute_panel_layouts;
use systems::render_panel_gizmos;

use crate::layout::ForLayout;
use crate::layout::ForStandalone;
use crate::layout::TextProps;
use crate::render::ShapedTextCache;
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

/// Layout-only plugin for diegetic UI panels.
///
/// Registers the layout computation system, text measurement, and shaped
/// text cache — but no rendering, no gizmos, no GPU. Suitable for headless
/// apps and benchmarks.
///
/// Use [`DiegeticUiPlugin`] instead for full rendering support.
pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<Self>() {
            app.init_resource::<ShapedTextCache>()
                .init_resource::<DiegeticPerfStats>()
                .add_systems(Update, compute_panel_layouts);
        }
    }
}

/// Plugin that adds diegetic UI panel support to a Bevy app.
///
/// Includes [`LayoutPlugin`] plus MSDF text rendering, font
/// registration, atlas management, and gizmo debug wireframes.
///
/// Registers:
/// - [`DiegeticTextMeasurer`] resource (default parley-backed, overridable).
/// - Layout computation system (runs in `Update`).
/// - MSDF text rendering (runs in `PostUpdate`).
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
            .add_plugins(LayoutPlugin)
            .init_resource::<ShowTextGizmos>()
            .register_type::<TextProps<ForLayout>>()
            .register_type::<TextProps<ForStandalone>>()
            .add_plugins(TextRenderPlugin)
            .init_gizmo_group::<DiegeticPanelGizmoGroup>()
            .add_systems(Startup, upload_atlas_to_gpu)
            .add_systems(Update, render_panel_gizmos.after(compute_panel_layouts));
    }
}
