use bevy::asset::Asset;
use bevy::asset::embedded_asset;
use bevy::color::Color;
use bevy::color::LinearRgba;
use bevy::math::Vec4;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MaterialExtension;
use bevy::pbr::MaterialPlugin;
use bevy::pbr::StandardMaterial;
use bevy::prelude::App;
use bevy::prelude::Handle;
use bevy::prelude::Plugin;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::ShaderType;
use bevy::render::storage::ShaderStorageBuffer;
use bevy::shader::ShaderRef;

use super::backend::SlugBackend;
use super::constants::SLUG_TEXT_SHADER_PATH;

/// Visible render mode for the isolated Slug shader path.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u32)]
pub enum SlugRenderMode {
    /// No visible pass. The caller skips spawning visible geometry.
    Invisible = 0,
    /// Normal Slug coverage fill.
    #[default]
    Text      = 1,
    /// Inverted Slug coverage inside each glyph quad.
    PunchOut  = 2,
    /// Solid glyph bounds quads without curve evaluation.
    SolidQuad = 3,
}

impl From<SlugRenderMode> for u32 {
    fn from(mode: SlugRenderMode) -> Self { mode as Self }
}

/// Material used by the isolated Slug shader spike.
pub type SlugTextMaterial = ExtendedMaterial<StandardMaterial, SlugTextExtension>;

/// Registers the isolated Slug shader and material type.
pub struct SlugTextSpikePlugin;

impl Plugin for SlugTextSpikePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/slug_text.wgsl");
        app.init_resource::<SlugBackend>();
        app.add_plugins(MaterialPlugin::<SlugTextMaterial>::default());
    }
}

/// Uniforms consumed by the Slug shader spike.
#[derive(Clone, Debug, ShaderType)]
pub struct SlugTextUniform {
    /// Linear fill color.
    pub fill_color:  Vec4,
    /// Visible render mode for this pass.
    pub render_mode: u32,
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
    /// Unique glyph records for this run.
    #[storage(103, read_only)]
    pub glyphs:   Handle<ShaderStorageBuffer>,
}

impl MaterialExtension for SlugTextExtension {
    fn fragment_shader() -> ShaderRef { SLUG_TEXT_SHADER_PATH.into() }

    fn prepass_fragment_shader() -> ShaderRef { SLUG_TEXT_SHADER_PATH.into() }
}

/// Inputs for one Slug spike material instance.
pub struct SlugTextMaterialInput {
    /// Base material settings.
    pub base:        StandardMaterial,
    /// Fill color.
    pub fill_color:  Color,
    /// Visible render mode.
    pub render_mode: SlugRenderMode,
    /// Band-packed quadratic curve records.
    pub curves:      Handle<ShaderStorageBuffer>,
    /// Horizontal band records.
    pub bands:       Handle<ShaderStorageBuffer>,
    /// Unique glyph records.
    pub glyphs:      Handle<ShaderStorageBuffer>,
}

/// Creates a `SlugTextMaterial` for the isolated feasibility shader.
#[must_use]
pub fn slug_text_material(input: SlugTextMaterialInput) -> SlugTextMaterial {
    let SlugTextMaterialInput {
        mut base,
        fill_color,
        render_mode,
        curves,
        bands,
        glyphs,
    } = input;
    base.unlit = true;

    let linear: LinearRgba = fill_color.into();
    ExtendedMaterial {
        base,
        extension: SlugTextExtension {
            uniforms: SlugTextUniform {
                fill_color:  Vec4::new(linear.red, linear.green, linear.blue, linear.alpha),
                render_mode: u32::from(render_mode),
            },
            curves,
            bands,
            glyphs,
        },
    }
}
