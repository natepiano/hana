//! Rendering systems for diegetic UI panels and text.

#[cfg(test)]
mod glyph_mesh_tests;
mod glyph_quad;
mod msdf_material;
mod text_renderer;
mod world_text;

pub use text_renderer::ShapedTextCache;
pub use text_renderer::TextRenderPlugin;
pub use world_text::WorldText;
