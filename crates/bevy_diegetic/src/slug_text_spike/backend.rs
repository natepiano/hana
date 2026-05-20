//! Slug backend state for the isolated feasibility renderer.

use std::collections::HashMap;

use bevy::prelude::Assets;
use bevy::prelude::Component;
use bevy::prelude::Event;
use bevy::prelude::Handle;
use bevy::prelude::Mesh;
use bevy::prelude::Resource;
use bevy::prelude::Vec2;
use bevy::render::storage::ShaderStorageBuffer;
use ttf_parser::Face;

use super::geometry;
use super::geometry::SlugOutlineError;
use super::packing::DEFAULT_BAND_COUNT;
use super::run;
use super::run::SlugBuiltTextRun;
use super::run::SlugFontKey;
use super::run::SlugGlyphCache;
use super::run::SlugGlyphInstance;
use super::run::SlugGlyphKey;
use super::run::SlugTextRun;
use super::run_render;
use super::run_render::SlugRunRenderError;
use crate::render::PositionedGlyph;

/// Request for one Slug text run.
#[derive(Clone, Copy, Debug)]
pub struct SlugTextRequest<'a> {
    /// Text to process.
    pub text:               &'a str,
    /// Exact font bytes used by the request.
    pub font_data:          &'a [u8],
    /// Stable caller-owned identity for `font_data`.
    pub font_key:           SlugFontKey,
    /// Family name registered with parley for this request.
    pub font_family:        &'a str,
    /// Scale from font design units to caller world units.
    pub world_scale:        f32,
    /// Number of horizontal Slug bands to build per glyph.
    pub band_count:         usize,
    /// Slug preprocessing version used for cache invalidation.
    pub preprocess_version: u32,
}

impl<'a> SlugTextRequest<'a> {
    /// Creates a request using the default band count.
    #[must_use]
    pub const fn new(
        text: &'a str,
        font_data: &'a [u8],
        font_key: SlugFontKey,
        font_family: &'a str,
        world_scale: f32,
    ) -> Self {
        Self {
            text,
            font_data,
            font_key,
            font_family,
            world_scale,
            band_count: DEFAULT_BAND_COUNT,
            preprocess_version: 0,
        }
    }
}

/// Slug preprocessing completion signal.
#[derive(Clone, Copy, Debug, Event)]
pub struct SlugBackendCompleted {
    /// Backend generation that completed.
    pub generation:    u64,
    /// Number of unique packed glyphs now resident in the backend cache.
    pub packed_glyphs: usize,
}

/// Result of preparing one Slug text run.
#[derive(Clone, Debug)]
pub struct SlugPreparedTextRun {
    /// Prepared text run.
    pub run:         SlugBuiltTextRun,
    /// Backend-owned storage key for this run.
    pub storage_key: SlugRunStorageKey,
    /// Completion event callers can trigger to wake pending text.
    pub completion:  SlugBackendCompleted,
}

/// Backend key for one prepared run's GPU storage handles.
#[derive(Clone, Copy, Component, Debug, Eq, Hash, PartialEq)]
pub struct SlugRunStorageKey(u64);

impl SlugRunStorageKey {
    /// Raw key value.
    #[must_use]
    pub const fn value(self) -> u64 { self.0 }
}

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

/// Experimental Slug backend resource.
#[derive(Debug, Default, Resource)]
pub struct SlugBackend {
    glyph_cache:        SlugGlyphCache,
    run_storage:        HashMap<SlugRunStorageKey, SlugRunStorage>,
    generation:         u64,
    next_storage_key:   u64,
    failed_runs:        usize,
    completed_runs:     usize,
    last_completion:    Option<SlugBackendCompleted>,
    preprocess_version: u32,
}

