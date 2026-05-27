use bevy::asset::Asset;
use bevy::color::Color;
use bevy::color::LinearRgba;
use bevy::math::Vec4;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MaterialExtension;
use bevy::pbr::StandardMaterial;
use bevy::prelude::Handle;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::ShaderType;
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;

use super::constants::SLUG_TEXT_SHADER_PATH;

/// Visible render mode for the text shader.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u32)]
pub(crate) enum RenderMode {
    /// Normal coverage fill.
    #[default]
    Text     = 1,
    /// Inverted coverage inside each glyph quad.
    PunchOut = 2,
}

impl From<RenderMode> for u32 {
    fn from(mode: RenderMode) -> Self { mode as Self }
}

/// Material used by the text renderer.
pub(crate) type TextMaterial = ExtendedMaterial<StandardMaterial, TextExtension>;

/// Uniforms consumed by the text shader.
#[derive(Clone, Debug, ShaderType)]
pub struct TextUniform {
    /// Linear fill color.
    pub fill_color:       Vec4,
    /// Visible render mode for this pass.
    pub render_mode:      u32,
    /// Per-layer depth offset applied to the OIT fragment position for coplanar
    /// layer ordering.
    pub oit_depth_offset: f32,
    /// Non-zero enables sub-pixel supersampling of glyph coverage (anti-aliases
    /// grazing-angle edges without MSAA).
    pub supersample:      u32,
}

/// Text material extension over `StandardMaterial`.
#[derive(Asset, AsBindGroup, Clone, Debug, TypePath)]
pub struct TextExtension {
    /// Shader uniforms.
    #[uniform(100)]
    pub uniforms: TextUniform,
    /// Band-packed quadratic curve records.
    #[storage(101, read_only)]
    pub curves:   Handle<ShaderBuffer>,
    /// Horizontal band records.
    #[storage(102, read_only)]
    pub bands:    Handle<ShaderBuffer>,
    /// Unique glyph records for this run.
    #[storage(103, read_only)]
    pub glyphs:   Handle<ShaderBuffer>,
}

impl MaterialExtension for TextExtension {
    fn fragment_shader() -> ShaderRef { SLUG_TEXT_SHADER_PATH.into() }

    fn prepass_fragment_shader() -> ShaderRef { SLUG_TEXT_SHADER_PATH.into() }
}

/// Inputs for one text material instance.
pub(crate) struct TextMaterialInput {
    /// Base material settings.
    pub base:             StandardMaterial,
    /// Fill color.
    pub fill_color:       Color,
    /// Visible render mode.
    pub render_mode:      RenderMode,
    /// Per-layer depth offset for coplanar OIT layer ordering.
    pub oit_depth_offset: f32,
    /// Band-packed quadratic curve records.
    pub curves:           Handle<ShaderBuffer>,
    /// Horizontal band records.
    pub bands:            Handle<ShaderBuffer>,
    /// Unique glyph records.
    pub glyphs:           Handle<ShaderBuffer>,
}

/// Creates a `TextMaterial` from one run's color, render mode, and
/// band-packed curve/band/glyph buffers.
#[must_use]
pub(crate) fn text_material(input: TextMaterialInput) -> TextMaterial {
    let TextMaterialInput {
        base,
        fill_color,
        render_mode,
        oit_depth_offset,
        curves,
        bands,
        glyphs,
    } = input;
    let linear: LinearRgba = fill_color.into();
    ExtendedMaterial {
        base,
        extension: TextExtension {
            uniforms: TextUniform {
                fill_color: Vec4::new(linear.red, linear.green, linear.blue, linear.alpha),
                render_mode: u32::from(render_mode),
                oit_depth_offset,
                supersample: 1,
            },
            curves,
            bands,
            glyphs,
        },
    }
}
