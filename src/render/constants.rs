//! Rendering constants and material utilities for diegetic panels.

use bevy::asset::uuid_handle;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;

// Layer ordering
/// Per-command depth bias for Geometry mode sort ordering.
///
/// Bevy packs this through `i32` into `DepthBiasState.constant`.
/// Controls the `Transparent3d` sort key so back-to-front submission
/// order matches the painter's order. Also wins the depth test for
/// coplanar fragments.
pub(crate) const LAYER_DEPTH_BIAS: f32 = 1.0;
/// Per-command OIT depth offset for coplanar fragment ordering.
///
/// Added to `position.z` in the fragment shader before `oit_draw`
/// stores the fragment. Pipeline `depth_bias` does NOT affect
/// `in.position.z`, so we apply this offset manually.
/// Reverse-Z: positive = closer to camera = composited in front.
pub(crate) const OIT_DEPTH_STEP: f32 = 0.0001;
/// Render layer offset — panel layers start here to avoid conflicts with
/// user-defined layers.
pub(super) const PANEL_LAYER_OFFSET: usize = 16;

// Material defaults
/// Default metallic value for panel surfaces. Non-metallic (dielectric).
pub(super) const DEFAULT_METALLIC: f32 = 0.0;
/// Default reflectance for panel surfaces. Very low specular to avoid
/// washing out colors under bright lights.
pub(super) const DEFAULT_REFLECTANCE: f32 = 0.02;
/// Default roughness for panel surfaces. Matte paper-like appearance.
pub(super) const DEFAULT_ROUGHNESS: f32 = 0.95;

// Panel RTT
/// Default texels per world-space meter for RTT resolution.
/// ~200 DPI at arm's length.
pub(super) const DEFAULT_TEXELS_PER_METER: f32 = 10000.0;
/// Maximum texture dimension in pixels.
pub(super) const MAX_TEXTURE_SIZE: u32 = 4096;
/// Minimum texture dimension in pixels.
pub(super) const MIN_TEXTURE_SIZE: u32 = 64;

// SDF rendering
/// World-space padding added to each SDF quad mesh beyond the shape boundary.
/// Gives the exterior anti-aliasing ramp room to render — without this, the
/// mesh edge coincides with the SDF boundary and the AA fade-out is clipped.
pub(crate) const SDF_AA_PADDING: f32 = 0.001;
/// Internal-asset handle for the `sdf_stroke.wgsl` shader.
pub(super) const SDF_STROKE_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("536f3741-5418-4d7a-a0b2-8cfb1d30e8a1");

// Text rendering
/// Fixed panel-local Z for text and image meshes.
///
/// Layering is handled by `StandardMaterial::depth_bias`, so panel-local
/// geometry stays coplanar.
pub(super) const TEXT_Z_OFFSET: f32 = 0.0;
/// Default clip rect for unclipped text: effectively infinite panel-local
/// bounds so the shader clip test becomes a no-op.
pub(super) const UNCLIPPED_TEXT_CLIP_RECT: Vec4 = Vec4::new(-1e6, -1e6, 1e6, 1e6);

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
