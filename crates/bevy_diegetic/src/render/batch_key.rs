//! Shared visual batch-key pieces.
//!
//! Text and panel-line batching keep separate stores and source-lifecycle
//! payloads, but they split on a common subset of render compatibility:
//! authored base material identity, alpha mode, visibility layers, lighting,
//! sidedness, and shadow participation.

use std::collections::HashMap;
use std::fmt;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::hash::Hash;
use std::hash::Hasher;

use bevy::asset::AssetId;
use bevy::camera::visibility::RenderLayers;
use bevy::image::Image;
use bevy::material::OpaqueRendererMethod;
use bevy::mesh::UvChannel;
use bevy::pbr::StandardMaterial;
use bevy::prelude::AlphaMode;
use bevy::prelude::Handle;
use bevy::render::render_resource::Face;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use crate::layout::GlyphShadowMode;
use crate::layout::Lighting;
use crate::layout::Sidedness;
use crate::panel::SurfaceShadow;

/// Interned identity for an authored base material.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct BaseMaterialId(u32);

/// `AlphaMode` re-encoded so it can sit in a hash-map key: `AlphaMode` has a
/// manual `Eq` but no `Hash` (`Mask` carries an `f32`), so `Mask` stores the
/// threshold's bits here.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum BatchAlphaMode {
    Opaque,
    Mask(u32),
    Blend,
    Premultiplied,
    AlphaToCoverage,
    Add,
    Multiply,
}

/// `OpaqueRendererMethod` re-encoded for hashable pipeline compatibility.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum BatchOpaqueRendererMethod {
    Forward,
    Deferred,
    Auto,
}

impl From<OpaqueRendererMethod> for BatchOpaqueRendererMethod {
    fn from(method: OpaqueRendererMethod) -> Self {
        match method {
            OpaqueRendererMethod::Forward => Self::Forward,
            OpaqueRendererMethod::Deferred => Self::Deferred,
            OpaqueRendererMethod::Auto => Self::Auto,
        }
    }
}

/// `UvChannel` re-encoded for hashable resource compatibility.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum BatchUvChannel {
    Uv0,
    Uv1,
}

impl From<&UvChannel> for BatchUvChannel {
    fn from(channel: &UvChannel) -> Self {
        match channel {
            UvChannel::Uv0 => Self::Uv0,
            UvChannel::Uv1 => Self::Uv1,
        }
    }
}

impl From<BatchUvChannel> for UvChannel {
    fn from(channel: BatchUvChannel) -> Self {
        match channel {
            BatchUvChannel::Uv0 => Self::Uv0,
            BatchUvChannel::Uv1 => Self::Uv1,
        }
    }
}

impl From<AlphaMode> for BatchAlphaMode {
    fn from(mode: AlphaMode) -> Self {
        match mode {
            AlphaMode::Opaque => Self::Opaque,
            AlphaMode::Mask(threshold) => Self::Mask(threshold.to_bits()),
            AlphaMode::Blend => Self::Blend,
            AlphaMode::Premultiplied => Self::Premultiplied,
            AlphaMode::AlphaToCoverage => Self::AlphaToCoverage,
            AlphaMode::Add => Self::Add,
            AlphaMode::Multiply => Self::Multiply,
        }
    }
}

impl From<BatchAlphaMode> for AlphaMode {
    fn from(mode: BatchAlphaMode) -> Self {
        match mode {
            BatchAlphaMode::Opaque => Self::Opaque,
            BatchAlphaMode::Mask(bits) => Self::Mask(f32::from_bits(bits)),
            BatchAlphaMode::Blend => Self::Blend,
            BatchAlphaMode::Premultiplied => Self::Premultiplied,
            BatchAlphaMode::AlphaToCoverage => Self::AlphaToCoverage,
            BatchAlphaMode::Add => Self::Add,
            BatchAlphaMode::Multiply => Self::Multiply,
        }
    }
}

/// `RenderLayers` behind a hashable newtype: it derives `Eq` but not `Hash`.
/// Hashing the iterated layer indices is consistent with the derived
/// `PartialEq` because `RenderLayers` trims trailing empty blocks.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct BatchRenderLayers(pub RenderLayers);

impl Hash for BatchRenderLayers {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for layer in self.0.iter() {
            layer.hash(state);
        }
    }
}

