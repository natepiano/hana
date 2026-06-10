//! Rendering constants and material utilities for diegetic panels.

use bevy::asset::uuid_handle;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use bevy_kana::ToF32;

use crate::layout::GlyphSidedness;

// layer ordering
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

/// `Transparent3d` sort bias for batched-text entities.
///
/// Per-run text materials sort after their panel's SDF backing layers via
/// `command_depth × LAYER_DEPTH_BIAS`; a batch is one phase item covering
/// runs across many panels, so it carries one bias that puts text after
/// every backing layer (backing biases are `command_index ×
/// LAYER_DEPTH_BIAS`, far below this constant). Without it, a sorted
/// (non-OIT) view can composite a panel's translucent backing over the
/// whole batch, dimming the text behind it. Within the batch, coplanar
/// glyph order comes from the per-record depth nudge instead.
///
/// Assumes a panel's backing layers stay below 64 commands deep; a panel
/// exceeding that would out-bias its own text on sorted views.
pub(crate) const BATCH_TEXT_DEPTH_BIAS: f32 = 64.0 * LAYER_DEPTH_BIAS;

/// `Transparent3d` sort bias for normal batched panel-line entities.
///
/// Lines are vertex-pulled across many panels like text, so one batch render
/// item cannot use a per-command material depth without fragmenting batches.
/// The shader carries per-record command depth; this coarse lane keeps lines
/// above backing quads and below text in sorted, non-OIT views.
pub(crate) const BATCH_PANEL_LINE_DEPTH_BIAS: f32 = 63.0 * LAYER_DEPTH_BIAS;

/// `Transparent3d` sort bias for overlay panel-line entities.
pub(crate) const BATCH_PANEL_LINE_OVERLAY_DEPTH_BIAS: f32 = 96.0 * LAYER_DEPTH_BIAS;

/// OIT depth offset for non-text panel layers.
///
/// Panel text stays at `0.0` so unrelated opaque geometry keeps real depth
/// authority. Backgrounds, borders, and other SDF backing layers move backward
/// from zero, using command order only as a coplanar tie-breaker.
#[must_use]
pub(crate) fn panel_backing_oit_depth_offset(command_index: usize) -> f32 {
    -(command_index.to_f32() + 1.0) * OIT_DEPTH_STEP
}

// material defaults
/// Default metallic value for panel surfaces. Non-metallic (dielectric).
pub(super) const DEFAULT_METALLIC: f32 = 0.0;
/// Default reflectance for panel surfaces. Very low specular to avoid
/// washing out colors under bright lights.
pub(super) const DEFAULT_REFLECTANCE: f32 = 0.02;
/// Default roughness for panel surfaces. Matte paper-like appearance.
pub(super) const DEFAULT_ROUGHNESS: f32 = 0.95;

// sdf rendering
/// World-space padding added to each SDF quad mesh beyond the SDF boundary.
/// Gives the exterior anti-aliasing ramp room to render — without this, the
/// mesh edge coincides with the SDF boundary and the AA fade-out is clipped.
pub(crate) const SDF_AA_PADDING: f32 = 0.001;
/// Internal-asset handle for the `sdf_stroke.wgsl` shader.
pub(super) const SDF_STROKE_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("536f3741-5418-4d7a-a0b2-8cfb1d30e8a1");

// text rendering
/// Fixed panel-local Z for text and image meshes.
///
/// Layering is handled by `StandardMaterial::depth_bias`, so panel-local
/// geometry stays coplanar.
pub(super) const TEXT_Z_OFFSET: f32 = 0.0;
/// Default clip rect for unclipped text: effectively infinite panel-local
/// bounds so the shader clip test becomes a no-op.
pub(super) const UNCLIPPED_TEXT_CLIP_RECT: Vec4 = Vec4::new(-1e6, -1e6, 1e6, 1e6);

/// Configures a `StandardMaterial`'s `double_sided` and `cull_mode` fields from
/// a [`GlyphSidedness`] choice. Shared by the panel-text and world-text glyph
/// material builders.
pub(crate) const fn apply_glyph_sidedness(base: &mut StandardMaterial, sidedness: GlyphSidedness) {
    match sidedness {
        GlyphSidedness::DoubleSided => {
            base.double_sided = true;
            base.cull_mode = None;
        },
        GlyphSidedness::OneSided => {
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
