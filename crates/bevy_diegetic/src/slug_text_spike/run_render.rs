use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::Mesh;
use bevy::render::render_resource::PrimitiveTopology;
use bevy_kana::ToF32;
use bevy_kana::ToU32;

use super::packing::SlugBandRecord;
use super::packing::SlugCurveRecord;
use super::packing::SlugGlyphRecord;
use super::run::SlugBuiltTextRun;
use super::run::SlugGlyphCache;
use super::run::SlugGlyphInstance;
use super::run::SlugGlyphKey;

/// GPU-ready data for one Slug text run in the isolated spike path.
#[derive(Clone, Debug)]
pub struct SlugRunRenderData {
    /// One quad per glyph instance in the shaped run.
    pub mesh:   Mesh,
    /// Combined curve records for all unique glyphs in this run.
    pub curves: Vec<SlugCurveRecord>,
    /// Combined band records for all unique glyphs in this run.
    pub bands:  Vec<SlugBandRecord>,
    /// Unique glyph table indexed by each quad through `UV_1.x`.
    pub glyphs: Vec<SlugGlyphRecord>,
}

/// Error while building run-level Slug render data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlugRunRenderError {
    /// The shaped run referenced a glyph missing from its packed glyph cache.
    MissingPackedGlyph(SlugGlyphKey),
}

impl Display for SlugRunRenderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPackedGlyph(key) => write!(
                f,
                "missing packed glyph for font key {} glyph id {} preprocessing version {}",
                key.font().value(),
                key.glyph_id(),
                key.preprocess_version()
            ),
        }
    }
}

impl Error for SlugRunRenderError {}

/// Builds one run-level mesh and packed storage set for a Slug text run.
pub fn build_slug_run_render_data(
    preview: &SlugBuiltTextRun,
    glyph_cache: &SlugGlyphCache,
    scale: f32,
) -> Result<SlugRunRenderData, SlugRunRenderError> {
    build_slug_run_render_data_with_clip(preview, glyph_cache, scale, None)
}

/// Builds one run-level mesh and packed storage set clipped to a local rect.
pub fn build_slug_run_render_data_with_clip(
    preview: &SlugBuiltTextRun,
    glyph_cache: &SlugGlyphCache,
    scale: f32,
    clip_rect: Option<[f32; 4]>,
) -> Result<SlugRunRenderData, SlugRunRenderError> {
    let mut packer = RunPacker::default();
    let mut mesh_builder = RunMeshBuilder::new(preview.run.glyphs().len());

    for glyph in preview.run.glyphs() {
        let record_index = packer.record_index(*glyph, glyph_cache)?;
        mesh_builder.push_glyph(*glyph, record_index, scale, clip_rect);
    }

    Ok(SlugRunRenderData {
        mesh:   mesh_builder.finish(),
        curves: packer.curves,
        bands:  packer.bands,
        glyphs: packer.glyphs,
    })
}

#[derive(Default)]
struct RunPacker {
    record_indices: HashMap<SlugGlyphKey, u32>,
    curves:         Vec<SlugCurveRecord>,
    bands:          Vec<SlugBandRecord>,
    glyphs:         Vec<SlugGlyphRecord>,
}

impl RunPacker {
    fn record_index(
        &mut self,
        glyph: SlugGlyphInstance,
        glyph_cache: &SlugGlyphCache,
    ) -> Result<u32, SlugRunRenderError> {
        if let Some(index) = self.record_indices.get(&glyph.key()).copied() {
            return Ok(index);
        }

        let packed = glyph_cache
            .get(glyph.key())
            .ok_or_else(|| SlugRunRenderError::MissingPackedGlyph(glyph.key()))?;
        let record_index = self.glyphs.len().to_u32();
        let curve_start = self.curves.len().to_u32();
        let band_start = self.bands.len().to_u32();

        self.curves.extend_from_slice(packed.curves());
        self.bands
            .extend(packed.bands().iter().map(|band| SlugBandRecord {
                start: band.start + curve_start,
                count: band.count,
                y_min: band.y_min,
                y_max: band.y_max,
            }));
        self.glyphs.push(SlugGlyphRecord::new(
            packed.bounds(),
            band_start,
            packed.bands().len().to_u32(),
        ));
        self.record_indices.insert(glyph.key(), record_index);

        Ok(record_index)
    }
}

