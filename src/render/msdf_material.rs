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
    pub hue_offset:   f32,
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
                hue_offset: 0.0,
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
