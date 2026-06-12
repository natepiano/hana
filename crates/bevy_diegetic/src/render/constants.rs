//! Rendering constants and material utilities for diegetic panels.

use bevy::asset::uuid_handle;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use bevy_kana::ToF32;

use crate::cascade::DEFAULT_TEXT_DRAW_LAYER;
use crate::cascade::TextDrawLayer;
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
///
/// Calibration: `bevy_lagrange` syncs the perspective near plane to
/// `radius × 0.001`, so a fragment at the camera's focus distance has
/// `position.z = near / d ≈ 0.001`. The largest offset magnitude,
/// `DEFAULT_TEXT_DRAW_LAYER` steps (command 0), must stay well below
/// that or the offset drives `position.z` non-positive and
/// `pack_24bit_depth_8bit_alpha` in `oit_draw` saturates it to the
/// cleared-background depth, where bevy's resolve pass drops every
/// fragment whose alpha < 1.0. At `1e-6`, 64 steps total `6.4e-5`
/// (6.4% of the focus depth) and one step spans ~17 quanta of the
/// 24-bit OIT depth packing, so adjacent ordinals stay distinct.
///
/// Panels much farther than the camera focus shrink `position.z` below
/// the 64-step budget (z = near/d crosses `6.4e-5` at ~15.6× the orbit
/// radius). The `OIT_MIN_DEPTH` floor in `sdf_panel.wgsl` and
/// `slug_text.wgsl` keeps those fragments storable; past the bound their
/// coplanar ordering collapses to OIT-list insertion order instead of
/// going invisible.
pub(crate) const OIT_DEPTH_STEP: f32 = 0.000_001;

/// Shared draw-order ordinal for a panel's coplanar children.
///
/// Backing/image/line draw slots (`RenderCommand::draw_slot`) and text draw
/// layers both convert into it; [`Self::depth_bias`] and
/// [`Self::oit_depth_offset`] are the only derivations of the two per-material
/// ordering fields, so any two ordinals composite in the same relative order
/// on sorted and OIT views.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct DrawOrdinal(i32);

/// `Transparent3d` sort bias for normal batched panel-line entities.
///
/// Lines are vertex-pulled across many panels like text, so one batch render
/// item cannot use a per-command material depth without fragmenting batches.
/// The shader carries per-record command depth; this coarse lane keeps lines
/// above backing quads and below text in sorted, non-OIT views.
pub(crate) const BATCH_PANEL_LINE_DEPTH_BIAS: f32 = 63.0 * LAYER_DEPTH_BIAS;

/// `Transparent3d` sort bias for overlay panel-line entities.
pub(crate) const BATCH_PANEL_LINE_OVERLAY_DEPTH_BIAS: f32 = 96.0 * LAYER_DEPTH_BIAS;

impl DrawOrdinal {
    /// Converts a geometry draw slot, saturating at `i32::MAX`. A
    /// saturated ordinal sits above every text layer (`i8`), so it
    /// composites in front of all text on sorted views and clamps to the
    /// `0.0` OIT offset — unreachable in practice (a panel would need
    /// 2^31 geometry commands).
    pub(crate) fn from_draw_slot(draw_slot: usize) -> Self {
        Self(i32::try_from(draw_slot).unwrap_or(i32::MAX))
    }

    /// `Transparent3d` sort bias. Bevy adds it to the item's view-space
    /// distance (`sort_distance = view_z + depth_bias`, ascending sort,
    /// drawn back-to-front), so a higher ordinal composites in front.
    pub(crate) fn depth_bias(self) -> f32 { self.0.to_f32() * LAYER_DEPTH_BIAS }

