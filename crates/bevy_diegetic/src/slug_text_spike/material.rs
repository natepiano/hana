use bevy::asset::Asset;
use bevy::asset::embedded_asset;
use bevy::color::Color;
use bevy::color::LinearRgba;
use bevy::math::Vec4;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MaterialExtension;
use bevy::pbr::StandardMaterial;
use bevy::prelude::App;
use bevy::prelude::Handle;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::ShaderType;
use bevy::render::storage::ShaderStorageBuffer;
use bevy::shader::ShaderRef;

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

/// Registers the embedded Slug text shader.
///
/// `embedded_asset!` resolves its path from the file it is invoked in, and
/// the shader still lives in this module, so the registration stays here.
/// [`TextPlugin`](crate::text::TextPlugin) calls this during setup, next to
/// the `SlugBackend` and material-plugin init it now owns. When the slug
/// files move under `text/slug/` (Phase 4 of the slug migration), this
/// folds directly into `TextPlugin::build`.
pub(crate) fn register_slug_text_shader(app: &mut App) {
    embedded_asset!(app, "shaders/slug_text.wgsl");
}

/// Uniforms consumed by the Slug shader spike.
#[derive(Clone, Debug, ShaderType)]
pub struct SlugTextUniform {
    /// Linear fill color.
    pub fill_color:      Vec4,
    /// Visible render mode for this pass.
    pub render_mode:     u32,
    /// Shadow-proxy flag: `0` renders normally; `1` discards every fragment
    /// in the main pass while still writing its coverage silhouette in the
    /// depth/shadow prepass, so the glyph casts its silhouette shadow
    /// without painting a second visible copy.
    pub is_shadow_proxy: u32,
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

/// Creates a visible `SlugTextMaterial`.
#[must_use]
pub fn slug_text_material(input: SlugTextMaterialInput) -> SlugTextMaterial {
    build_slug_text_material(input, 0)
}

/// Creates a shadow-proxy `SlugTextMaterial`: invisible in the main pass,
/// but its coverage silhouette still writes depth in the prepass so the
/// glyph casts a shadow without a second visible copy. The caller supplies
/// the `AlphaMode::Mask` base so the prepass runs this fragment shader.
#[must_use]
pub fn slug_text_shadow_proxy_material(input: SlugTextMaterialInput) -> SlugTextMaterial {
    build_slug_text_material(input, 1)
}

fn build_slug_text_material(
    input: SlugTextMaterialInput,
    is_shadow_proxy: u32,
) -> SlugTextMaterial {
    let SlugTextMaterialInput {
        base,
        fill_color,
        render_mode,
        curves,
        bands,
        glyphs,
    } = input;
    let linear: LinearRgba = fill_color.into();
    ExtendedMaterial {
        base,
        extension: SlugTextExtension {
            uniforms: SlugTextUniform {
                fill_color: Vec4::new(linear.red, linear.green, linear.blue, linear.alpha),
                render_mode: u32::from(render_mode),
                is_shadow_proxy,
            },
            curves,
            bands,
            glyphs,
        },
    }
}
