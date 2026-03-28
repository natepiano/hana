//! SDF panel material — extends `StandardMaterial` with rounded rectangle
//! rendering via signed distance fields.
//!
//! Uses Bevy's [`ExtendedMaterial`] to layer SDF panel rendering on top of
//! the full PBR pipeline. This gives panels correct lighting, shadows, and
//! all `StandardMaterial` properties for free.

use bevy::asset::Asset;
use bevy::color::Color;
use bevy::color::LinearRgba;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MaterialExtension;
use bevy::pbr::StandardMaterial;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::ShaderType;
use bevy::shader::ShaderRef;

/// The full SDF panel material type: `StandardMaterial` extended with
/// SDF rounded rectangle rendering.
pub(super) type SdfPanelMaterial = ExtendedMaterial<StandardMaterial, SdfPanelExtension>;

/// Uniform data for the SDF panel extension shader.
#[derive(Clone, Debug, ShaderType)]
pub(super) struct SdfPanelUniform {
    /// Half-size of the element in world units (width/2, height/2).
    pub half_size:     bevy::math::Vec2,
    /// Per-corner radii in world units: [TL, TR, BR, BL].
    pub corner_radii:  bevy::math::Vec4,
    /// Border widths in world units: [top, right, bottom, left].
    pub border_widths: bevy::math::Vec4,
    /// Border color in linear RGBA.
    pub border_color:  bevy::math::Vec4,
}

/// SDF panel extension for `StandardMaterial`.
///
/// Adds SDF rounded rectangle rendering on top of Bevy's PBR pipeline.
/// The extension shader computes per-fragment alpha from the signed
/// distance field and composites fill + border colors before lighting.
#[derive(Asset, AsBindGroup, Clone, Debug, TypePath)]
pub(super) struct SdfPanelExtension {
    /// SDF shader uniforms.
    #[uniform(100)]
    pub uniforms: SdfPanelUniform,
}

impl MaterialExtension for SdfPanelExtension {
    fn fragment_shader() -> ShaderRef { "shaders/sdf_panel.wgsl".into() }

    /// Use the SDF shader for the depth/shadow prepass so that rounded
    /// shapes clip correctly in shadows.
    fn prepass_fragment_shader() -> ShaderRef { "shaders/sdf_panel.wgsl".into() }
}

/// Creates a new [`SdfPanelMaterial`] from a resolved base `StandardMaterial`.
///
/// The base material's PBR properties (roughness, metallic, reflectance,
/// base_color) are preserved. `alpha_mode`, `double_sided`, and `cull_mode`
/// are overridden for panel rendering.
#[must_use]
pub(super) fn sdf_panel_material(
    mut base: StandardMaterial,
    half_width: f32,
    half_height: f32,
    corner_radii: [f32; 4],
    border_widths: [f32; 4],
    border_color: Option<Color>,
) -> SdfPanelMaterial {
    base.double_sided = true;
    base.cull_mode = None;
    // SDF provides its own per-fragment alpha — always use Blend.
    base.alpha_mode = bevy::prelude::AlphaMode::Blend;

    let border_linear: bevy::math::Vec4 = border_color.map_or(bevy::math::Vec4::ZERO, |c| {
        let l: LinearRgba = c.into();
        bevy::math::Vec4::new(l.red, l.green, l.blue, l.alpha)
    });

    ExtendedMaterial {
        base,
        extension: SdfPanelExtension {
            uniforms: SdfPanelUniform {
                half_size:     bevy::math::Vec2::new(half_width, half_height),
                corner_radii:  bevy::math::Vec4::from_array(corner_radii),
                border_widths: bevy::math::Vec4::from_array(border_widths),
                border_color:  border_linear,
            },
        },
    }
}
