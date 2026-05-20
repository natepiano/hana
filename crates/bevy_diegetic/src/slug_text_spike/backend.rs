//! Slug backend state for the isolated feasibility renderer.

use bevy::prelude::Event;
use bevy::prelude::Resource;

use super::geometry::SlugOutlineError;
use super::packing::DEFAULT_BAND_COUNT;
use super::run;
use super::run::SlugBuiltTextRun;
use super::run::SlugFontKey;
use super::run::SlugGlyphCache;
use super::run::SlugGlyphKey;

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
    pub run:        SlugBuiltTextRun,
    /// Completion event callers can trigger to wake pending text.
    pub completion: SlugBackendCompleted,
}

/// Experimental Slug backend resource.
#[derive(Debug, Default, Resource)]
pub struct SlugBackend {
    glyph_cache:        SlugGlyphCache,
    generation:         u64,
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
        self.generation = self.generation.saturating_add(1);
        self.completed_runs = self.completed_runs.saturating_add(1);
        let completion = SlugBackendCompleted {
            generation:    self.generation,
            packed_glyphs: self.glyph_cache.len(),
        };
        self.last_completion = Some(completion);
        Ok(SlugPreparedTextRun { run, completion })
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

    /// Slug preprocessing version used by glyph cache keys.
    #[must_use]
    pub const fn preprocess_version(&self) -> u32 { self.preprocess_version }

    /// Stable key for a glyph in the current backend preprocessing profile.
    #[must_use]
    pub const fn glyph_key(&self, font: SlugFontKey, glyph_id: u16) -> SlugGlyphKey {
        SlugGlyphKey::with_preprocess_version(font, glyph_id, self.preprocess_version)
    }
}
