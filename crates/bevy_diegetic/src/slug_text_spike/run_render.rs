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

const GLYPH_PADDING_DESIGN_UNITS: f32 = 16.0;

/// Approximate storage profile for one Slug text run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SlugRunStorageProfile {
    /// Number of positioned glyph instances in the run.
    pub glyph_instances: usize,
    /// Number of unique glyph records packed for the run.
    pub unique_glyphs:   usize,
    /// Number of mesh vertices emitted for the run.
    pub mesh_vertices:   usize,
    /// Number of mesh indices emitted for the run.
    pub mesh_indices:    usize,
    /// Number of curve records uploaded for the run.
    pub curve_records:   usize,
    /// Number of band records uploaded for the run.
    pub band_records:    usize,
    /// Bytes used by curve records before GPU alignment.
    pub curve_bytes:     usize,
    /// Bytes used by band records before GPU alignment.
    pub band_bytes:      usize,
    /// Bytes used by glyph records before GPU alignment.
    pub glyph_bytes:     usize,
}

impl SlugRunStorageProfile {
    /// Total bytes used by Slug storage records before GPU alignment.
    #[must_use]
    pub const fn storage_bytes(self) -> usize {
        self.curve_bytes + self.band_bytes + self.glyph_bytes
    }
}

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

impl SlugRunRenderData {
    /// Returns a profile for this run's mesh and storage records.
    #[must_use]
    pub fn profile(&self) -> SlugRunStorageProfile {
        SlugRunStorageProfile {
            glyph_instances: self.mesh.count_vertices() / 4,
            unique_glyphs:   self.glyphs.len(),
            mesh_vertices:   self.mesh.count_vertices(),
            mesh_indices:    self.mesh.indices().map_or(0, Indices::len),
            curve_records:   self.curves.len(),
            band_records:    self.bands.len(),
            curve_bytes:     self.curves.len() * std::mem::size_of::<SlugCurveRecord>(),
            band_bytes:      self.bands.len() * std::mem::size_of::<SlugBandRecord>(),
            glyph_bytes:     self.glyphs.len() * std::mem::size_of::<SlugGlyphRecord>(),
        }
    }
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
        let band_count = packed.bands().len().to_u32() / 2;

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
    use crate::slug_text_spike::SlugBackend;
    use crate::slug_text_spike::SlugFontKey;
    use crate::slug_text_spike::SlugTextRequest;

    const FONT_DATA: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
    const FONT_FAMILY: &str = "JetBrains Mono";
    const FONT_KEY: SlugFontKey = SlugFontKey::new(7);
    const FONT_SCALE: f32 = 0.001;

    #[test]
    fn profile_counts_unique_glyph_storage_once() {
        let (preview, backend) = build_preview("Typography");
        let render_data = build_slug_run_render_data(&preview, backend.glyph_cache(), FONT_SCALE)
            .expect("fixture run should render");
        let profile = render_data.profile();

        assert_eq!(profile.glyph_instances, preview.run.glyphs().len());
        assert_eq!(profile.mesh_vertices, profile.glyph_instances * 4);
        assert_eq!(profile.mesh_indices, profile.glyph_instances * 6);
        assert_eq!(profile.unique_glyphs, backend.glyph_cache().len());
        assert!(
            profile.unique_glyphs < profile.glyph_instances,
            "repeated letters should share packed glyph storage"
        );
        assert!(profile.storage_bytes() > 0);
    }

    #[test]
    fn clip_rect_trims_mesh_positions_and_uvs() {
        let (preview, backend) = build_preview("H");
        let bounds = preview.run.bounds();
        let clip_x = bounds.min.x.midpoint(bounds.max.x) * FONT_SCALE;
        let clip_rect = Some([
            clip_x,
            bounds.min.y * FONT_SCALE,
            bounds.max.x * FONT_SCALE,
            bounds.max.y * FONT_SCALE,
        ]);

        let render_data = build_slug_run_render_data_with_clip(
            &preview,
            backend.glyph_cache(),
            FONT_SCALE,
            clip_rect,
        )
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
        let (preview, backend) = build_preview("H");
        let bounds = preview.run.bounds();
        let clip_rect = Some([
            bounds.max.x.mul_add(FONT_SCALE, 1.0),
            bounds.min.y * FONT_SCALE,
            bounds.max.x.mul_add(FONT_SCALE, 2.0),
            bounds.max.y * FONT_SCALE,
        ]);

        let render_data = build_slug_run_render_data_with_clip(
            &preview,
            backend.glyph_cache(),
            FONT_SCALE,
            clip_rect,
        )
        .expect("fixture run should render");

        assert_eq!(render_data.profile().mesh_vertices, 0);
        assert_eq!(render_data.profile().mesh_indices, 0);
    }

    fn build_preview(text: &str) -> (SlugBuiltTextRun, SlugBackend) {
        let mut backend = SlugBackend::default();
        let prepared = backend
            .prepare_text_run(SlugTextRequest::new(
                text,
                FONT_DATA,
                FONT_KEY,
                FONT_FAMILY,
                FONT_SCALE,
            ))
            .expect("fixture text should prepare");
        (prepared.run, backend)
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
