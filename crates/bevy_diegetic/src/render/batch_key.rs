//! Shared visual batch-key pieces.
//!
//! Text and panel-line batching keep separate stores, GPU payloads, materials,
//! and shaders, but they split on a common subset of render compatibility:
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
use bevy::pbr::StandardMaterial;
use bevy::prelude::AlphaMode;
use bevy::prelude::Handle;
use bevy::render::render_resource::Face;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use crate::layout::GlyphLighting;
use crate::layout::GlyphShadowMode;
use crate::layout::GlyphSidedness;
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

/// Lighting mode shared by visual batch keys.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum VisualLighting {
    Lit,
    Unlit,
}

impl From<GlyphLighting> for VisualLighting {
    fn from(lighting: GlyphLighting) -> Self {
        match lighting {
            GlyphLighting::Lit => Self::Lit,
            GlyphLighting::Unlit => Self::Unlit,
        }
    }
}

/// Sidedness mode shared by visual batch keys.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum VisualSidedness {
    DoubleSided,
    OneSided,
}

impl From<GlyphSidedness> for VisualSidedness {
    fn from(sidedness: GlyphSidedness) -> Self {
        match sidedness {
            GlyphSidedness::DoubleSided => Self::DoubleSided,
            GlyphSidedness::OneSided => Self::OneSided,
        }
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
    pub lighting:      VisualLighting,
    /// Culling/double-sided compatibility.
    pub sidedness:     VisualSidedness,
    /// Shadow-caster compatibility.
    pub shadow:        VisualShadow,
    /// Owning render layers.
    pub layers:        BatchRenderLayers,
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
