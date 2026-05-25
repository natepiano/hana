//! Slug backend state: glyph cache, prepared runs, and GPU storage handles.

use std::collections::HashMap;

use bevy::prelude::Assets;
use bevy::prelude::Component;
use bevy::prelude::Handle;
use bevy::prelude::Mesh;
use bevy::prelude::Resource;
use bevy::prelude::Vec2;
use bevy::render::storage::ShaderStorageBuffer;
use ttf_parser::Face;

use super::geometry;
use super::geometry::SlugOutlineError;
use super::run::SlugBuiltTextRun;
use super::run::SlugFontKey;
use super::run::SlugGlyphCache;
use super::run::SlugGlyphInstance;
use super::run::SlugGlyphKey;
use super::run::SlugTextRun;
use super::run_render;
use super::run_render::SlugRunRenderError;
use crate::render::PositionedGlyph;

/// Result of preparing one Slug text run.
#[derive(Clone, Debug)]
pub struct SlugPreparedTextRun {
    /// Prepared text run.
    pub run:         SlugBuiltTextRun,
    /// Backend-owned storage key for this run.
    pub storage_key: SlugRunStorageKey,
}

/// Backend key for one prepared run's GPU storage handles.
#[derive(Clone, Copy, Component, Debug, Eq, Hash, PartialEq)]
pub struct SlugRunStorageKey(u64);

/// Backend-owned GPU handles for one prepared Slug run.
#[derive(Clone, Debug)]
pub struct SlugRunStorage {
    /// One quad per glyph instance.
    pub mesh:   Handle<Mesh>,
    /// Band-packed quadratic curve records.
    pub curves: Handle<ShaderStorageBuffer>,
    /// Horizontal band records.
    pub bands:  Handle<ShaderStorageBuffer>,
    /// Unique glyph records.
    pub glyphs: Handle<ShaderStorageBuffer>,
}

/// Slug backend resource: owns the glyph cache and prepared-run GPU storage.
#[derive(Debug, Default, Resource)]
pub struct SlugBackend {
    glyph_cache:        SlugGlyphCache,
    run_storage:        HashMap<SlugRunStorageKey, SlugRunStorage>,
    next_storage_key:   u64,
    preprocess_version: u32,
}

