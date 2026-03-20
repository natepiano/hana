//! Rendering systems for diegetic UI panels and text.

mod glyph_quad;
mod msdf_material;
mod text_renderer;
mod world_text;

pub use glyph_quad::GlyphQuadData;
pub use glyph_quad::build_glyph_mesh;
pub use msdf_material::MsdfExtension;
pub use msdf_material::MsdfTextMaterial;
pub use msdf_material::MsdfTextUniform;
pub use msdf_material::msdf_text_material;
pub use text_renderer::ShapedTextCache;
pub use text_renderer::TextRenderPlugin;
pub use text_renderer::TextShapingContext;
pub use text_renderer::shape_text_to_quads;
pub use world_text::WorldText;
