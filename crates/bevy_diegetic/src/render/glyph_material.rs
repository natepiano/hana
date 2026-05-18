//! Glyph text material — extends `StandardMaterial` with signed-
//! distance-field atlas decoding.
//!
//! Uses Bevy's [`ExtendedMaterial`] to layer glyph rendering on top of
//! the full PBR pipeline. The shader branches on the
//! [`DistanceField`](crate::DistanceField) uniform to pick MSDF (median
//! of RGB) vs. SDF (single R channel) decode. PBR lighting, shadows,
//! double-sided normals, and other `StandardMaterial` properties stay
//! intact.

use bevy::asset::Asset;
use bevy::image::Image;
use bevy::math::UVec2;
use bevy::math::Vec4;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MaterialExtension;
use bevy::pbr::StandardMaterial;
use bevy::prelude::AlphaMode;
use bevy::prelude::Handle;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::ShaderType;
use bevy::shader::ShaderRef;
use bevy_kana::ToF32;

use super::constants::SHADOW_PROXY_ALPHA_MASK_THRESHOLD;
use crate::constants::EMBEDDED_GLYPH_TEXT_SHADER_PATH;
use crate::text::DistanceField;

/// The full MSDF text material type: `StandardMaterial` extended with MSDF
/// atlas decoding.
///
/// Use [`glyph_material`] or [`glyph_shadow_proxy_material`] to create
/// instances. The `base` field exposes all `StandardMaterial` properties
/// (metallic, roughness, emissive, `double_sided`, etc.) for full PBR control.
pub(super) type GlyphMaterial = ExtendedMaterial<StandardMaterial, GlyphMaterialExtension>;

/// Uniform data for the MSDF extension shader.
#[derive(Clone, Debug, ShaderType)]
pub(super) struct GlyphMaterialUniform {
    /// SDF range in atlas pixels.
    pub sdf_range:        f32,
    /// Atlas texture width in pixels.
    pub atlas_width:      f32,
    /// Atlas texture height in pixels.
    pub atlas_height:     f32,
    /// Hue rotation applied to every vertex color, in radians (0.0 = none).
    ///
    /// Rotates the hue of all vertex colors in the mesh by this angle using
    /// Rodrigues' rotation in RGB space. The rotation is performed entirely
    /// on the GPU — changing this value has zero CPU cost and does not
    /// trigger mesh rebuilds or change detection.
    ///
    /// The rotation is relative to whatever vertex colors are already baked
    /// into the mesh. A value of `TAU / 3` (~2.09) shifts reds to greens,
    /// greens to blues, blues to reds. A full `TAU` (6.28) completes the
    /// cycle back to the original colors.
    ///
    /// Example uses:
    /// - Scrolling a rainbow color scheme across text by incrementing the offset each frame.
    /// - Pulsing or cycling a highlight color on selected text.
    /// - Theming — shifting all text toward a warm or cool palette without rebuilding the layout
    ///   tree or mesh.
    /// - Damage/status effects — temporarily shifting text hue to indicate state changes in-game.
    ///
    /// Has no effect on text using the material's base `color` uniform
    /// (white vertex colors). Only affects text with per-vertex colors set
    /// via [`TextConfig::with_color`](crate::TextConfig::with_color).
    pub hue_offset:       f32,
    /// Glyph render mode (maps to [`GlyphRenderMode`](crate::GlyphRenderMode)):
    /// 0 = Text, 1 = `PunchOut`, 2 = `SolidQuad`.
    pub render_mode:      u32,
    /// Whether this material is a shadow proxy (invisible in main pass,
    /// contributes to the shadow prepass). 0 = visible, 1 = shadow-only.
    pub is_shadow_proxy:  u32,
    /// Distance-field variant the atlas was rasterized with:
    /// 0 = MSDF (median of RGB), 1 = SDF (R only). Built from a typed
    /// [`DistanceField`] at the helper boundary so spawn sites never
    /// touch raw discriminants.
    pub distance_field:   u32,
    /// Clip rect in panel-local Y-up space: [left, bottom, right, top].
    /// Fragments outside are discarded. Defaults to large bounds (no clip).
    pub clip_rect:        Vec4,
    /// Depth offset added to `position.z` before OIT fragment storage.
    pub oit_depth_offset: f32,
}

/// MSDF atlas extension for `StandardMaterial`.
///
/// Adds MSDF glyph decoding on top of Bevy's PBR pipeline. The extension
/// shader reads the MSDF atlas texture, computes per-pixel alpha from the
/// signed distance field, and modifies the PBR input's base color before
/// lighting is applied.
#[derive(Asset, AsBindGroup, Clone, Debug, TypePath)]
pub(super) struct GlyphMaterialExtension {
    /// MSDF shader uniforms.
    #[uniform(100)]
    pub uniforms:      GlyphMaterialUniform,
    /// The MSDF atlas texture.
    #[texture(101)]
    #[sampler(102)]
    pub atlas_texture: Handle<Image>,
}

impl MaterialExtension for GlyphMaterialExtension {
    fn fragment_shader() -> ShaderRef { EMBEDDED_GLYPH_TEXT_SHADER_PATH.into() }