impl Debug for BatchRenderLayers {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}

/// Shadow participation shared by visual batch keys.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum VisualShadow {
    Cast,
    None,
}

impl From<GlyphShadowMode> for VisualShadow {
    fn from(shadow: GlyphShadowMode) -> Self {
        match shadow {
            GlyphShadowMode::Cast => Self::Cast,
            GlyphShadowMode::None => Self::None,
        }
    }
}

impl From<SurfaceShadow> for VisualShadow {
    fn from(shadow: SurfaceShadow) -> Self {
        match shadow {
            SurfaceShadow::On => Self::Cast,
            SurfaceShadow::Off => Self::None,
        }
    }
}

/// Common render-compatibility key fields for visual batching.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VisualBatchKey {
    /// Interned authored base material.
    pub base_material: BaseMaterialId,
    /// Authored alpha/pipeline compatibility.
    pub alpha:         BatchAlphaMode,
    /// Lit/unlit compatibility.
    pub lighting:      Lighting,
    /// Culling/double-sided compatibility.
    pub sidedness:     Sidedness,
    /// Shadow-caster compatibility.
    pub shadow:        VisualShadow,
    /// Owning render layers.
    pub layers:        BatchRenderLayers,
}

/// Pipeline-specialization facts that split material-table batches.
///
/// These fields are copied from the resolved `StandardMaterial` because changing
/// them can alter Bevy PBR shader definitions, pass routing, culling, lighting,
/// or alpha behavior. Scalar/vector PBR values are stored in
/// `MaterialSlotValues`, not in this key.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PipelineCompatibility {
    /// `StandardMaterial::alpha_mode` as a hashable alpha pipeline selector.
    pub alpha:                     BatchAlphaMode,
    /// `StandardMaterial::double_sided` controls the PBR normal-flip shader flag.
    pub double_sided:              bool,
    /// `StandardMaterial::cull_mode` selects front, back, or no face culling.
    pub cull_mode:                 Option<Face>,
    /// `StandardMaterial::unlit` selects the lit or unlit PBR shader path.
    pub unlit:                     bool,
    /// `StandardMaterial::fog_enabled` selects the fog-aware shader path.
    pub fog_enabled:               bool,
    /// `StandardMaterial::opaque_render_method` selects forward or deferred opaque routing.
    pub opaque_render_method:      BatchOpaqueRendererMethod,
    /// `StandardMaterial::deferred_lighting_pass_id` selects the deferred lighting pass.
    pub deferred_lighting_pass_id: u8,
}

impl From<&StandardMaterial> for PipelineCompatibility {
    fn from(material: &StandardMaterial) -> Self {
        Self {
            alpha:                     material.alpha_mode.into(),
            double_sided:              material.double_sided,
            cull_mode:                 material.cull_mode,
            unlit:                     material.unlit,
            fog_enabled:               material.fog_enabled,
            opaque_render_method:      material.opaque_render_method.into(),
            deferred_lighting_pass_id: material.deferred_lighting_pass_id,
        }
    }
}

/// Texture and UV-channel facts that split material-table batches.
///
/// The frame material table stores scalar/vector values only. These fields
/// identify texture resources, UV-channel requirements, and shader-definition
/// requirements that must be copied into the `StandardMaterial` half of a batch
/// render material.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResourceCompatibility {
    /// `StandardMaterial::base_color_texture` bound for base-color sampling.
    pub base_color_texture:         Option<Handle<Image>>,
    /// `StandardMaterial::base_color_channel` selects the mesh UV channel for base color.
    pub base_color_channel:         BatchUvChannel,
    /// `StandardMaterial::emissive_texture` bound for emissive sampling.
    pub emissive_texture:           Option<Handle<Image>>,
    /// `StandardMaterial::emissive_channel` selects the mesh UV channel for emissive.
    pub emissive_channel:           BatchUvChannel,
    /// `StandardMaterial::metallic_roughness_texture` bound for metallic/roughness sampling.
    pub metallic_roughness_texture: Option<Handle<Image>>,
    /// `StandardMaterial::metallic_roughness_channel` selects the mesh UV channel for
    /// metallic/roughness.
    pub metallic_roughness_channel: BatchUvChannel,
    /// `StandardMaterial::normal_map_texture` requires normals, tangents, and normal sampling.
    pub normal_map_texture:         Option<Handle<Image>>,
    /// `StandardMaterial::normal_map_channel` selects the mesh UV channel for normals.
    pub normal_map_channel:         BatchUvChannel,
    /// `StandardMaterial::flip_normal_map_y` changes normal-map decoding.
    pub flip_normal_map_y:          bool,
    /// `StandardMaterial::occlusion_texture` bound for ambient-occlusion sampling.
    pub occlusion_texture:          Option<Handle<Image>>,
    /// `StandardMaterial::occlusion_channel` selects the mesh UV channel for occlusion.
    pub occlusion_channel:          BatchUvChannel,
    /// `StandardMaterial::depth_map` enables parallax/depth-map shader requirements.
    pub depth_map:                  Option<Handle<Image>>,
}