struct RunMeshBuilder {
    positions:     Vec<[f32; 3]>,
    normals:       Vec<[f32; 3]>,
    uvs:           Vec<[f32; 2]>,
    glyph_indices: Vec<[f32; 2]>,
    indices:       Vec<u32>,
}

struct GlyphQuadExtents {
    left:      f32,
    right:     f32,
    bottom:    f32,
    top:       f32,
    uv_left:   f32,
    uv_right:  f32,
    uv_top:    f32,
    uv_bottom: f32,
}

impl GlyphQuadExtents {
    const fn new(left: f32, right: f32, bottom: f32, top: f32) -> Self {
        Self {
            left,
            right,
            bottom,
            top,
            uv_left: 0.0,
            uv_right: 1.0,
            uv_top: 0.0,
            uv_bottom: 1.0,
        }
    }

    fn clipped(mut self, clip_rect: Option<[f32; 4]>) -> Option<Self> {
        let Some([clip_left, clip_bottom, clip_right, clip_top]) = clip_rect else {
            return Some(self);
        };
        if self.right <= clip_left
            || self.left >= clip_right
            || self.top <= clip_bottom
            || self.bottom >= clip_top
        {
            return None;
        }

        let original_left = self.left;
        let original_right = self.right;
        let original_bottom = self.bottom;
        let original_top = self.top;
        self.left = self.left.max(clip_left);
        self.right = self.right.min(clip_right);
        self.bottom = self.bottom.max(clip_bottom);
        self.top = self.top.min(clip_top);

        let width = original_right - original_left;
        let height = original_top - original_bottom;
        if width <= f32::EPSILON || height <= f32::EPSILON {
            return None;
        }
        self.uv_left = (self.left - original_left) / width;
        self.uv_right = (self.right - original_left) / width;
        self.uv_top = (original_top - self.top) / height;
        self.uv_bottom = (original_top - self.bottom) / height;
        Some(self)
    }
}

impl RunMeshBuilder {
    fn new(glyph_count: usize) -> Self {
        Self {
            positions:     Vec::with_capacity(glyph_count * 4),
            normals:       Vec::with_capacity(glyph_count * 4),
            uvs:           Vec::with_capacity(glyph_count * 4),
            glyph_indices: Vec::with_capacity(glyph_count * 4),
            indices:       Vec::with_capacity(glyph_count * 6),
        }
    }

    fn push_glyph(
        &mut self,
        glyph: SlugGlyphInstance,
        record_index: u32,
        scale: f32,
        clip_rect: Option<[f32; 4]>,
    ) {
        let bounds = glyph.bounds();
        let bounds_scale = glyph.bounds_scale();
        let origin = glyph.origin();
        let left = bounds.min.x.mul_add(bounds_scale.x, origin.x) * scale;
        let right = bounds.max.x.mul_add(bounds_scale.x, origin.x) * scale;
        let bottom = bounds.min.y.mul_add(bounds_scale.y, origin.y) * scale;
        let top = bounds.max.y.mul_add(bounds_scale.y, origin.y) * scale;
        let Some(extents) = GlyphQuadExtents::new(left, right, bottom, top).clipped(clip_rect)
        else {
            return;
        };

        let base = (self.positions.len()).to_u32();
        let glyph_index = [record_index.to_f32(), 0.0];

        self.positions.push([extents.left, extents.top, 0.0]);
        self.positions.push([extents.right, extents.top, 0.0]);
        self.positions.push([extents.right, extents.bottom, 0.0]);
        self.positions.push([extents.left, extents.bottom, 0.0]);

        self.normals.extend([[0.0, 0.0, 1.0]; 4]);
        self.uvs.extend([
            [extents.uv_left, extents.uv_top],
            [extents.uv_right, extents.uv_top],
            [extents.uv_right, extents.uv_bottom],
            [extents.uv_left, extents.uv_bottom],
        ]);
        self.glyph_indices.extend([glyph_index; 4]);

        self.indices
            .extend([base, base + 3, base + 2, base, base + 2, base + 1]);
    }

    fn finish(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.glyph_indices);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}
