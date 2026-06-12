mod run_data;

pub(crate) use run_data::glyph_quad_extents;

#[cfg(test)]
use crate::render::TextMaterial;

#[cfg(test)]
pub(super) const fn text_material_oit_depth_offset(material: &TextMaterial) -> f32 {
    crate::render::text_material_oit_depth_offset(material)
}
