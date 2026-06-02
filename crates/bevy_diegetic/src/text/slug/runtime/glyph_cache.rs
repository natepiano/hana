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

/// GPU storage handles for one prepared run.
#[derive(Clone, Debug)]
pub(crate) struct RunStorage {
    /// One quad per glyph instance.
    pub mesh:   Handle<Mesh>,
    /// Band-packed quadratic curve records.
    pub curves: Handle<ShaderBuffer>,
    /// Horizontal band records.
    pub bands:  Handle<ShaderBuffer>,
    /// Unique glyph records.
    pub glyphs: Handle<ShaderBuffer>,
}

/// `GlyphCache` resource: owns the glyph cache and per-run GPU storage.
#[derive(Debug, Default, Resource)]
pub(crate) struct GlyphCache {
    outline_cache:      GlyphOutlineCache,
    run_storage:        HashMap<RunStorageKey, RunStorage>,
    preprocess_version: u32,
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

    /// Builds this run's geometry and writes it to the storage keyed by
    /// `storage_key`, overwriting the existing mesh and buffers in place when the
    /// key is already present and allocating new assets the first time it is
    /// seen.
    ///
    /// In-place writes keep the same `Handle<Mesh>` / `Handle<ShaderBuffer>`, so
    /// the render world re-uploads only the changed bytes — no per-frame asset
    /// allocate-and-drop, and the run's mesh entity is never respawned. A run
    /// with no visible glyph quads (fully clipped) writes nothing and returns
    /// `NoVisibleGlyphs`.
    /// Builds this run's GPU-ready render data from cached glyph outlines,
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
        // clipped text runs should therefore not create mesh/storage assets.
        if render_data.mesh.count_vertices() == 0 {
            return Err(RunRenderError::NoVisibleGlyphs);
        }
        Ok(render_data)
    }

    /// Writes `render_data` to the storage keyed by `storage_key`, overwriting
    /// the existing mesh and buffers in place when the key is present and
    /// allocating new assets the first time it is seen. Pair it with
    /// [`Self::build_run_render_data`]; the two are split so a caller can time
    /// the build and upload halves separately.
    ///
    /// In-place writes keep the same `Handle<Mesh>` / `Handle<ShaderBuffer>`, so
    /// the render world re-uploads only the changed bytes — no per-frame asset
    /// allocate-and-drop, and the run's mesh entity is never respawned.
    pub fn commit_run_storage(
        &mut self,
        storage_key: RunStorageKey,
        render_data: RunRenderData,
        meshes: &mut Assets<Mesh>,
        storage_buffers: &mut Assets<ShaderBuffer>,
    ) -> RunStorage {
        if let Some(storage) = self.run_storage.get(&storage_key) {
            // Same label, changed text: overwrite the existing assets behind
            // their stable handles. `get_mut` marks each asset modified, so the
            // render world re-uploads the new bytes without a handle swap.
            if let Some(mut mesh) = meshes.get_mut(&storage.mesh) {
                *mesh = render_data.mesh;
            }
            if let Some(mut curves) = storage_buffers.get_mut(&storage.curves) {
                curves.set_data(render_data.curves);
            }
            if let Some(mut bands) = storage_buffers.get_mut(&storage.bands) {
                bands.set_data(render_data.bands);
            }
            if let Some(mut glyphs) = storage_buffers.get_mut(&storage.glyphs) {
                glyphs.set_data(render_data.glyphs);
            }
            return storage.clone();
        }
        let storage = RunStorage {
            mesh:   meshes.add(render_data.mesh),
            curves: storage_buffers.add(ShaderBuffer::from(render_data.curves)),
            bands:  storage_buffers.add(ShaderBuffer::from(render_data.bands)),
            glyphs: storage_buffers.add(ShaderBuffer::from(render_data.glyphs)),
        };
        self.run_storage.insert(storage_key, storage.clone());
        storage
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
    fn ensure_run_storage_reuses_existing_handles() {
        let mut backend = GlyphCache::default();
        let prepared = prepare(&mut backend, "Typography");
        let mut meshes = Assets::<Mesh>::default();
        let mut storage_buffers = Assets::<ShaderBuffer>::default();
        let key = RunStorageKey(1);

        let first_data = backend
            .build_run_render_data(&prepared, None)
            .expect("fixture storage should build");
        let first = backend.commit_run_storage(key, first_data, &mut meshes, &mut storage_buffers);
        let second_data = backend
            .build_run_render_data(&prepared, None)
            .expect("fixture storage should build");
        let second =
            backend.commit_run_storage(key, second_data, &mut meshes, &mut storage_buffers);

        assert_eq!(first.mesh, second.mesh);
        assert_eq!(first.curves, second.curves);
        assert_eq!(first.bands, second.bands);
        assert_eq!(first.glyphs, second.glyphs);
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
        let mut storage_buffers = Assets::<ShaderBuffer>::default();
        let key = RunStorageKey(1);

        let render_data = backend
            .build_run_render_data(&prepared, None)
            .expect("fixture storage should build");
        backend.commit_run_storage(key, render_data, &mut meshes, &mut storage_buffers);

        assert!(backend.remove_run_storage(key).is_some());
        assert!(backend.remove_run_storage(key).is_none());
    }

    #[test]
    fn fully_clipped_run_does_not_allocate_zero_vertex_mesh() {
        let mut backend = GlyphCache::default();
        let prepared = prepare(&mut backend, "Typography");
        let meshes = Assets::<Mesh>::default();
        let storage_buffers = Assets::<ShaderBuffer>::default();

        let err = backend
            .build_run_render_data(&prepared, Some([f32::MAX - 2.0, 0.0, f32::MAX - 1.0, 1.0]))
            .expect_err("fully clipped text should not allocate render storage");

        assert_eq!(err, RunRenderError::NoVisibleGlyphs);
        assert_eq!(backend.run_storage_len(), 0);
        assert_eq!(meshes.len(), 0);
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