impl From<&StandardMaterial> for ResourceCompatibility {
    fn from(material: &StandardMaterial) -> Self {
        Self {
            base_color_texture:         material.base_color_texture.clone(),
            base_color_channel:         BatchUvChannel::from(&material.base_color_channel),
            emissive_texture:           material.emissive_texture.clone(),
            emissive_channel:           BatchUvChannel::from(&material.emissive_channel),
            metallic_roughness_texture: material.metallic_roughness_texture.clone(),
            metallic_roughness_channel: BatchUvChannel::from(&material.metallic_roughness_channel),
            normal_map_texture:         material.normal_map_texture.clone(),
            normal_map_channel:         BatchUvChannel::from(&material.normal_map_channel),
            flip_normal_map_y:          material.flip_normal_map_y,
            occlusion_texture:          material.occlusion_texture.clone(),
            occlusion_channel:          BatchUvChannel::from(&material.occlusion_channel),
            depth_map:                  material.depth_map.clone(),
        }
    }
}

/// Applies texture-resource compatibility to a cloned batch base material.
///
/// Non-texture pipeline facts stay in [`PipelineCompatibility`]. This helper is
/// the only direction where texture handles move from a material-table batch key
/// into the `StandardMaterial` half of an extended render material.
#[must_use]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Phase 3 and Phase 6 batch material creation route through this helper"
    )
)]
pub(crate) fn apply_resource_compatibility_to_standard_material(
    base: &StandardMaterial,
    compatibility: &ResourceCompatibility,
) -> StandardMaterial {
    let mut material = base.clone();
    material
        .base_color_texture
        .clone_from(&compatibility.base_color_texture);
    material.base_color_channel = compatibility.base_color_channel.into();
    material
        .emissive_texture
        .clone_from(&compatibility.emissive_texture);
    material.emissive_channel = compatibility.emissive_channel.into();
    material
        .metallic_roughness_texture
        .clone_from(&compatibility.metallic_roughness_texture);
    material.metallic_roughness_channel = compatibility.metallic_roughness_channel.into();
    material
        .normal_map_texture
        .clone_from(&compatibility.normal_map_texture);
    material.normal_map_channel = compatibility.normal_map_channel.into();
    material.flip_normal_map_y = compatibility.flip_normal_map_y;
    material
        .occlusion_texture
        .clone_from(&compatibility.occlusion_texture);
    material.occlusion_channel = compatibility.occlusion_channel.into();
    material.depth_map.clone_from(&compatibility.depth_map);
    material
}

/// Hash/Eq-able digest of the `StandardMaterial` fields visual batch materials
/// carry: floats as bits, textures as asset ids. `alpha_mode` and `depth_bias`
/// are deliberately absent because batch materials overwrite them per key.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct InternedMaterialKey {
    base_color:                 [u32; 4],
    base_color_texture:         Option<AssetId<Image>>,
    emissive:                   [u32; 4],
    emissive_texture:           Option<AssetId<Image>>,
    metallic:                   u32,
    perceptual_roughness:       u32,
    metallic_roughness_texture: Option<AssetId<Image>>,
    reflectance:                u32,
    normal_map_texture:         Option<AssetId<Image>>,
    occlusion_texture:          Option<AssetId<Image>>,
    unlit:                      bool,
    double_sided:               bool,
    cull_mode:                  Option<Face>,
}

