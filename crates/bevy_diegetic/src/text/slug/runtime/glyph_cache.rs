//! Glyph cache: positioned-glyph input, prepared runs, and GPU storage handles.

use std::collections::HashMap;

use bevy::prelude::Assets;
use bevy::prelude::Component;
use bevy::prelude::Entity;
use bevy::prelude::Handle;
use bevy::prelude::Mesh;
use bevy::prelude::Resource;
use bevy::prelude::Vec2;
use bevy::render::storage::ShaderBuffer;
use ttf_parser::Face;

use super::BuiltTextRun;
use super::FontKey;
use super::GlyphInstance;
use super::GlyphKey;
use super::GlyphOutlineCache;
use super::TextRun;
use crate::layout::ShapedGlyph;
use crate::text::Font;
use crate::text::slug::RunRenderError;
use crate::text::slug::glyph;
use crate::text::slug::glyph::OutlineError;
use crate::text::slug::render;
use crate::text::slug::render::RunRenderData;

/// One production glyph positioned for run preparation: a `ShapedGlyph` plus
/// the `Font` and collection index needed to extract its outline.
#[derive(Clone, Copy)]
pub(crate) struct PositionedGlyph<'a> {
    pub glyph:            &'a ShapedGlyph,
    pub font:             &'a Font,
    pub collection_index: u32,
}

/// Result of preparing one text run.
#[derive(Clone, Debug)]
pub(crate) struct PreparedTextRun {
    /// Prepared text run.
    run: BuiltTextRun,
}

impl PreparedTextRun {
    /// Number of visible glyph quads in this prepared run.
    #[must_use]
    pub fn glyph_count(&self) -> usize { self.run.run.glyphs().len() }
}

/// Stable per-label key for one run's GPU storage handles, derived from the
/// label entity so the same label addresses the same storage every frame.
#[derive(Clone, Copy, Component, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RunStorageKey(u64);

impl From<Entity> for RunStorageKey {
    fn from(label: Entity) -> Self { Self(label.to_bits()) }
}

/// GPU storage handle for one prepared run. Only the run's glyph-quad mesh is
/// per-run; the curve/band/glyph records it indexes live in the shared atlas
/// ([`GlyphAtlasHandles`]).
#[derive(Clone, Debug)]
pub struct RunStorage {
    /// One quad per glyph instance; each quad indexes the shared glyph atlas.
    pub mesh: Handle<Mesh>,
}

/// Stable handles for the shared glyph atlas buffers every run's material binds.
/// One set for the whole cache, re-uploaded only when a new glyph grows the
/// atlas (see [`GlyphCache::commit_glyph_atlas`]).
#[derive(Clone, Debug)]
pub(crate) struct GlyphAtlasHandles {
    /// Shared band-packed quadratic curve records.
    pub curves: Handle<ShaderBuffer>,
    /// Shared horizontal/vertical band records.
    pub bands:  Handle<ShaderBuffer>,
    /// Shared glyph records, indexed by each run's mesh through `UV_1.x`.
    pub glyphs: Handle<ShaderBuffer>,
}

/// `GlyphCache` resource: owns the glyph cache and per-run GPU storage.
#[derive(Debug, Default, Resource)]
pub(crate) struct GlyphCache {
    outline_cache:      GlyphOutlineCache,
    run_storage:        HashMap<RunStorageKey, RunStorage>,
    preprocess_version: u32,
    atlas:              Option<GlyphAtlasHandles>,
    uploaded_revision:  u32,
}

