//! Rendering systems for diegetic UI panels and text.

mod clip;
mod constants;
#[cfg(test)]
mod glyph_mesh_tests;
mod glyph_quad;
mod msdf_material;
mod panel_geometry;
mod panel_rtt;
mod sdf_material;
mod text_renderer;
mod transparency;
mod world_text;

use bevy::prelude::*;
pub(crate) use constants::LAYER_DEPTH_BIAS;
pub(crate) use constants::OIT_DEPTH_STEP;
pub(crate) use constants::SDF_AA_PADDING;
pub use constants::default_panel_material;
pub(crate) use sdf_material::SdfPanelMaterial;
pub(crate) use sdf_material::sdf_panel_material;
pub(crate) use sdf_material::sdf_shape_material;
pub use transparency::StableTransparency;
pub use transparency::TextAlphaModeDefault;
#[cfg(feature = "typography_overlay")]
pub use world_text::ComputedWorldText;
pub use world_text::PanelTextChild;
pub use world_text::PendingGlyphs;
pub use world_text::WorldText;
pub use world_text::WorldTextReady;

/// Umbrella render plugin — registers the three render-side sub-plugins
/// (MSDF text, SDF panel geometry, RTT panel compositing).
pub(crate) struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            text_renderer::TextRenderPlugin,
            panel_geometry::PanelGeometryPlugin,
            panel_rtt::PanelRttPlugin,
        ))
        .init_resource::<TextAlphaModeDefault>()
        .add_observer(transparency::on_stable_transparency_added)
        .add_observer(transparency::on_stable_transparency_removed)
        .add_systems(Update, queue_alpha_default_refresh);
    }
}

/// Marker: this text entity's cached alpha mode was resolved through an
/// earlier [`TextAlphaModeDefault`] value that has since changed, so its
/// rendered material is stale and must be rebuilt.
///
/// Inserted by [`queue_alpha_default_refresh`] when the resource changes;
/// cleared unconditionally at the top of each consumer's per-entity loop.
#[derive(Component)]
pub(super) struct StaleAlphaMode;

/// Queues standalone and panel text entities for an alpha-mode rebuild when
/// [`TextAlphaModeDefault`] changes. Entities with explicit alpha mode overrides
/// are still marked — the optimization to skip them requires access to per-panel
/// alpha mode from this system and hasn't shown up as a measurable cost.
fn queue_alpha_default_refresh(
    alpha_default: Res<TextAlphaModeDefault>,
    world_texts: Query<Entity, (With<world_text::WorldText>, Without<PanelTextChild>)>,
    panel_texts: Query<Entity, With<PanelTextChild>>,
    mut commands: Commands,
) {
    if !alpha_default.is_changed() {
        return;
    }

    for entity in &world_texts {
        commands.entity(entity).insert(StaleAlphaMode);
    }

    for entity in &panel_texts {
        commands.entity(entity).insert(StaleAlphaMode);
    }
}