    /// Use the same MSDF shader for the depth/shadow prepass so that
    /// `AlphaMode::Mask` can do per-pixel alpha testing via the MSDF
    /// atlas. Without this, the prepass uses the default
    /// `StandardMaterial` behavior and all shadows are rectangular.
    fn prepass_fragment_shader() -> ShaderRef { EMBEDDED_GLYPH_TEXT_SHADER_PATH.into() }
}

/// Inputs for a visible glyph text material.
pub(super) struct GlyphMaterialInput {
    pub base:             StandardMaterial,
    pub sdf_range:        f32,
    pub atlas_dimensions: UVec2,
    pub atlas_texture:    Handle<Image>,
    pub hue_offset:       f32,
    pub render_mode:      u32,
    pub distance_field:   DistanceField,
    pub clip_rect:        Vec4,
    pub oit_depth_offset: f32,
    pub alpha_mode:       AlphaMode,
}

/// Inputs for a shadow-proxy glyph text material.
pub(super) struct GlyphShadowProxyMaterialInput {
    pub base:             StandardMaterial,
    pub sdf_range:        f32,
    pub atlas_dimensions: UVec2,
    pub atlas_texture:    Handle<Image>,
    pub hue_offset:       f32,
    pub render_mode:      u32,
    pub distance_field:   DistanceField,
    pub clip_rect:        Vec4,
    pub oit_depth_offset: f32,
}

/// Creates a new [`GlyphMaterial`] from a resolved base `StandardMaterial`.
///
/// The base material's `alpha_mode` is overridden with the resolved
/// [`AlphaMode`] from `input`. Other PBR properties, including
/// sidedness/culling, are preserved from the caller.
#[must_use]
pub(super) fn glyph_material(input: GlyphMaterialInput) -> GlyphMaterial {
    let GlyphMaterialInput {
        mut base,
        sdf_range,
        atlas_dimensions,
        atlas_texture,
        hue_offset,
        render_mode,
        distance_field,
        clip_rect,
        oit_depth_offset,
        alpha_mode,
    } = input;
    base.alpha_mode = alpha_mode;
    build_glyph_material(
        base,
        sdf_range,
        atlas_dimensions,
        atlas_texture,
        hue_offset,
        render_mode,
        distance_field,
        clip_rect,
        oit_depth_offset,
        0,
    )
}

/// Creates a shadow proxy [`GlyphMaterial`] from a resolved base.
///
/// Same as [`glyph_material`] but configured for shadow-only rendering:
/// - `alpha_mode: Mask(SHADOW_PROXY_ALPHA_MASK_THRESHOLD)` so the shadow prepass runs the fragment
///   shader
/// - `is_shadow_proxy: 1` causes the main-pass fragment shader to discard all fragments
#[must_use]
pub(super) fn glyph_shadow_proxy_material(input: GlyphShadowProxyMaterialInput) -> GlyphMaterial {
    let GlyphShadowProxyMaterialInput {
        mut base,
        sdf_range,
        atlas_dimensions,
        atlas_texture,
        hue_offset,
        render_mode,
        distance_field,
        clip_rect,
        oit_depth_offset,
    } = input;
    base.alpha_mode = AlphaMode::Mask(SHADOW_PROXY_ALPHA_MASK_THRESHOLD);
    build_glyph_material(
        base,
        sdf_range,
        atlas_dimensions,
        atlas_texture,
        hue_offset,
        render_mode,
        distance_field,
        clip_rect,
        oit_depth_offset,
        1,
    )
}

fn build_glyph_material(
    base: StandardMaterial,
    sdf_range: f32,
    atlas_dimensions: UVec2,
    atlas_texture: Handle<Image>,
    hue_offset: f32,
    render_mode: u32,
    distance_field: DistanceField,
    clip_rect: Vec4,
    oit_depth_offset: f32,
    is_shadow_proxy: u32,
) -> GlyphMaterial {
    ExtendedMaterial {
        base,
        extension: GlyphMaterialExtension {
            uniforms: GlyphMaterialUniform {
                sdf_range,
                atlas_width: atlas_dimensions.x.to_f32(),
                atlas_height: atlas_dimensions.y.to_f32(),
                hue_offset,
                render_mode,
                is_shadow_proxy,
                distance_field: u32::from(distance_field),
                clip_rect,
                oit_depth_offset,
            },
            atlas_texture,
        },
    }
}

#[cfg(test)]
mod tests {
    use bevy::render::render_resource::ShaderType;

    use super::*;

    #[test]
    fn uniform_layout_round_trips_through_shader_type() {
        // Sanity-check that adding the `distance_field` field hasn't
        // broken the WGSL-side struct layout assumption. encase derives
        // std140 padding via `ShaderType`, and `min_size()` is what the
        // WGSL struct must match.
        let size = <GlyphMaterialUniform as ShaderType>::min_size().get();
        assert!(
            size >= 48,
            "uniform should be at least 48 bytes, got {size}"
        );
        assert_eq!(
            size % 16,
            0,
            "std140 size must be 16-byte aligned, got {size}"
        );
    }
}