impl SlugBackend {
    /// Prepares one text run and updates the backend glyph cache.
    pub fn prepare_text_run(
        &mut self,
        request: SlugTextRequest<'_>,
    ) -> Result<SlugPreparedTextRun, SlugOutlineError> {
        let request = SlugTextRequest {
            preprocess_version: self.preprocess_version,
            ..request
        };
        let run = run::build_slug_text_run_with_cache(request, &mut self.glyph_cache)?;
        Ok(self.finish_prepared_run(run))
    }

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
                positioned.glyph.advance * placement_scale.x,
                packed_glyph.bounds(),
                bounds_scale,
            ));
        }

        Ok(self.finish_prepared_run(SlugBuiltTextRun {
            run:            SlugTextRun::new(instances),
            baseline:       0.0,
            reference_size: layout_font_size,
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

    /// Records a failed Slug request.
    pub const fn record_failure(&mut self) {
        self.failed_runs = self.failed_runs.saturating_add(1);
    }

    /// Reusable packed glyph cache owned by this backend.
    #[must_use]
    pub const fn glyph_cache(&self) -> &SlugGlyphCache { &self.glyph_cache }

    /// Current backend generation.
    #[must_use]
    pub const fn generation(&self) -> u64 { self.generation }

    /// Number of successfully prepared runs.
    #[must_use]
    pub const fn completed_runs(&self) -> usize { self.completed_runs }

    /// Number of failed Slug run requests.
    #[must_use]
    pub const fn failed_runs(&self) -> usize { self.failed_runs }

    /// Last completion signal produced by this backend.
    #[must_use]
    pub const fn last_completion(&self) -> Option<SlugBackendCompleted> { self.last_completion }

    /// Number of prepared runs with backend-owned GPU handles.
    #[must_use]
    pub fn stored_runs(&self) -> usize { self.run_storage.len() }

    /// Removes one prepared run's backend-owned GPU handles.
    pub fn remove_run_storage(&mut self, key: SlugRunStorageKey) -> Option<SlugRunStorage> {
        self.run_storage.remove(&key)
    }

    /// Removes every backend-owned run storage handle.
    pub fn clear_run_storage(&mut self) { self.run_storage.clear(); }

    /// Slug preprocessing version used by glyph cache keys.
    #[must_use]
    pub const fn preprocess_version(&self) -> u32 { self.preprocess_version }

    /// Stable key for a glyph in the current backend preprocessing profile.
    #[must_use]
    pub const fn glyph_key(&self, font: SlugFontKey, glyph_id: u16) -> SlugGlyphKey {
        SlugGlyphKey::with_preprocess_version(font, glyph_id, self.preprocess_version)
    }

    fn finish_prepared_run(&mut self, run: SlugBuiltTextRun) -> SlugPreparedTextRun {
        self.generation = self.generation.saturating_add(1);
        self.completed_runs = self.completed_runs.saturating_add(1);
        let completion = SlugBackendCompleted {
            generation:    self.generation,
            packed_glyphs: self.glyph_cache.len(),
        };
        let storage_key = self.next_run_storage_key();
        self.last_completion = Some(completion);
        SlugPreparedTextRun {
            run,
            storage_key,
            completion,
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

    use super::*;

    const FONT_DATA: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
    const FONT_FAMILY: &str = "JetBrains Mono";
    const FONT_KEY: SlugFontKey = SlugFontKey::new(11);
    const FONT_SCALE: f32 = 0.001;
    const CRIMSON_TEXT_DATA: &[u8] = include_bytes!("../../assets/fonts/CrimsonText-Regular.ttf");
    const EB_GARAMOND_DATA: &[u8] = include_bytes!("../../assets/fonts/EBGaramond-Regular.ttf");
    const LIBERATION_SANS_DATA: &[u8] =
        include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf");
    const NOTO_SANS_DATA: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Regular.ttf");
    const NOTO_CJK_DATA: &[u8] = include_bytes!("../../assets/fonts/NotoSansCJKsc-Regular.otf");

    #[test]
    fn prepared_runs_receive_distinct_storage_keys() {
        let mut backend = SlugBackend::default();
        let first = prepare(&mut backend, "Typography");
        let second = prepare(&mut backend, "Typography");

        assert_ne!(first.storage_key, second.storage_key);
        assert_eq!(backend.completed_runs(), 2);
        assert_eq!(backend.stored_runs(), 0);
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

        assert_eq!(backend.stored_runs(), 1);
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
        assert_eq!(backend.stored_runs(), 1);

        let removed = backend.remove_run_storage(prepared.storage_key);
        assert!(removed.is_some());
        assert_eq!(backend.stored_runs(), 0);
    }

    #[test]
    fn text_runs_skip_invisible_space_glyphs() {
        let mut backend = SlugBackend::default();
        let prepared = prepare(&mut backend, "A A");

        assert_eq!(prepared.run.run.glyphs().len(), 2);
        assert_eq!(backend.glyph_cache().len(), 1);
        assert!(prepared.run.run.advance_width() > 0.0);
    }

    #[test]
    fn space_only_text_run_is_invisible_not_failed() {
        let mut backend = SlugBackend::default();
        let prepared = prepare(&mut backend, " ");

        assert!(prepared.run.run.glyphs().is_empty());
        assert!(backend.glyph_cache().is_empty());
        assert_eq!(backend.completed_runs(), 1);
        assert_eq!(backend.failed_runs(), 0);
    }

    #[test]
    fn phase8_font_matrix_documents_current_outline_support() {
        let supported_fonts = [
            ("JetBrains Mono", FONT_DATA, SlugFontKey::new(101)),
            ("Noto Sans", NOTO_SANS_DATA, SlugFontKey::new(102)),
            ("EB Garamond", EB_GARAMOND_DATA, SlugFontKey::new(103)),
            ("Crimson Text", CRIMSON_TEXT_DATA, SlugFontKey::new(104)),
            (
                "Liberation Sans",
                LIBERATION_SANS_DATA,
                SlugFontKey::new(105),
            ),
        ];

        for (family, font_data, font_key) in supported_fonts {
            let mut backend = SlugBackend::default();
            let prepared = backend
                .prepare_text_run(SlugTextRequest::new(
                    "Typography",
                    font_data,
                    font_key,
                    family,
                    FONT_SCALE,
                ))
                .expect("Phase 8 Latin font fixture should prepare");
            assert_eq!(prepared.run.run.glyphs().len(), 10);
            assert!(!backend.glyph_cache().is_empty());
        }

        let mut backend = SlugBackend::default();
        let unsupported = backend.prepare_text_run(SlugTextRequest::new(
            "漢",
            NOTO_CJK_DATA,
            SlugFontKey::new(106),
            "Noto Sans CJK SC",
            FONT_SCALE,
        ));
        assert!(matches!(
            unsupported,
            Err(SlugOutlineError::CubicOutline {
                character: '漢',
                ..
            })
        ));
    }

    fn prepare(backend: &mut SlugBackend, text: &str) -> SlugPreparedTextRun {
        backend
            .prepare_text_run(SlugTextRequest::new(
                text,
                FONT_DATA,
                FONT_KEY,
                FONT_FAMILY,
                FONT_SCALE,
            ))
            .expect("fixture text should prepare")
    }
}
