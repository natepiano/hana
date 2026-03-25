//! Rendering systems for diegetic UI panels and text.

mod constants;
#[cfg(test)]
mod glyph_mesh_tests;
mod glyph_quad;
mod msdf_material;
mod text_renderer;
mod world_text;

pub use text_renderer::LineMetricsSnapshot;
pub use text_renderer::ShapedTextCache;
pub use text_renderer::TextRenderPlugin;
#[cfg(feature = "typography_overlay")]
pub use world_text::ComputedWorldText;
pub use world_text::PanelTextChild;
pub use world_text::PendingGlyphs;
pub use world_text::WorldText;
pub use world_text::WorldTextReady;
