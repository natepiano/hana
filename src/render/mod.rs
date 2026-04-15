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
mod world_text;

pub(crate) use constants::LAYER_DEPTH_BIAS;
pub(crate) use constants::OIT_DEPTH_STEP;
pub(crate) use constants::SDF_AA_PADDING;
pub use constants::default_panel_material;
pub use panel_geometry::PanelGeometryPlugin;
pub use panel_rtt::PanelRttPlugin;
pub(crate) use sdf_material::SdfPanelMaterial;
pub(crate) use sdf_material::sdf_panel_material;
pub(crate) use sdf_material::sdf_shape_material;
pub use text_renderer::LineMetricsSnapshot;
pub use text_renderer::ShapedTextCache;
pub use text_renderer::TextRenderPlugin;
#[cfg(feature = "typography_overlay")]
pub use world_text::ComputedWorldText;
pub use world_text::PanelTextChild;
pub use world_text::PendingGlyphs;
pub use world_text::WorldText;
pub use world_text::WorldTextReady;