impl GlyphCache {
    /// Prepares one run from already-positioned production glyphs with
    /// caller-provided X/Y placement scale.
    pub fn prepare_positioned_run_with_scale(
        &mut self,
        glyphs: &[PositionedGlyph<'_>],
        anchor: Vec2,
        layout_font_size: f32,
        placement_scale: Vec2,
        band_count: usize,
    ) -> Result<PreparedTextRun, OutlineError> {
        let mut instances = Vec::with_capacity(glyphs.len());
        for positioned in glyphs {
            let face = Face::parse(positioned.font.data(), positioned.collection_index)
                .map_err(|_| OutlineError::InvalidFont)?;
            if !glyph::glyph_id_has_visible_outline(&face, positioned.glyph.id) {
                continue;
            }
            let bounds_scale =
                placement_scale * (layout_font_size / f32::from(face.units_per_em()));
            let key = self.glyph_key(
                FontKey::new(positioned.glyph.font_face.blob_id),
                positioned.glyph.id,
            );
            let packed_glyph = self.outline_cache.get_or_insert_packed_from_face(
                key,
                positioned.font.data(),
                positioned.collection_index,
                POSITIONED_GLYPH_DIAGNOSTIC_CHAR,
                band_count,
            )?;
            instances.push(GlyphInstance::new_non_uniform(
                key,
                Vec2::new(
                    (positioned.glyph.x - anchor.x) * placement_scale.x,
                    -(positioned.glyph.baseline + positioned.glyph.y - anchor.y)
                        * placement_scale.y,
                ),
                packed_glyph.bounds(),
                bounds_scale,
            ));
        }

        Ok(PreparedTextRun {
            run: BuiltTextRun {
                run: TextRun::new(instances),
            },
        })
    }

    /// Builds this run's GPU-ready render data from the shared glyph atlas,
    /// clipped to `clip_rect`. Returns `NoVisibleGlyphs` when clipping leaves no
    /// quad. Pair it with [`Self::commit_run_storage`] to build then upload a
    /// run; the two are split so a caller can time the build and upload halves
    /// separately.
    pub fn build_run_render_data(
        &self,
        prepared: &PreparedTextRun,
        clip_rect: Option<[f32; 4]>,
    ) -> Result<RunRenderData, RunRenderError> {
        let render_data = render::build_run_render_data_with_clip(
            &prepared.run,
            &self.outline_cache,
            1.0,
            clip_rect,
        )?;
        // Bevy's mesh allocator skips allocation for zero-vertex meshes, but
        // the extracted mesh asset can still be processed for upload. Fully
        // clipped text runs should therefore not create a mesh asset.
        if render_data.mesh.count_vertices() == 0 {
            return Err(RunRenderError::NoVisibleGlyphs);
        }
        Ok(render_data)
    }

    /// Writes `render_data`'s mesh to the storage keyed by `storage_key`,
    /// overwriting the existing mesh in place when the key is present and
    /// allocating a new mesh asset the first time it is seen. Pair it with
    /// [`Self::build_run_render_data`]; the two are split so a caller can time
    /// the build and upload halves separately.
    ///
    /// The in-place write keeps the same `Handle<Mesh>`, so the render world
    /// re-uploads only the changed vertices without a handle swap and the run's
    /// mesh entity is never respawned. The curve/band/glyph records are not
    /// per-run — they live in the shared atlas committed by
    /// [`Self::commit_glyph_atlas`].
    pub fn commit_run_storage(
        &mut self,
        storage_key: RunStorageKey,
        render_data: RunRenderData,
        meshes: &mut Assets<Mesh>,
    ) -> RunStorage {
        if let Some(storage) = self.run_storage.get(&storage_key) {
            // Same label, changed text: overwrite the existing mesh behind its
            // stable handle. `get_mut` marks it modified, so the render world
            // re-uploads the new vertices without a handle swap.
            if let Some(mut mesh) = meshes.get_mut(&storage.mesh) {
                *mesh = render_data.mesh;
            }
            return storage.clone();
        }
        let storage = RunStorage {
            mesh: meshes.add(render_data.mesh),
        };
        self.run_storage.insert(storage_key, storage.clone());
        storage
    }

    /// Uploads the shared glyph atlas to its GPU buffers when new glyphs have
    /// been packed since the last upload, returning the stable handles every
    /// run's material binds. The three buffers are created the first time this
    /// runs and then overwritten in place behind their handles, so a frame that
    /// packs no new glyph re-uploads nothing. Returns `None` before any glyph is
    /// packed, so no zero-length buffer is ever created.
    pub fn commit_glyph_atlas(
        &mut self,
        storage_buffers: &mut Assets<ShaderBuffer>,
    ) -> Option<GlyphAtlasHandles> {
        if self.outline_cache.atlas_glyph_records().is_empty() {
            return None;
        }
        let revision = self.outline_cache.atlas_revision();
        if let Some(handles) = self.atlas.clone() {
            // The atlas only grows; re-serialize the three buffers in place when
            // a new glyph bumped the revision, otherwise reuse them untouched.
            if self.uploaded_revision != revision {
                if let Some(mut curves) = storage_buffers.get_mut(&handles.curves) {
                    curves.set_data(self.outline_cache.atlas_curves().to_vec());
                }
                if let Some(mut bands) = storage_buffers.get_mut(&handles.bands) {
                    bands.set_data(self.outline_cache.atlas_bands().to_vec());
                }
                if let Some(mut glyphs) = storage_buffers.get_mut(&handles.glyphs) {
                    glyphs.set_data(self.outline_cache.atlas_glyph_records().to_vec());
                }
                self.uploaded_revision = revision;
            }
            return Some(handles);
        }
        let handles = GlyphAtlasHandles {
            curves: storage_buffers.add(ShaderBuffer::from(
                self.outline_cache.atlas_curves().to_vec(),
            )),
            bands:  storage_buffers.add(ShaderBuffer::from(
                self.outline_cache.atlas_bands().to_vec(),
            )),
            glyphs: storage_buffers.add(ShaderBuffer::from(
                self.outline_cache.atlas_glyph_records().to_vec(),
            )),
        };
        self.uploaded_revision = revision;
        self.atlas = Some(handles.clone());
        Some(handles)
    }

    /// Removes one run's GPU storage handles.
    pub fn remove_run_storage(&mut self, key: RunStorageKey) -> Option<RunStorage> {
        self.run_storage.remove(&key)
    }

    /// Number of runs that currently hold GPU storage.
    #[cfg(test)]
    pub fn run_storage_len(&self) -> usize { self.run_storage.len() }

    /// Stable key for a glyph in the current preprocessing profile.
    #[must_use]
    pub const fn glyph_key(&self, font: FontKey, glyph_id: u16) -> GlyphKey {
        GlyphKey::with_preprocess_version(font, glyph_id, self.preprocess_version)
    }
}

const POSITIONED_GLYPH_DIAGNOSTIC_CHAR: char = '\u{FFFD}';

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should fail loudly when fixture text runs cannot be prepared"
)]
mod tests {
    use bevy::prelude::Assets;

