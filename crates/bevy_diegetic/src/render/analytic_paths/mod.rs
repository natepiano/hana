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
pub(crate) use batching::PathBatchKey;
pub(crate) use batching::PathBatchResources;
pub(crate) use batching::PathBatchStore;
use bevy::asset::embedded_asset;
use bevy::asset::load_internal_asset;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
pub(crate) use geometry::Bounds;
pub(crate) use geometry::PathContour;
pub(crate) use geometry::PathOutline;
pub(crate) use geometry::QuadraticSegment;
pub(crate) use material::BatchPathMaterialInput;
pub(crate) use material::PathExtendedMaterial;
pub(crate) use material::RenderMode;
pub(crate) use material::batch_path_material;
#[cfg(test)]
pub(crate) use material::path_material_oit_depth_offset;
pub(crate) use material::set_batch_path_material_buffers;
pub(crate) use material::set_path_material_anti_alias;
pub(crate) use material::set_path_material_atlas;
pub(crate) use material::set_path_material_hairline;
pub(crate) use packing::BandRecord;
pub(crate) use packing::CurveRecord;
pub(crate) use packing::DEFAULT_BAND_COUNT;
pub(crate) use packing::PackedPath;
pub(crate) use packing::PathInstanceRecord;
pub(crate) use packing::PathRecord;
pub(crate) use packing::RunRecord;
pub(crate) use packing::build_packed_path;

use self::constants::ANALYTIC_PATH_VERTEX_PULL_SHADER_HANDLE;

/// Stable handles for the shared analytic-path atlas buffers every run's
/// material binds.
#[derive(Clone, Debug)]
pub(crate) struct PathAtlasHandles {
    /// Shared band-packed quadratic curve records.
    pub curves:       Handle<ShaderBuffer>,
    /// Shared along-Y/along-X band records.
    pub bands:        Handle<ShaderBuffer>,
    /// Shared path records, indexed by each instance record's `atlas_index`.
    pub path_records: Handle<ShaderBuffer>,
}

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
        app.add_plugins(MaterialPlugin::<PathExtendedMaterial>::default());
    }
}
