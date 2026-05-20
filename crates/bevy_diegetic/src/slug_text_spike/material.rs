use bevy::asset::Asset;
use bevy::asset::embedded_asset;
use bevy::color::Color;
use bevy::color::LinearRgba;
use bevy::math::Vec2;
use bevy::math::Vec4;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MaterialExtension;
use bevy::pbr::MaterialPlugin;
use bevy::pbr::StandardMaterial;
use bevy::prelude::AlphaMode;
use bevy::prelude::App;
use bevy::prelude::Handle;
use bevy::prelude::Plugin;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::ShaderType;
use bevy::render::storage::ShaderStorageBuffer;
use bevy::shader::ShaderRef;
use bevy_kana::ToU32;

use super::constants::SLUG_TEXT_SHADER_PATH;
use super::geometry::SlugBounds;

/// Material used by the isolated Slug shader spike.
pub type SlugTextMaterial = ExtendedMaterial<StandardMaterial, SlugTextExtension>;

/// Registers the isolated Slug shader and material type.
pub struct SlugTextSpikePlugin;

impl Plugin for SlugTextSpikePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/slug_text.wgsl");
        app.add_plugins(MaterialPlugin::<SlugTextMaterial>::default());
    }
}

/// Uniforms consumed by the Slug shader spike.
#[derive(Clone, Debug, ShaderType)]
pub struct SlugTextUniform {
    /// Glyph bounds minimum in font design-space units.
    pub bounds_min:  Vec2,
    /// Glyph bounds size in font design-space units.
    pub bounds_size: Vec2,
    /// Linear fill color.
    pub fill_color:  Vec4,
    /// Number of bands in the band buffer.
    pub band_count:  u32,
}

/// Slug material extension over `StandardMaterial`.
#[derive(Asset, AsBindGroup, Clone, Debug, TypePath)]
pub struct SlugTextExtension {
    /// Shader uniforms.
    #[uniform(100)]
    pub uniforms: SlugTextUniform,
    /// Band-packed quadratic curve records.
    #[storage(101, read_only)]
    pub curves:   Handle<ShaderStorageBuffer>,
    /// Horizontal band records.
    #[storage(102, read_only)]
    pub bands:    Handle<ShaderStorageBuffer>,
}

impl MaterialExtension for SlugTextExtension {
    fn fragment_shader() -> ShaderRef { SLUG_TEXT_SHADER_PATH.into() }

    fn prepass_fragment_shader() -> ShaderRef { SLUG_TEXT_SHADER_PATH.into() }
}

/// Inputs for one Slug spike material instance.
pub struct SlugTextMaterialInput {
    /// Base material settings.
    pub base:       StandardMaterial,
    /// Glyph bounds in font design-space units.
    pub bounds:     SlugBounds,
    /// Fill color.
    pub fill_color: Color,
    /// Band-packed quadratic curve records.
    pub curves:     Handle<ShaderStorageBuffer>,
    /// Horizontal band records.
    pub bands:      Handle<ShaderStorageBuffer>,
    /// Number of bands in `bands`.
    pub band_count: usize,
}

/// Creates a `SlugTextMaterial` for the isolated feasibility shader.
#[must_use]
pub fn slug_text_material(input: SlugTextMaterialInput) -> SlugTextMaterial {
    let SlugTextMaterialInput {
        mut base,
        bounds,
        fill_color,
        curves,
        bands,
        band_count,
    } = input;
    base.alpha_mode = AlphaMode::Mask(0.5);
    base.unlit = true;

    let linear: LinearRgba = fill_color.into();
    ExtendedMaterial {
        base,
        extension: SlugTextExtension {
            uniforms: SlugTextUniform {
                bounds_min:  bounds.min,
                bounds_size: Vec2::new(bounds.width(), bounds.height()),
                fill_color:  Vec4::new(linear.red, linear.green, linear.blue, linear.alpha),
                band_count:  band_count.to_u32(),
            },
            curves,
            bands,
        },
    }
}
