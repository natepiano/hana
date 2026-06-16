//! Material helpers for diegetic panel rendering.

use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::render::render_resource::Face;

use super::constants::DEFAULT_METALLIC;
use super::constants::DEFAULT_REFLECTANCE;
use super::constants::DEFAULT_ROUGHNESS;
use crate::layout::Sidedness;

/// Configures a `StandardMaterial`'s `double_sided` and `cull_mode` fields from
/// a [`Sidedness`] choice. Shared by shape and text material builders.
pub(crate) const fn apply_sidedness(base: &mut StandardMaterial, sidedness: Sidedness) {
    match sidedness {
        Sidedness::DoubleSided => {
            base.double_sided = true;
            base.cull_mode = None;
        },
        Sidedness::OneSided => {
            base.double_sided = false;
            base.cull_mode = Some(Face::Back);
        },
    }
}

/// Returns the library's default matte `StandardMaterial`.
#[must_use]
pub fn default_panel_material() -> StandardMaterial {
    StandardMaterial {
        perceptual_roughness: DEFAULT_ROUGHNESS,
        metallic: DEFAULT_METALLIC,
        reflectance: DEFAULT_REFLECTANCE,
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

/// Resolves a material from the element, panel, and library default chain.
///
/// If `layout_color` is `Some`, the resolved material's `base_color` is
/// overridden with that color. If `layout_color` is `None`, the material's
/// own `base_color` is preserved.
#[must_use]
pub(crate) fn resolve_material(
    element_material: Option<&StandardMaterial>,
    panel_material: Option<&StandardMaterial>,
    layout_color: Option<Color>,
) -> StandardMaterial {
    let mut material = element_material
        .or(panel_material)
        .cloned()
        .unwrap_or_else(default_panel_material);

    if let Some(color) = layout_color {
        material.base_color = color;
    }

    material
}
