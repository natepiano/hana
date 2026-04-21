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
use text_renderer::PanelTextQuads;
pub use transparency::StableTransparency;
pub use transparency::TextAlphaModeDefault;
#[cfg(feature = "typography_overlay")]
pub use world_text::ComputedWorldText;
pub use world_text::PanelTextChild;
pub use world_text::PendingGlyphs;
pub use world_text::WorldText;
pub use world_text::WorldTextReady;

use crate::layout::WorldTextStyle;

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
        .add_systems(Update, invalidate_text_on_alpha_default_change);
    }
}

/// Forces text materials to rebuild when [`TextAlphaModeDefault`] changes.
///
/// Standalone `WorldText` and batched panel text cache their materials until
/// their `Changed<>` filter fires. The resource-level default is invisible to
/// those filters, so without this system users toggling the default would see
/// no effect on already-spawned text.
fn invalidate_text_on_alpha_default_change(
    alpha_default: Res<TextAlphaModeDefault>,
    mut world_styles: Query<&mut WorldTextStyle>,
    mut panel_quads: Query<&mut PanelTextQuads>,
) {
    if !alpha_default.is_changed() {
        return;
    }
    for mut s in &mut world_styles {
        let _ = s.as_mut();
    }
    for mut q in &mut panel_quads {
        let _ = q.as_mut();
    }
}
