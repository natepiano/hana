//! Shared analytic quadratic path renderer.
//!
//! Text still owns shaping, font lookup, and glyph outline extraction. This
//! module owns the renderer-facing records, packing, material, shader handles,
//! and batch store so future panel-authored vector marks can use the same
//! analytic coverage path.

mod atlas;
mod batching;
mod constants;
mod geometry;
mod material;
mod packing;

pub(crate) use atlas::PathAtlas;
pub(crate) use batching::BatchGpu;
pub(crate) use batching::BatchKey;
pub(crate) use batching::GlyphBatchStore;
use bevy::asset::embedded_asset;
use bevy::asset::load_internal_asset;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
pub(crate) use geometry::Bounds;
pub(crate) use geometry::PathContour;
pub(crate) use geometry::PathOutline;
pub(crate) use geometry::QuadraticSegment;
pub(crate) use material::BatchTextMaterialInput;
pub(crate) use material::RenderMode;
pub(crate) use material::TextMaterial;
pub(crate) use material::batch_text_material;
pub(crate) use material::set_batch_text_material_buffers;
pub(crate) use material::set_text_material_anti_alias;
pub(crate) use material::set_text_material_atlas;
pub(crate) use material::set_text_material_hairline;
#[cfg(test)]
pub(crate) use material::text_material_oit_depth_offset;
#[cfg(feature = "batch_proof")]
pub(crate) use material::toggle_text_material_debug_glyph_index;
pub(crate) use packing::BandRecord;
pub(crate) use packing::CurveRecord;
pub(crate) use packing::DEFAULT_BAND_COUNT;
pub(crate) use packing::GlyphInstanceRecord;
pub(crate) use packing::GlyphOutline;
pub(crate) use packing::GlyphRecord;
#[allow(
    unused_imports,
    reason = "Phase A exposes shared path names before Phase B consumes them"
)]
pub(crate) use packing::PackedPath;
#[allow(
    unused_imports,
    reason = "Phase A exposes shared path names before Phase B consumes them"
)]
pub(crate) use packing::PathInstanceRecord;
#[allow(
    unused_imports,
    reason = "Phase A exposes shared path names before Phase B consumes them"
)]
pub(crate) use packing::PathRecord;
#[allow(
    unused_imports,
    reason = "Phase A exposes shared path names before Phase B consumes them"
)]
pub(crate) use packing::PathRunRecord;
pub(crate) use packing::RunRecord;
pub(crate) use packing::build_packed_path;

use self::constants::ANALYTIC_PATH_VERTEX_PULL_SHADER_HANDLE;

/// Stable handles for the shared analytic-path atlas buffers every run's
/// material binds.
#[derive(Clone, Debug)]
pub(crate) struct PathAtlasHandles {
    /// Shared band-packed quadratic curve records.
    pub curves: Handle<ShaderBuffer>,
    /// Shared horizontal/vertical band records.
    pub bands:  Handle<ShaderBuffer>,
    /// Shared path records, indexed by each instance record's `atlas_index`.
    pub glyphs: Handle<ShaderBuffer>,
}

/// Compatibility alias while text glyphs are bridged onto analytic paths.
pub(crate) type GlyphAtlasHandles = PathAtlasHandles;

pub(crate) struct AnalyticPathPlugin;

impl Plugin for AnalyticPathPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "analytic_path.wgsl");
        load_internal_asset!(
            app,
            ANALYTIC_PATH_VERTEX_PULL_SHADER_HANDLE,
            "analytic_path_vertex_pull.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(MaterialPlugin::<TextMaterial>::default());
    }
}
