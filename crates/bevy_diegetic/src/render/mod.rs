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
#[cfg(feature = "typography_overlay")]
pub use world_text::ComputedWorldText;
pub use world_text::PanelTextChild;
pub use world_text::PendingGlyphs;
pub(crate) use world_text::WorldFontUnit;
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
        .add_observer(transparency::on_stable_transparency_added)
        .add_observer(transparency::on_stable_transparency_removed);
    }
}
