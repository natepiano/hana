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

use crate::text::slug::glyph::BandRecord;
use crate::text::slug::glyph::CurveRecord;
use crate::text::slug::glyph::GlyphRecord;
use crate::text::slug::runtime::BuiltTextRun;
use crate::text::slug::runtime::GlyphInstance;
use crate::text::slug::runtime::GlyphKey;
use crate::text::slug::runtime::GlyphOutlineCache;

const GLYPH_PADDING_DESIGN_UNITS: f32 = 16.0;

/// GPU-ready data for one text run.
#[derive(Clone, Debug)]
pub struct RunRenderData {
    /// One quad per glyph instance in the run.
    pub mesh:   Mesh,
    /// Combined curve records for all unique glyphs in this run.
    pub curves: Vec<CurveRecord>,
    /// Combined band records for all unique glyphs in this run.
    pub bands:  Vec<BandRecord>,
    /// Unique glyph table indexed by each quad through `UV_1.x`.
    pub glyphs: Vec<GlyphRecord>,
}

/// Error while building run-level render data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RunRenderError {
    /// The shaped run referenced a glyph missing from its packed glyph cache.
    MissingPackedGlyph(GlyphKey),
    /// Clipping removed every quad from the prepared run.
    NoVisibleGlyphs,
}

impl Display for RunRenderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPackedGlyph(key) => write!(
                f,
                "missing packed glyph for font key {} glyph id {} preprocessing version {}",
                key.font().value(),
                key.glyph_id(),
                key.preprocess_version()
            ),
            Self::NoVisibleGlyphs => write!(f, "text run has no visible glyph quads"),
        }
    }
}

impl Error for RunRenderError {}

/// Builds one run-level mesh and packed storage set clipped to a local rect.
pub fn build_run_render_data_with_clip(
    preview: &BuiltTextRun,
    glyph_outline_cache: &GlyphOutlineCache,
    scale: f32,
    clip_rect: Option<[f32; 4]>,
) -> Result<RunRenderData, RunRenderError> {
    let mut packer = RunPacker::default();
    let mut mesh_builder = RunMeshBuilder::new(preview.run.glyphs().len());

    for glyph in preview.run.glyphs() {
        let record_index = packer.record_index(*glyph, glyph_outline_cache)?;
        mesh_builder.push_glyph(*glyph, record_index, scale, clip_rect);
    }

    Ok(RunRenderData {
        mesh:   mesh_builder.finish(),
        curves: packer.curves,
        bands:  packer.bands,
        glyphs: packer.glyphs,
    })
}

#[derive(Default)]
struct RunPacker {
    record_indices: HashMap<GlyphKey, u32>,
    curves:         Vec<CurveRecord>,
    bands:          Vec<BandRecord>,
    glyphs:         Vec<GlyphRecord>,
}