    /// OIT fragment depth offset, added to `position.z` before `oit_draw`
    /// (reverse-Z: positive = closer). Clamped at `0.0`: ordinals below the
    /// default text layer move backward from zero so default-layer text
    /// composites over them, while ordinals at or above it stay at `0.0` so
    /// unrelated opaque geometry keeps real depth authority over panel
    /// content.
    pub(crate) fn oit_depth_offset(self) -> f32 {
        (self.0 - i32::from(DEFAULT_TEXT_DRAW_LAYER))
            .min(0)
            .to_f32()
            * OIT_DEPTH_STEP
    }
}

impl From<TextDrawLayer> for DrawOrdinal {
    fn from(draw_layer: TextDrawLayer) -> Self { Self(i32::from(draw_layer.0)) }
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

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use super::*;

    /// Ordinal pairs `(low, high)` spanning below-default, default-crossing,
    /// and above-default ranges.
    const ORDERED_LAYER_PAIRS: [(i8, i8); 8] = [
        (i8::MIN, -1),
        (-1, 0),
        (0, 1),
        (1, 63),
        (63, DEFAULT_TEXT_DRAW_LAYER),
        (0, DEFAULT_TEXT_DRAW_LAYER),
        (DEFAULT_TEXT_DRAW_LAYER, 65),
        (DEFAULT_TEXT_DRAW_LAYER, i8::MAX),
    ];

    #[test]
    fn sorted_and_oit_orderings_agree_for_every_layer_pair() {
        for (low, high) in ORDERED_LAYER_PAIRS {
            let low_ordinal = DrawOrdinal::from(TextDrawLayer(low));
            let high_ordinal = DrawOrdinal::from(TextDrawLayer(high));
            assert!(
                low_ordinal.depth_bias() < high_ordinal.depth_bias(),
                "sorted bias must rise from {low} to {high}",
            );
            if low < DEFAULT_TEXT_DRAW_LAYER {
                assert!(
                    low_ordinal.oit_depth_offset() < high_ordinal.oit_depth_offset(),
                    "OIT offset must rise from {low} to {high}",
                );
            } else {
                assert_eq!(
                    low_ordinal.oit_depth_offset().to_bits(),
                    high_ordinal.oit_depth_offset().to_bits(),
                    "OIT offsets at or above the default layer are clamped equal",
                );
            }
        }
    }

    #[test]
    fn default_layer_reproduces_previous_batch_material_values() {
        let text_ordinal = DrawOrdinal::from(TextDrawLayer(DEFAULT_TEXT_DRAW_LAYER));
        // Pre-DrawOrdinal constants: BATCH_TEXT_DEPTH_BIAS = 64.0 ×
        // LAYER_DEPTH_BIAS and a hard-coded 0.0 OIT offset.
        assert_eq!(text_ordinal.depth_bias().to_bits(), 64.0f32.to_bits());
        assert_eq!(text_ordinal.oit_depth_offset().to_bits(), 0.0f32.to_bits());
    }

    #[test]
    fn backing_depth_bias_matches_previous_command_index_formula() {
        for draw_slot in [0_usize, 1, 5, 63] {
            let expected = draw_slot.to_f32() * LAYER_DEPTH_BIAS;
            let actual = DrawOrdinal::from_draw_slot(draw_slot).depth_bias();
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
    }

    #[test]
    fn backing_oit_offsets_stay_behind_default_text_and_rise_with_draw_slot() {
        let default_layer =
            usize::try_from(DEFAULT_TEXT_DRAW_LAYER).expect("default layer is positive");
        let mut previous = f32::NEG_INFINITY;
        for draw_slot in 0..default_layer {
            let offset = DrawOrdinal::from_draw_slot(draw_slot).oit_depth_offset();
            assert!(offset < 0.0, "slot {draw_slot} must sit behind text");
            assert!(offset > previous, "offsets must rise with draw slot");
            previous = offset;
        }
    }

    #[test]
    fn from_draw_slot_saturates_at_i32_max() {
        assert_eq!(
            DrawOrdinal::from_draw_slot(usize::MAX),
            DrawOrdinal(i32::MAX),
        );
    }
}
