//! SDF panel material — extends `StandardMaterial` with rounded rectangle
//! rendering via signed distance fields.
//!
//! Uses Bevy's [`ExtendedMaterial`] to layer SDF panel rendering on top of
//! the full PBR pipeline. This gives panels correct lighting, shadows, and
//! all `StandardMaterial` properties for free.

use bevy::asset::Asset;
use bevy::color::Alpha;
use bevy::color::Color;
use bevy::color::LinearRgba;
use bevy::math::Vec2;
use bevy::math::Vec4;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MaterialExtension;
use bevy::pbr::StandardMaterial;
use bevy::prelude::AlphaMode;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::ShaderType;
use bevy::shader::ShaderRef;

use crate::constants::EMBEDDED_SDF_PANEL_SHADER_PATH;

/// The full SDF panel material type: `StandardMaterial` extended with
/// SDF rounded rectangle rendering.
pub(crate) type SdfPanelMaterial = ExtendedMaterial<StandardMaterial, SdfPanelExtension>;

/// Uniform data for the SDF panel extension shader.
#[derive(Clone, Debug, ShaderType)]
pub(crate) struct SdfPanelUniform {
    /// Half-size of the SDF form in world units (width/2, height/2).
    pub half_size:        Vec2,
    /// Half-size of the mesh quad in world units. Larger than `half_size`
    /// by the AA padding so the exterior anti-aliasing ramp has fragments
    /// to render on.
    pub mesh_half_size:   Vec2,
    /// Per-corner radii in world units: [TL, TR, BR, BL].
    pub corner_radii:     Vec4,
    /// Border widths in world units: [top, right, bottom, left].
    pub border_widths:    Vec4,
    /// Border color in linear RGBA.
    pub border_color:     Vec4,
    /// SDF selector. `0` = rounded rect, `1` = triangle, `2` = circle,
    /// `3` = diamond, `4` = line segment, `5..=7` = oriented cap forms.
    pub sdf_kind:         u32,
    /// Extra parameters for custom SDF forms.
    pub sdf_params:       Vec4,
    /// Alpha of the fill/base color. Used by the shadow prepass to
    /// distinguish filled surfaces from border-only rings.
    pub fill_alpha:       f32,
    /// Clip rect in local quad space: `[left, bottom, right, top]`.
    /// Fragments outside this rect are discarded. Defaults to the full
    /// quad bounds (`[-half_w, -half_h, half_w, half_h]`) when no clip
    /// is active.
    pub clip_rect:        Vec4,
    /// Depth offset added to `position.z` before OIT fragment storage.
    /// Separates coplanar layers in the OIT linked list so the resolve
    /// pass composites them in the correct painter's order.
    /// Higher values = closer to camera (reverse-Z).
    pub oit_depth_offset: f32,
}

/// SDF panel extension for `StandardMaterial`.
///
/// Adds SDF rounded rectangle rendering on top of Bevy's PBR pipeline.
/// The extension shader computes per-fragment alpha from the signed
/// distance field and composites fill + border colors before lighting.
#[derive(Asset, AsBindGroup, Clone, Debug, TypePath)]
pub(crate) struct SdfPanelExtension {
    /// SDF shader uniforms.
    #[uniform(100)]
    pub uniforms: SdfPanelUniform,
}

impl MaterialExtension for SdfPanelExtension {
    fn fragment_shader() -> ShaderRef { EMBEDDED_SDF_PANEL_SHADER_PATH.into() }

    /// Use the SDF shader for the depth/shadow prepass so rounded corners clip
    /// correctly in shadows.
    fn prepass_fragment_shader() -> ShaderRef { EMBEDDED_SDF_PANEL_SHADER_PATH.into() }
}

/// Rust-side selector for the SDF form used by the panel shader.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SdfPrimitiveKind {
    #[default]
    RoundedRect,
    Triangle,
    Circle,
    Diamond,
}

impl From<SdfPrimitiveKind> for u32 {
    fn from(sdf_primitive_kind: SdfPrimitiveKind) -> Self {
        match sdf_primitive_kind {
            SdfPrimitiveKind::RoundedRect => 0,
            SdfPrimitiveKind::Triangle => 1,
            SdfPrimitiveKind::Circle => 2,
            SdfPrimitiveKind::Diamond => 3,
        }
    }
}

/// Inputs for a rounded-rectangle panel material.
pub(crate) struct SdfPanelMaterialInput {
    pub half_size:        Vec2,
    pub mesh_half_size:   Vec2,
    pub corner_radii:     [f32; 4],
    pub border_widths:    [f32; 4],
    pub border_color:     Option<Color>,
    pub clip_rect:        Vec4,
    pub oit_depth_offset: f32,
}

/// Inputs for a non-rectangular SDF material.
pub(crate) struct SdfPrimitiveMaterialInput {
    pub half_size:          Vec2,
    pub mesh_half_size:     Vec2,
    pub corner_radii:       [f32; 4],
    pub border_widths:      [f32; 4],
    pub border_color:       Option<Color>,
    pub sdf_primitive_kind: SdfPrimitiveKind,
    pub params:             Vec4,
    pub clip_rect:          Vec4,
    pub oit_depth_offset:   f32,
}

/// Creates a new [`SdfPanelMaterial`] from a resolved base `StandardMaterial`.
///
/// The base material's PBR properties (roughness, metallic, reflectance,
/// `base_color`) are preserved. `alpha_mode`, `double_sided`, and `cull_mode`
/// are overridden for panel rendering.
#[must_use]
pub(crate) fn sdf_panel_material(
    base: StandardMaterial,
    input: SdfPanelMaterialInput,
) -> SdfPanelMaterial {
    sdf_primitive_material(
        base,
        SdfPrimitiveMaterialInput {
            half_size:          input.half_size,
            mesh_half_size:     input.mesh_half_size,
            corner_radii:       input.corner_radii,
            border_widths:      input.border_widths,
            border_color:       input.border_color,
            sdf_primitive_kind: SdfPrimitiveKind::RoundedRect,
            params:             Vec4::ZERO,
            clip_rect:          input.clip_rect,
            oit_depth_offset:   input.oit_depth_offset,
        },
    )
}

/// Creates a new [`SdfPanelMaterial`] with an explicit SDF selector.
#[must_use]
pub(crate) fn sdf_primitive_material(
    mut base: StandardMaterial,
    input: SdfPrimitiveMaterialInput,
) -> SdfPanelMaterial {
    base.double_sided = true;
    base.cull_mode = None;
    // SDF provides its own per-fragment alpha — always use Blend.
    base.alpha_mode = AlphaMode::Blend;
    let fill_alpha = base.base_color.alpha();

    let border_linear: Vec4 = input.border_color.map_or(Vec4::ZERO, |color| {
        let linear: LinearRgba = color.into();
        Vec4::new(linear.red, linear.green, linear.blue, linear.alpha)
    });

    ExtendedMaterial {
        base,
        extension: SdfPanelExtension {
            uniforms: SdfPanelUniform {
                half_size: input.half_size,
                mesh_half_size: input.mesh_half_size,
                corner_radii: Vec4::from_array(input.corner_radii),
                border_widths: Vec4::from_array(input.border_widths),
                border_color: border_linear,
                sdf_kind: input.sdf_primitive_kind.into(),
                sdf_params: input.params,
                fill_alpha,
                clip_rect: input.clip_rect,
                oit_depth_offset: input.oit_depth_offset,
            },
        },
    }
}
