//! Internal runtime plugin that wires the full diegetic UI stack together.

use bevy::prelude::*;

use super::AtlasConfig;
use super::DiegeticPanelGizmoGroup;
use super::DiegeticTextMeasurer;
use super::LayoutPlugin;
use super::ShowTextGizmos;
use super::UnitConfig;
use crate::callouts::CalloutPlugin;
use crate::render::PanelGeometryPlugin;
use crate::render::PanelRttPlugin;
use crate::render::TextRenderPlugin;
use crate::text;
use crate::text::Font;
use crate::text::FontLoader;
use crate::text::FontRegistry;
use crate::text::MsdfAtlas;

pub(super) struct DiegeticRuntimePlugin {
    config:      Option<AtlasConfig>,
    unit_config: Option<UnitConfig>,
}

impl DiegeticRuntimePlugin {
    pub(super) const fn new(config: Option<AtlasConfig>, unit_config: Option<UnitConfig>) -> Self {
        Self {
            config,
            unit_config,
        }
    }
}

impl Plugin for DiegeticRuntimePlugin {
    fn build(&self, app: &mut App) {
        // Initialize font registry and wire up parley-backed text measurement.
        let Some(registry) = FontRegistry::new() else {
            warn!("bevy_diegetic: embedded font failed to parse — plugin disabled");
            return;
        };
        let measurer = DiegeticTextMeasurer {
            measure_fn: text::create_parley_measurer(
                registry.font_context(),
                registry.family_names(),
            ),
        };

        // Initialize MSDF atlas — glyphs are rasterized on demand.
        // Skip if the user pre-inserted a custom atlas resource directly.
        if !app.world().contains_resource::<MsdfAtlas>() {
            let config = self.config.unwrap_or_default();

            // Only log when the user explicitly configured the atlas.
            if self.config.is_some() {
                config.log_and_clamp();
            }

            let page_size = config.page_size();
            let canonical_size = config.canonical_size();
            let glyph_worker_threads = config.clamped_glyph_worker_threads();
            app.insert_resource(MsdfAtlas::with_config(
                page_size,
                canonical_size,
                glyph_worker_threads,
            ));
        }

        app.insert_resource(self.unit_config.unwrap_or_default())
            .insert_resource(registry)
            .insert_resource(measurer)
            .init_asset::<Font>()
            .init_asset_loader::<FontLoader>()
            .add_plugins(LayoutPlugin)
            .add_plugins(CalloutPlugin)
            .init_resource::<ShowTextGizmos>()
            .add_plugins(TextRenderPlugin)
            .add_plugins(PanelGeometryPlugin)
            .add_plugins(PanelRttPlugin)
            .init_gizmo_group::<DiegeticPanelGizmoGroup>()
            .add_systems(
                Startup,
                (
                    super::systems::init_atlas_and_embedded_font,
                    super::systems::configure_panel_gizmos,
                ),
            )
            .add_systems(
                PostUpdate,
                (
                    super::systems::consume_loaded_fonts,
                    super::systems::watch_font_failures,
                ),
            )
            .add_systems(
                Update,
                (
                    super::systems::ensure_oit_on_cameras,
                    super::screen_space::position_screen_space_panels
                        .before(super::systems::compute_panel_layouts),
                    super::screen_space::setup_screen_space_cameras
                        .after(super::systems::compute_panel_layouts),
                    super::systems::render_layout_gizmos
                        .after(super::systems::compute_panel_layouts),
                    super::systems::render_debug_gizmos
                        .after(super::systems::compute_panel_layouts),
                ),
            )
            .add_systems(
                PostUpdate,
                (
                    super::screen_space::propagate_screen_space_render_layers,
                    super::screen_space::cleanup_screen_space_cameras,
                ),
            );

        #[cfg(feature = "typography_overlay")]
        {
            app.add_observer(crate::debug::on_overlay_added);
            app.add_observer(crate::debug::on_overlay_removed);
            app.add_systems(Update, crate::debug::build_typography_overlay);
            app.add_systems(
                PostUpdate,
                crate::debug::emit_typography_overlay_ready
                    .after(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
            );
        }
    }
}