    use super::*;
    use crate::text::slug::support;

    const FONT_DATA: &[u8] = include_bytes!("../../../../assets/fonts/JetBrainsMono-Regular.ttf");
    const CRIMSON_TEXT_DATA: &[u8] =
        include_bytes!("../../../../assets/fonts/CrimsonText-Regular.ttf");
    const EB_GARAMOND_DATA: &[u8] =
        include_bytes!("../../../../assets/fonts/EBGaramond-Regular.ttf");
    const LIBERATION_SANS_DATA: &[u8] =
        include_bytes!("../../../../assets/fonts/LiberationSans-Regular.ttf");
    const NOTO_SANS_DATA: &[u8] = include_bytes!("../../../../assets/fonts/NotoSans-Regular.ttf");
    const NOTO_CJK_DATA: &[u8] =
        include_bytes!("../../../../assets/fonts/NotoSansCJKsc-Regular.otf");

    #[test]
    fn commit_run_storage_reuses_the_mesh_handle() {
        let mut backend = GlyphCache::default();
        let prepared = prepare(&mut backend, "Typography");
        let mut meshes = Assets::<Mesh>::default();
        let key = RunStorageKey(1);

        let first_data = backend
            .build_run_render_data(&prepared, None)
            .expect("fixture storage should build");
        let first = backend.commit_run_storage(key, first_data, &mut meshes);
        let second_data = backend
            .build_run_render_data(&prepared, None)
            .expect("fixture storage should build");
        let second = backend.commit_run_storage(key, second_data, &mut meshes);

        assert_eq!(first.mesh, second.mesh);
        assert_eq!(
            meshes.len(),
            1,
            "the second write reuses the mesh asset instead of allocating another"
        );
    }

