//! Rendering systems for diegetic UI panels and text.

mod clip;
mod constants;
mod panel_geometry;
mod panel_rtt;
mod sdf_material;
mod text_renderer;
mod text_shaping;
mod world_text;

use bevy::prelude::*;
pub(crate) use constants::LAYER_DEPTH_BIAS;
pub(crate) use constants::SDF_AA_PADDING;
pub use constants::default_panel_material;
use panel_geometry::PanelGeometryPlugin;
use panel_rtt::PanelRttPlugin;
pub(crate) use sdf_material::SdfPanelMaterial;
pub(crate) use sdf_material::SdfPanelMaterialInput;
pub(crate) use sdf_material::SdfPrimitiveKind;
pub(crate) use sdf_material::SdfPrimitiveMaterialInput;
pub(crate) use sdf_material::sdf_panel_material;
pub(crate) use sdf_material::sdf_primitive_material;
use text_renderer::TextRenderPlugin;
pub(crate) use text_shaping::PositionedGlyph;
#[cfg(feature = "typography_overlay")]
pub use world_text::ComputedWorldText;
pub use world_text::PanelTextChild;
pub use world_text::PendingGlyphs;
pub(crate) use world_text::WorldFontUnit;
pub use world_text::WorldText;
pub use world_text::WorldTextReady;

/// Umbrella render plugin — registers the three render-side sub-plugins
/// (slug text, SDF panel geometry, RTT panel compositing).
pub(crate) struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((TextRenderPlugin, PanelGeometryPlugin, PanelRttPlugin));
    }
}
