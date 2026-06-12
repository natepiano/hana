mod constants;
mod material;
mod run_data;

use bevy::prelude::Handle;
use bevy::render::storage::ShaderBuffer;
pub(super) use constants::SLUG_TEXT_VERTEX_PULL_SHADER_HANDLE;
pub(crate) use material::BatchTextMaterialInput;
pub(crate) use material::RenderMode;
pub(crate) use material::TextExtension;
pub(crate) use material::TextExtensionKey;
pub(crate) use material::TextMaterial;
pub(crate) use material::batch_text_material;
pub(crate) use material::set_batch_text_material_buffers;
pub(crate) use material::set_text_material_anti_alias;
#[cfg(feature = "batch_proof")]
pub(crate) use material::toggle_text_material_debug_glyph_index;
pub(crate) use run_data::glyph_quad_extents;

#[cfg(test)]
pub(super) const fn text_material_oit_depth_offset(material: &TextMaterial) -> f32 {
    material::text_material_oit_depth_offset(material)
}

pub(super) fn set_text_material_atlas(
    material: &mut TextMaterial,
    curves: Handle<ShaderBuffer>,
    bands: Handle<ShaderBuffer>,
    glyphs: Handle<ShaderBuffer>,
) {
    material::set_text_material_atlas(material, curves, bands, glyphs);
}
