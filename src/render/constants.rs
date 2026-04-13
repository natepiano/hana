//! Rendering constants and material utilities for diegetic panels.

use bevy::pbr::StandardMaterial;
use bevy::prelude::*;

// Layer ordering
/// Per-command depth bias for Geometry mode sort ordering.
///
/// Bevy packs this through `i32` into `DepthBiasState.constant`.
/// Controls the `Transparent3d` sort key so back-to-front submission
/// order matches the painter's order. Also wins the depth test for
/// coplanar fragments.
pub(super) const LAYER_DEPTH_BIAS: f32 = 1.0;

/// World-space padding added to each SDF quad mesh beyond the shape boundary.
/// Gives the exterior anti-aliasing ramp room to render — without this, the
/// mesh edge coincides with the SDF boundary and the AA fade-out is clipped.
pub(super) const SDF_AA_PADDING: f32 = 0.001;

/// Per-command OIT depth offset for coplanar fragment ordering.
///
/// Added to `position.z` in the fragment shader before `oit_draw`
/// stores the fragment. Pipeline `depth_bias` does NOT affect
/// `in.position.z`, so we apply this offset manually.
/// Reverse-Z: positive = closer to camera = composited in front.
pub(super) const OIT_DEPTH_STEP: f32 = 0.0001;

// Material defaults
/// Default metallic value for panel surfaces. Non-metallic (dielectric).
pub(super) const DEFAULT_METALLIC: f32 = 0.0;

/// Default reflectance for panel surfaces. Very low specular to avoid
/// washing out colors under bright lights.
pub(super) const DEFAULT_REFLECTANCE: f32 = 0.02;

/// Default roughness for panel surfaces. Matte paper-like appearance.
pub(super) const DEFAULT_ROUGHNESS: f32 = 0.95;

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

/// Resolves a material from the element → panel → library default chain.
///
/// If `layout_color` is `Some`, the resolved material's `base_color` is
/// overridden with that color. If `layout_color` is `None`, the material's
/// own `base_color` is preserved.
#[must_use]
pub(super) fn resolve_material(
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