impl SlugBackend {
    /// Prepares one run from already-positioned production glyphs.
    pub(crate) fn prepare_positioned_run(
        &mut self,
        glyphs: &[PositionedGlyph<'_>],
        anchor: Vec2,
        layout_font_size: f32,
        world_scale: f32,
        band_count: usize,
    ) -> Result<SlugPreparedTextRun, SlugOutlineError> {
        self.prepare_positioned_run_with_scale(
            glyphs,
            anchor,
            layout_font_size,
            Vec2::splat(world_scale),
            band_count,
        )
    }

    /// Prepares one run from already-positioned production glyphs with
    /// caller-provided X/Y placement scale.
    pub(crate) fn prepare_positioned_run_with_scale(
        &mut self,
        glyphs: &[PositionedGlyph<'_>],
        anchor: Vec2,
        layout_font_size: f32,
        placement_scale: Vec2,
        band_count: usize,
    ) -> Result<SlugPreparedTextRun, SlugOutlineError> {
        let mut instances = Vec::with_capacity(glyphs.len());
        for positioned in glyphs {
            let face = Face::parse(positioned.font.data(), positioned.font.collection_index)
                .map_err(|_| SlugOutlineError::InvalidFont)?;
            if !geometry::glyph_id_has_visible_outline(&face, positioned.glyph.id) {
                continue;
            }
            let bounds_scale =
                placement_scale * (layout_font_size / f32::from(face.units_per_em()));
            let key = self.glyph_key(
                SlugFontKey::new(positioned.glyph.font_face.blob_id),
                positioned.glyph.id,
            );
            let packed_glyph = self.glyph_cache.get_or_insert_packed_from_face(
                key,
                positioned.font.data(),
                positioned.font.collection_index,
                POSITIONED_GLYPH_DIAGNOSTIC_CHAR,
                band_count,
            )?;
            instances.push(SlugGlyphInstance::new_non_uniform(
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

        Ok(self.finish_prepared_run(SlugBuiltTextRun {
            run: SlugTextRun::new(instances),
        }))
    }

    /// Ensures this prepared run has backend-owned mesh and storage handles.
    pub fn ensure_run_storage(
        &mut self,
        prepared: &SlugPreparedTextRun,
        clip_rect: Option<[f32; 4]>,
        meshes: &mut Assets<Mesh>,
        storage_buffers: &mut Assets<ShaderStorageBuffer>,
    ) -> Result<SlugRunStorage, SlugRunRenderError> {
        if let Some(storage) = self.run_storage.get(&prepared.storage_key) {
            return Ok(storage.clone());
        }

        let render_data = run_render::build_slug_run_render_data_with_clip(
            &prepared.run,
            &self.glyph_cache,
            1.0,
            clip_rect,
        )?;
        let storage = SlugRunStorage {
            mesh:   meshes.add(render_data.mesh),
            curves: storage_buffers.add(ShaderStorageBuffer::from(render_data.curves)),
            bands:  storage_buffers.add(ShaderStorageBuffer::from(render_data.bands)),
            glyphs: storage_buffers.add(ShaderStorageBuffer::from(render_data.glyphs)),
        };
        self.run_storage
            .insert(prepared.storage_key, storage.clone());
        Ok(storage)
    }

    /// Removes one prepared run's backend-owned GPU handles.
    pub fn remove_run_storage(&mut self, key: SlugRunStorageKey) -> Option<SlugRunStorage> {
        self.run_storage.remove(&key)
    }

    /// Removes every backend-owned run storage handle.
    pub fn clear_run_storage(&mut self) { self.run_storage.clear(); }

    /// Stable key for a glyph in the current backend preprocessing profile.
    #[must_use]
    pub const fn glyph_key(&self, font: SlugFontKey, glyph_id: u16) -> SlugGlyphKey {
        SlugGlyphKey::with_preprocess_version(font, glyph_id, self.preprocess_version)
    }

    const fn finish_prepared_run(&mut self, run: SlugBuiltTextRun) -> SlugPreparedTextRun {
        SlugPreparedTextRun {
            run,
            storage_key: self.next_run_storage_key(),
        }
    }

    const fn next_run_storage_key(&mut self) -> SlugRunStorageKey {
        let key = SlugRunStorageKey(self.next_storage_key);
        self.next_storage_key = self.next_storage_key.saturating_add(1);
        key
    }
}

const POSITIONED_GLYPH_DIAGNOSTIC_CHAR: char = '\u{FFFD}';

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should fail loudly when fixture Slug runs cannot be prepared"
)]
mod tests {
    use bevy::prelude::Assets;

    use super::super::test_support::prepare_fixture_run;
    use super::*;

    const FONT_DATA: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");
    const CRIMSON_TEXT_DATA: &[u8] =
        include_bytes!("../../../assets/fonts/CrimsonText-Regular.ttf");
    const EB_GARAMOND_DATA: &[u8] = include_bytes!("../../../assets/fonts/EBGaramond-Regular.ttf");
    const LIBERATION_SANS_DATA: &[u8] =
        include_bytes!("../../../assets/fonts/LiberationSans-Regular.ttf");
    const NOTO_SANS_DATA: &[u8] = include_bytes!("../../../assets/fonts/NotoSans-Regular.ttf");
    const NOTO_CJK_DATA: &[u8] = include_bytes!("../../../assets/fonts/NotoSansCJKsc-Regular.otf");

    #[test]
    fn prepared_runs_receive_distinct_storage_keys() {
        let mut backend = SlugBackend::default();
        let first = prepare(&mut backend, "Typography");
        let second = prepare(&mut backend, "Typography");

        assert_ne!(first.storage_key, second.storage_key);
    }

    #[test]
    fn ensure_run_storage_reuses_existing_handles() {
        let mut backend = SlugBackend::default();
        let prepared = prepare(&mut backend, "Typography");
        let mut meshes = Assets::<Mesh>::default();
        let mut storage_buffers = Assets::<ShaderStorageBuffer>::default();

        let first = backend
            .ensure_run_storage(&prepared, None, &mut meshes, &mut storage_buffers)
            .expect("fixture storage should build");
        let second = backend
            .ensure_run_storage(&prepared, None, &mut meshes, &mut storage_buffers)
            .expect("fixture storage should be reused");

        assert_eq!(first.mesh, second.mesh);
        assert_eq!(first.curves, second.curves);
        assert_eq!(first.bands, second.bands);
        assert_eq!(first.glyphs, second.glyphs);
    }

    #[test]
    fn run_storage_can_be_removed_after_mesh_despawn() {
        let mut backend = SlugBackend::default();
        let prepared = prepare(&mut backend, "Typography");
        let mut meshes = Assets::<Mesh>::default();
        let mut storage_buffers = Assets::<ShaderStorageBuffer>::default();

        backend
            .ensure_run_storage(&prepared, None, &mut meshes, &mut storage_buffers)
            .expect("fixture storage should build");

        assert!(backend.remove_run_storage(prepared.storage_key).is_some());
        assert!(backend.remove_run_storage(prepared.storage_key).is_none());
    }

    #[test]
    fn text_runs_skip_invisible_space_glyphs() {
        let mut backend = SlugBackend::default();
        let prepared = prepare(&mut backend, "A A");

        assert_eq!(prepared.run.run.glyphs().len(), 2);
    }

    #[test]
    fn space_only_text_run_is_invisible_not_failed() {
        let mut backend = SlugBackend::default();
        let prepared = prepare(&mut backend, " ");

        assert!(prepared.run.run.glyphs().is_empty());
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
            let mut backend = SlugBackend::default();
            let prepared = prepare_fixture_run(&mut backend, font_data, font_key, "Typography")
                .expect("Latin font fixture should prepare");
            assert_eq!(prepared.run.run.glyphs().len(), 10);
        }

        let mut backend = SlugBackend::default();
        let prepared = prepare_fixture_run(&mut backend, NOTO_CJK_DATA, 106, "漢")
            .expect("CFF cubic font fixture should prepare through quadratic conversion");
        assert_eq!(prepared.run.run.glyphs().len(), 1);
    }

    fn prepare(backend: &mut SlugBackend, text: &str) -> SlugPreparedTextRun {
        prepare_fixture_run(backend, FONT_DATA, 11, text).expect("fixture text should prepare")
    }
}
