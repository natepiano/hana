//! MSDF text material for GPU rendering.

use bevy::asset::Asset;
use bevy::color::LinearRgba;
use bevy::image::Image;
use bevy::pbr::Material;
use bevy::prelude::Handle;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::ShaderType;

/// Uniform data for the MSDF text shader.
#[derive(Clone, Debug, ShaderType)]
pub struct MsdfTextUniform {
    /// Base text color (linear RGBA).
    pub color:        LinearRgba,
    /// SDF range in atlas pixels.
    pub sdf_range:    f32,
    /// Atlas texture width in pixels.
    pub atlas_width:  f32,
    /// Atlas texture height in pixels.
    pub atlas_height: f32,
    /// Padding for 16-byte alignment.
    _padding:         f32,
}

/// Material for rendering MSDF text glyphs.
///
/// Implements Bevy's [`Material`] trait with a custom fragment shader
/// that performs MSDF decoding and adaptive anti-aliasing.
#[derive(Asset, AsBindGroup, Clone, Debug, TypePath)]
pub struct MsdfTextMaterial {
    /// Shader uniforms: color, SDF range, atlas dimensions.
    #[uniform(0)]
    pub uniforms:      MsdfTextUniform,
    /// The MSDF atlas texture.
    #[texture(1)]
    #[sampler(2)]
    pub atlas_texture: Handle<Image>,
}

impl MsdfTextMaterial {
    /// Creates a new material with the given color and atlas texture.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub const fn new(
        color: LinearRgba,
        sdf_range: f32,
        atlas_width: u32,
        atlas_height: u32,
        atlas_texture: Handle<Image>,
    ) -> Self {
        Self {
            uniforms: MsdfTextUniform {
                color,
                sdf_range,
                atlas_width: atlas_width as f32,
                atlas_height: atlas_height as f32,
                _padding: 0.0,
            },
            atlas_texture,
        }
    }
}

impl Material for MsdfTextMaterial {
    fn fragment_shader() -> bevy::shader::ShaderRef { "shaders/msdf_text.wgsl".into() }

    fn alpha_mode(&self) -> bevy::prelude::AlphaMode { bevy::prelude::AlphaMode::Blend }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        // Disable back-face culling so text is visible from both sides.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}