impl RunPacker {
    fn record_index(
        &mut self,
        glyph: GlyphInstance,
        glyph_cache: &GlyphOutlineCache,
    ) -> Result<u32, RunRenderError> {
        if let Some(index) = self.record_indices.get(&glyph.key()).copied() {
            return Ok(index);
        }

        let packed = glyph_cache
            .get(glyph.key())
            .ok_or_else(|| RunRenderError::MissingPackedGlyph(glyph.key()))?;
        let record_index = self.glyphs.len().to_u32();
        let curve_start = self.curves.len().to_u32();
        let band_start = self.bands.len().to_u32();
        let band_count = packed.bands().len().to_u32() / 2;

        self.curves.extend_from_slice(packed.curves());
        self.bands
            .extend(packed.bands().iter().map(|band| BandRecord {
                start: band.start + curve_start,
                count: band.count,
                y_min: band.y_min,
                y_max: band.y_max,
            }));
        self.glyphs.push(GlyphRecord::new(
            packed.bounds(),
            band_start,
            band_count,
            band_start + band_count,
            band_count,
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
    left:          f32,
    right:         f32,
    bottom:        f32,
    top:           f32,
    source_left:   f32,
    source_right:  f32,
    source_bottom: f32,
    source_top:    f32,
    uv_left:       f32,
    uv_right:      f32,
    uv_top:        f32,
    uv_bottom:     f32,
}

impl GlyphQuadExtents {
    fn new(left: f32, right: f32, bottom: f32, top: f32, padding_x: f32, padding_y: f32) -> Self {
        let width = (right - left).max(f32::EPSILON);
        let height = (top - bottom).max(f32::EPSILON);
        let uv_padding_x = padding_x / width;
        let uv_padding_y = padding_y / height;

        Self {
            left:          left - padding_x,
            right:         right + padding_x,
            bottom:        bottom - padding_y,
            top:           top + padding_y,
            source_left:   left,
            source_right:  right,
            source_bottom: bottom,
            source_top:    top,
            uv_left:       -uv_padding_x,
            uv_right:      1.0 + uv_padding_x,
            uv_top:        -uv_padding_y,
            uv_bottom:     1.0 + uv_padding_y,
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

        self.left = self.left.max(clip_left);
        self.right = self.right.min(clip_right);
        self.bottom = self.bottom.max(clip_bottom);
        self.top = self.top.min(clip_top);

        let width = self.source_right - self.source_left;
        let height = self.source_top - self.source_bottom;
        if width <= f32::EPSILON || height <= f32::EPSILON {
            return None;
        }
        self.uv_left = (self.left - self.source_left) / width;
        self.uv_right = (self.right - self.source_left) / width;
        self.uv_top = (self.source_top - self.top) / height;
        self.uv_bottom = (self.source_top - self.bottom) / height;
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
        glyph: GlyphInstance,
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
        let padding_x = GLYPH_PADDING_DESIGN_UNITS * bounds_scale.x.abs() * scale;
        let padding_y = GLYPH_PADDING_DESIGN_UNITS * bounds_scale.y.abs() * scale;
        let Some(extents) = GlyphQuadExtents::new(left, right, bottom, top, padding_x, padding_y)
            .clipped(clip_rect)
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

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "tests should fail loudly when fixture mesh data is missing"
)]
mod tests {
    use bevy::mesh::MeshVertexAttribute;
    use bevy::mesh::VertexAttributeValues;

    use super::*;
    use crate::text::slug::runtime::GlyphOutlineCache;
    use crate::text::slug::support;

    const FONT_DATA: &[u8] = include_bytes!("../../../../assets/fonts/JetBrainsMono-Regular.ttf");

    #[test]
    fn render_data_counts_unique_glyph_storage_once() {
        let (preview, cache) = support::fixture_run_with_cache(FONT_DATA, 7, "Typography");
        let render_data = build_run_render_data_with_clip(&preview, &cache, 1.0, None)
            .expect("fixture run should render");

        let glyph_instances = preview.run.glyphs().len();
        let mesh_indices = render_data.mesh.indices().map_or(0, Indices::len);
        assert_eq!(render_data.mesh.count_vertices(), glyph_instances * 4);
        assert_eq!(mesh_indices, glyph_instances * 6);
        assert!(
            render_data.glyphs.len() < glyph_instances,
            "repeated letters should share packed glyph storage"
        );
        assert!(!render_data.curves.is_empty());
    }

    #[test]
    fn clip_rect_trims_mesh_positions_and_uvs() {
        let (preview, cache) = support::fixture_run_with_cache(FONT_DATA, 7, "H");
        let (min_x, max_x, min_y, max_y) = mesh_extent(&preview, &cache);
        let clip_x = f32::midpoint(min_x, max_x);
        let clip_rect = Some([clip_x, min_y, max_x, max_y]);

        let render_data = build_run_render_data_with_clip(&preview, &cache, 1.0, clip_rect)
            .expect("fixture run should render");

        let positions = float32x3_attribute(&render_data.mesh, Mesh::ATTRIBUTE_POSITION);
        let uvs = float32x2_attribute(&render_data.mesh, Mesh::ATTRIBUTE_UV_0);
        assert_eq!(positions.len(), 4);
        assert_eq!(uvs.len(), 4);
        assert!(
            positions.iter().all(|position| position[0] >= clip_x),
            "clipped mesh should not extend left of the clip rect"
        );
        assert!(
            uvs.iter().any(|uv| uv[0] > 0.0),
            "left-trimmed glyph should move UVs into the glyph quad"
        );
    }

    #[test]
    fn fully_clipped_glyph_emits_no_mesh_vertices() {
        let (preview, cache) = support::fixture_run_with_cache(FONT_DATA, 7, "H");
        let (_, max_x, min_y, max_y) = mesh_extent(&preview, &cache);
        let clip_rect = Some([max_x + 1.0, min_y, max_x + 2.0, max_y]);

        let render_data = build_run_render_data_with_clip(&preview, &cache, 1.0, clip_rect)
            .expect("fixture run should render");

        assert_eq!(render_data.mesh.count_vertices(), 0);
        assert_eq!(render_data.mesh.indices().map_or(0, Indices::len), 0);
    }

    /// X/Y extent of the unclipped fixture mesh: `(min_x, max_x, min_y, max_y)`.
    fn mesh_extent(preview: &BuiltTextRun, cache: &GlyphOutlineCache) -> (f32, f32, f32, f32) {
        let render_data = build_run_render_data_with_clip(preview, cache, 1.0, None)
            .expect("fixture run should render");
        let positions = float32x3_attribute(&render_data.mesh, Mesh::ATTRIBUTE_POSITION);
        (
            positions.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min),
            positions
                .iter()
                .map(|p| p[0])
                .fold(f32::NEG_INFINITY, f32::max),
            positions.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min),
            positions
                .iter()
                .map(|p| p[1])
                .fold(f32::NEG_INFINITY, f32::max),
        )
    }

    fn float32x3_attribute(mesh: &Mesh, attribute: MeshVertexAttribute) -> &[[f32; 3]] {
        let values = mesh
            .attribute(attribute)
            .expect("mesh should contain requested attribute");
        let VertexAttributeValues::Float32x3(values) = values else {
            panic!("mesh attribute should be Float32x3");
        };
        values
    }

    fn float32x2_attribute(mesh: &Mesh, attribute: MeshVertexAttribute) -> &[[f32; 2]] {
        let values = mesh
            .attribute(attribute)
            .expect("mesh should contain requested attribute");
        let VertexAttributeValues::Float32x2(values) = values else {
            panic!("mesh attribute should be Float32x2");
        };
        values
    }
}