impl From<&StandardMaterial> for InternedMaterialKey {
    fn from(material: &StandardMaterial) -> Self {
        let base_color = material.base_color.to_linear();
        Self {
            base_color:                 [
                base_color.red.to_bits(),
                base_color.green.to_bits(),
                base_color.blue.to_bits(),
                base_color.alpha.to_bits(),
            ],
            base_color_texture:         material.base_color_texture.as_ref().map(Handle::id),
            emissive:                   [
                material.emissive.red.to_bits(),
                material.emissive.green.to_bits(),
                material.emissive.blue.to_bits(),
                material.emissive.alpha.to_bits(),
            ],
            emissive_texture:           material.emissive_texture.as_ref().map(Handle::id),
            metallic:                   material.metallic.to_bits(),
            perceptual_roughness:       material.perceptual_roughness.to_bits(),
            metallic_roughness_texture: material
                .metallic_roughness_texture
                .as_ref()
                .map(Handle::id),
            reflectance:                material.reflectance.to_bits(),
            normal_map_texture:         material.normal_map_texture.as_ref().map(Handle::id),
            occlusion_texture:          material.occlusion_texture.as_ref().map(Handle::id),
            unlit:                      material.unlit,
            double_sided:               material.double_sided,
            cull_mode:                  material.cull_mode,
        }
    }
}

/// Assigns one [`BaseMaterialId`] per distinct authored visual material.
#[derive(Debug, Default)]
pub(crate) struct VisualMaterialInterner {
    ids:       HashMap<InternedMaterialKey, BaseMaterialId>,
    /// Reverse lookup: a clone of the first-seen material per id, the base the
    /// batch material is built from.
    materials: Vec<StandardMaterial>,
}

impl VisualMaterialInterner {
    /// Id for an authored base material, minting one on first sight.
    pub(crate) fn intern_base_material(&mut self, material: &StandardMaterial) -> BaseMaterialId {
        let key = InternedMaterialKey::from(material);
        if let Some(id) = self.ids.get(&key) {
            return *id;
        }
        let id = BaseMaterialId(self.materials.len().to_u32());
        self.materials.push(material.clone());
        self.ids.insert(key, id);
        id
    }

    /// The authored material behind an interned id.
    #[must_use]
    pub(crate) fn base_material(&self, id: BaseMaterialId) -> &StandardMaterial {
        &self.materials[id.0.to_usize()]
    }
}

#[cfg(test)]
mod tests {
    use bevy::mesh::UvChannel;
    use bevy::prelude::Color;

    use super::*;

    #[test]
    fn scalar_material_changes_do_not_change_resource_compatibility() {
        let first = StandardMaterial {
            base_color: Color::srgb(0.2, 0.3, 0.4),
            metallic: 0.1,
            reflectance: 0.8,
            ..Default::default()
        };
        let second = StandardMaterial {
            base_color: Color::srgb(0.8, 0.3, 0.1),
            metallic: 0.7,
            reflectance: 0.2,
            ..Default::default()
        };

        assert_eq!(
            ResourceCompatibility::from(&first),
            ResourceCompatibility::from(&second)
        );
    }

    #[test]
    fn apply_resource_compatibility_copies_texture_handles_and_channels() {
        let base = StandardMaterial {
            base_color: Color::srgb(0.1, 0.2, 0.3),
            ..Default::default()
        };
        let source = StandardMaterial {
            base_color_texture: Some(Handle::default()),
            base_color_channel: UvChannel::Uv1,
            emissive_texture: Some(Handle::default()),
            emissive_channel: UvChannel::Uv1,
            metallic_roughness_texture: Some(Handle::default()),
            metallic_roughness_channel: UvChannel::Uv1,
            normal_map_texture: Some(Handle::default()),
            normal_map_channel: UvChannel::Uv1,
            flip_normal_map_y: true,
            occlusion_texture: Some(Handle::default()),
            occlusion_channel: UvChannel::Uv1,
            depth_map: Some(Handle::default()),
            ..Default::default()
        };
        let compatibility = ResourceCompatibility::from(&source);

        let patched = apply_resource_compatibility_to_standard_material(&base, &compatibility);

        assert_eq!(ResourceCompatibility::from(&patched), compatibility);
        assert_eq!(patched.base_color, base.base_color);
    }
}
