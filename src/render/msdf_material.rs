//! MSDF text material — extends `StandardMaterial` with MSDF atlas decoding.
//!
//! Uses Bevy's [`ExtendedMaterial`] to layer MSDF glyph rendering on top of the
//! full PBR pipeline. This gives text correct lighting, shadows, double-sided
//! normals, and all `StandardMaterial` properties for free.

use bevy::asset::Asset;
use bevy::image::Image;
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

/// The full MSDF text material type: `StandardMaterial` extended with MSDF
/// atlas decoding.
///
/// Use `MsdfTextMaterial::new(...)` to create instances. The `base` field
/// exposes all `StandardMaterial` properties (metallic, roughness, emissive,
/// `double_sided`, etc.) for full PBR control.
pub(super) type MsdfTextMaterial = ExtendedMaterial<StandardMaterial, MsdfExtension>;

/// Uniform data for the MSDF extension shader.
#[derive(Clone, Debug, ShaderType)]
pub(super) struct MsdfTextUniform {
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
pub(super) struct MsdfExtension {
    /// MSDF shader uniforms.
    #[uniform(100)]
    pub uniforms:      MsdfTextUniform,
    /// The MSDF atlas texture.
    #[texture(101)]
    #[sampler(102)]
    pub atlas_texture: Handle<Image>,
}

impl MaterialExtension for MsdfExtension {
    fn fragment_shader() -> ShaderRef { "shaders/msdf_text.wgsl".into() }

    /// Use the same MSDF shader for the depth/shadow prepass so that
    /// `AlphaMode::Mask` can do per-pixel alpha testing via the MSDF
    /// atlas. Without this, the prepass uses the default
    /// `StandardMaterial` behavior and all shadows are rectangular.
    fn prepass_fragment_shader() -> ShaderRef { "shaders/msdf_text.wgsl".into() }
}

/// Creates a new [`MsdfTextMaterial`] from a resolved base `StandardMaterial`.
///
/// The base material's `alpha_mode`, `double_sided`, and `cull_mode` are
/// overridden for text rendering. All other PBR properties (roughness,
/// metallic, reflectance, `base_color`, unlit) are preserved from the caller.
#[must_use]
pub(super) fn msdf_text_material(
    mut base: StandardMaterial,
    sdf_range: f32,
    atlas_width: u32,
    atlas_height: u32,
    atlas_texture: Handle<Image>,
    hue_offset: f32,
    render_mode: u32,
    clip_rect: Vec4,
    oit_depth_offset: f32,
) -> MsdfTextMaterial {
    base.alpha_mode = AlphaMode::Blend;
    base.double_sided = true;
    base.cull_mode = None;

    ExtendedMaterial {
        base,
        extension: MsdfExtension {
            uniforms: MsdfTextUniform {
                sdf_range,
                atlas_width: atlas_width.to_f32(),
                atlas_height: atlas_height.to_f32(),
                hue_offset,
                render_mode,
                is_shadow_proxy: 0,
                clip_rect,
                oit_depth_offset,
            },
            atlas_texture,
        },
    }
}

/// Creates a shadow proxy [`MsdfTextMaterial`] from a resolved base.
///
/// Same as [`msdf_text_material`] but configured for shadow-only rendering:
/// - `alpha_mode: Mask(0.5)` so the shadow prepass runs the fragment shader
/// - `is_shadow_proxy: 1` causes the main-pass fragment shader to discard all fragments
#[must_use]
pub(super) fn msdf_shadow_proxy_material(
    mut base: StandardMaterial,
    sdf_range: f32,
    atlas_width: u32,
    atlas_height: u32,
    atlas_texture: Handle<Image>,
    hue_offset: f32,
    render_mode: u32,
    clip_rect: Vec4,
    oit_depth_offset: f32,
) -> MsdfTextMaterial {
    base.alpha_mode = AlphaMode::Mask(0.5);
    base.double_sided = true;
    base.cull_mode = None;

    ExtendedMaterial {
        base,
        extension: MsdfExtension {
            uniforms: MsdfTextUniform {
                sdf_range,
                atlas_width: atlas_width.to_f32(),
                atlas_height: atlas_height.to_f32(),
                hue_offset,
                render_mode,
                is_shadow_proxy: 1,
                clip_rect,
                oit_depth_offset,
            },
            atlas_texture,
        },
    }
}