    #[test]
    fn run_storage_can_be_removed_after_mesh_despawn() {
        let mut backend = GlyphCache::default();
        let prepared = prepare(&mut backend, "Typography");
        let mut meshes = Assets::<Mesh>::default();
        let key = RunStorageKey(1);

        let render_data = backend
            .build_run_render_data(&prepared, None)
            .expect("fixture storage should build");
        backend.commit_run_storage(key, render_data, &mut meshes);

        assert!(backend.remove_run_storage(key).is_some());
        assert!(backend.remove_run_storage(key).is_none());
    }

    #[test]
    fn fully_clipped_run_does_not_allocate_zero_vertex_mesh() {
        let mut backend = GlyphCache::default();
        let prepared = prepare(&mut backend, "Typography");
        let meshes = Assets::<Mesh>::default();

        let err = backend
            .build_run_render_data(&prepared, Some([f32::MAX - 2.0, 0.0, f32::MAX - 1.0, 1.0]))
            .expect_err("fully clipped text should not allocate render storage");

        assert_eq!(err, RunRenderError::NoVisibleGlyphs);
        assert_eq!(backend.run_storage_len(), 0);
        assert_eq!(meshes.len(), 0);
    }

    #[test]
    fn commit_glyph_atlas_uploads_one_buffer_each_then_reuses_them() {
        let mut backend = GlyphCache::default();
        prepare(&mut backend, "Typography");
        let mut storage_buffers = Assets::<ShaderBuffer>::default();

        let first = backend
            .commit_glyph_atlas(&mut storage_buffers)
            .expect("packed glyphs should produce atlas handles");
        assert_eq!(
            storage_buffers.len(),
            3,
            "the atlas is one curves, one bands, and one glyphs buffer"
        );

        let second = backend
            .commit_glyph_atlas(&mut storage_buffers)
            .expect("atlas handles persist across commits");
        assert_eq!(first.curves, second.curves);
        assert_eq!(first.bands, second.bands);
        assert_eq!(first.glyphs, second.glyphs);
        assert_eq!(
            storage_buffers.len(),
            3,
            "no newly packed glyph means no new atlas buffer"
        );
    }

    #[test]
    fn commit_glyph_atlas_with_no_packed_glyphs_creates_no_buffer() {
        let mut backend = GlyphCache::default();
        let mut storage_buffers = Assets::<ShaderBuffer>::default();

        assert!(backend.commit_glyph_atlas(&mut storage_buffers).is_none());
        assert_eq!(storage_buffers.len(), 0);
    }

    #[test]
    fn text_runs_skip_invisible_space_glyphs() {
        let mut backend = GlyphCache::default();
        let prepared = prepare(&mut backend, "A A");

        assert_eq!(prepared.glyph_count(), 2);
    }

    #[test]
    fn space_only_text_run_is_invisible_not_failed() {
        let mut backend = GlyphCache::default();
        let prepared = prepare(&mut backend, " ");

        assert_eq!(prepared.glyph_count(), 0);
    }

    #[test]
    fn font_matrix_supports_quadratic_and_cubic_outlines() {
        let latin_fonts = [
            (FONT_DATA, 101_u64),
            (NOTO_SANS_DATA, 102),
            (EB_GARAMOND_DATA, 103),
            (CRIMSON_TEXT_DATA, 104),
            (LIBERATION_SANS_DATA, 105),
        ];

        for (font_data, font_key) in latin_fonts {
            let mut backend = GlyphCache::default();
            let prepared =
                support::prepare_fixture_run(&mut backend, font_data, font_key, "Typography")
                    .expect("Latin font fixture should prepare");
            assert_eq!(prepared.glyph_count(), 10);
        }

        let mut backend = GlyphCache::default();
        let prepared = support::prepare_fixture_run(&mut backend, NOTO_CJK_DATA, 106, "漢")
            .expect("CFF cubic font fixture should prepare through quadratic conversion");
        assert_eq!(prepared.glyph_count(), 1);
    }

    fn prepare(backend: &mut GlyphCache, text: &str) -> PreparedTextRun {
        support::prepare_fixture_run(backend, FONT_DATA, 11, text)
            .expect("fixture text should prepare")
    }
}
