//! Panel performance statistics and diagnostics publishing.

use bevy::diagnostic::Diagnostic;
use bevy::diagnostic::Diagnostics;
use bevy::diagnostic::RegisterDiagnostic;
use bevy::prelude::*;
use bevy_kana::ToF64;

use super::constants::DIAG_ATLAS_ACTIVE_JOBS;
use super::constants::DIAG_ATLAS_AVG_RASTER_MS;
use super::constants::DIAG_ATLAS_BATCH_MAX_ACTIVE_JOBS;
use super::constants::DIAG_ATLAS_COMPLETED_GLYPHS;
use super::constants::DIAG_ATLAS_DIRTY_PAGES;
use super::constants::DIAG_ATLAS_IN_FLIGHT_GLYPHS;
use super::constants::DIAG_ATLAS_INSERTED_GLYPHS;
use super::constants::DIAG_ATLAS_INVISIBLE_GLYPHS;
use super::constants::DIAG_ATLAS_MAX_RASTER_MS;
use super::constants::DIAG_ATLAS_PAGES_ADDED;
use super::constants::DIAG_ATLAS_PEAK_ACTIVE_JOBS;
use super::constants::DIAG_ATLAS_POLL_MS;
use super::constants::DIAG_ATLAS_SYNC_MS;
use super::constants::DIAG_ATLAS_TOTAL_GLYPHS;
use super::constants::DIAG_ATLAS_WORKER_THREADS;
use super::constants::DIAG_LAYOUT_COMPUTE_MS;
use super::constants::DIAG_LAYOUT_COMPUTE_PANELS;
use super::constants::DIAG_PANEL_TEXT_ATLAS_LOOKUP_MS;
use super::constants::DIAG_PANEL_TEXT_MESH_BUILD_MS;
use super::constants::DIAG_PANEL_TEXT_PARLEY_MS;
use super::constants::DIAG_PANEL_TEXT_PENDING_GLYPHS;
use super::constants::DIAG_PANEL_TEXT_QUEUED_GLYPHS;
use super::constants::DIAG_PANEL_TEXT_SHAPE_MS;
use super::constants::DIAG_PANEL_TEXT_SHAPED_PANELS;
use super::constants::DIAG_PANEL_TEXT_TOTAL_MS;

/// Lightweight timing data for diegetic UI systems.
///
/// These values are updated by the built-in layout and text extraction systems
/// so examples and applications can inspect where time is being spent during
/// content-heavy updates.
///
/// **Note:** This API is provisional. Field names and structure are coupled
/// to the current internal system architecture and may change as the
/// library matures. Consider using Bevy's `DiagnosticsStore` for
/// production profiling.
#[derive(Resource, Clone, Debug, Default, Reflect)]
#[reflect(Resource)]
pub struct DiegeticPerfStats {
    /// Stage 1 — `compute_panel_layouts` wall time in milliseconds.
    /// Layout math: element positions and sizes for every dirty panel.
    pub compute_ms:     f32,
    /// Stage 1 — panels processed by the most recent layout run.
    pub compute_panels: usize,
    /// Stages 2 & 3 — panel-text shape + mesh-build timings and counts.
    pub panel_text:     PanelTextPerfStats,
    /// Async MSDF atlas polling timings and glyph-job counts.
    pub atlas:          AtlasPerfStats,
}

/// Panel-text per-frame timings. Covers stages 2 and 3 of the panel pipeline:
///
/// 1. `compute_panel_layouts` → [`DiegeticPerfStats::compute_ms`]
/// 2. `shape_panel_text_children` → [`Self::shape_ms`] (strings → glyph quads)
/// 3. `build_panel_batched_meshes` → [`Self::mesh_build_ms`] (quads → mesh entities)
///
/// Render-pass time is not measured here — it lives in Bevy's own diagnostics
/// (`FrameTimeDiagnosticsPlugin`, `RenderDiagnosticsPlugin`) and is outside this
/// crate's control.
///
/// Only panel text is covered. Standalone `WorldText` entities not parented to a
/// panel run through a separate path (`render_world_text`) and are not included.
#[derive(Clone, Debug, Default, Reflect)]
pub struct PanelTextPerfStats {
    /// End-to-end panel-text wall time this frame, in milliseconds.
    /// Equals [`Self::shape_ms`] + [`Self::mesh_build_ms`].
    ///
    /// Written twice per frame: first by `shape_panel_text_children` using
    /// the *previous* frame's `mesh_build_ms`, then overwritten by
    /// `build_panel_batched_meshes` using the current frame's values. The
    /// final value is only correct because mesh build is scheduled
    /// `.after(shape_panel_text_children)`; reordering those systems would
    /// leave `total_ms` stale by one frame.
    pub total_ms:        f32,
    /// Stage 2 — wall time of `shape_panel_text_children` this frame.
    /// Covers string → glyph-quad shaping for every panel-text entity that
    /// changed or is waiting on async glyph rasterization.
    pub shape_ms:        f32,
    /// Inside [`Self::shape_ms`] — time spent in parley text shaping,
    /// summed across entities. If this dominates, the cost is content-side
    /// (many strings, complex scripts, heavy font features).
    pub parley_ms:       f32,
    /// Inside [`Self::shape_ms`] — time spent in MSDF atlas lookups and
    /// async glyph queueing, summed across entities. If this dominates,
    /// the cost is atlas-side (cache misses, new glyph dispatch).
    pub atlas_lookup_ms: f32,
    /// Stage 3 — wall time of `build_panel_batched_meshes` this frame.
    /// Covers batching `PanelTextQuads` into per-page mesh entities and
    /// despawning stale meshes.
    pub mesh_build_ms:   f32,
    /// Number of panels whose text was re-shaped this frame.
    pub shaped_panels:   usize,
    /// Glyphs newly queued for async rasterization during the shape pass.
    /// Non-zero in steady-state usually indicates new characters appearing
    /// (font change, new content).
    pub queued_glyphs:   usize,
    /// Glyphs still awaiting async rasterization at the end of the shape
    /// pass. Non-zero in steady-state means raster workers are saturated —
    /// cross-reference [`AtlasPerfStats::in_flight_glyphs`] and
    /// [`AtlasPerfStats::peak_active_jobs`].
    pub pending_glyphs:  usize,
}

/// Atlas-poll timings and glyph-job counts.
#[derive(Clone, Debug, Default, Reflect)]
pub struct AtlasPerfStats {
    /// Time spent draining async atlas results in the most recent atlas poll.
    pub poll_ms:               f32,
    /// Time spent syncing dirty atlas pages to GPU images in the most recent atlas poll.
    pub sync_ms:               f32,
    /// Number of completed async glyph jobs drained by the most recent atlas poll.
    pub completed_glyphs:      usize,
    /// Number of visible glyphs inserted into atlas pages by the most recent atlas poll.
    pub inserted_glyphs:       usize,
    /// Number of invisible glyph entries cached by the most recent atlas poll.
    pub invisible_glyphs:      usize,
    /// Number of atlas pages added by the most recent atlas poll.
    pub pages_added:           usize,
    /// Number of dirty atlas pages observed before the most recent GPU sync.
    pub dirty_pages:           usize,
    /// Number of glyph raster jobs still in flight after the most recent atlas poll.
    pub in_flight_glyphs:      usize,
    /// Number of glyph raster jobs actively executing at the end of the most recent atlas poll.
    pub active_jobs:           usize,
    /// Peak concurrently executing glyph raster jobs observed so far.
    pub peak_active_jobs:      usize,
    /// Number of distinct worker threads that completed jobs in the most recent atlas poll.
    pub worker_threads:        usize,
    /// Average worker-side glyph raster duration for the most recent drained batch.
    pub avg_raster_ms:         f32,
    /// Maximum worker-side glyph raster duration for the most recent drained batch.
    pub max_raster_ms:         f32,
    /// Highest active-job count reported by any job in the most recent drained batch.
    pub batch_max_active_jobs: usize,
    /// Total number of glyphs currently cached in the atlas.
    pub total_glyphs:          usize,
}

#[derive(Resource, Default)]
struct DiegeticDiagnosticsRegistered;

pub(super) struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        if app
            .world()
            .contains_resource::<DiegeticDiagnosticsRegistered>()
        {
            return;
        }

        app.insert_resource(DiegeticDiagnosticsRegistered);
        for diagnostic in [
            Diagnostic::new(DIAG_LAYOUT_COMPUTE_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_LAYOUT_COMPUTE_PANELS),
            Diagnostic::new(DIAG_PANEL_TEXT_TOTAL_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_PANEL_TEXT_SHAPE_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_PANEL_TEXT_PARLEY_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_PANEL_TEXT_ATLAS_LOOKUP_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_PANEL_TEXT_MESH_BUILD_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_PANEL_TEXT_SHAPED_PANELS),
            Diagnostic::new(DIAG_PANEL_TEXT_QUEUED_GLYPHS),
            Diagnostic::new(DIAG_PANEL_TEXT_PENDING_GLYPHS),
            Diagnostic::new(DIAG_ATLAS_POLL_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_ATLAS_SYNC_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_ATLAS_COMPLETED_GLYPHS),
            Diagnostic::new(DIAG_ATLAS_INSERTED_GLYPHS),
            Diagnostic::new(DIAG_ATLAS_INVISIBLE_GLYPHS),
            Diagnostic::new(DIAG_ATLAS_PAGES_ADDED),
            Diagnostic::new(DIAG_ATLAS_DIRTY_PAGES),
            Diagnostic::new(DIAG_ATLAS_IN_FLIGHT_GLYPHS),
            Diagnostic::new(DIAG_ATLAS_ACTIVE_JOBS),
            Diagnostic::new(DIAG_ATLAS_PEAK_ACTIVE_JOBS),
            Diagnostic::new(DIAG_ATLAS_WORKER_THREADS),
            Diagnostic::new(DIAG_ATLAS_AVG_RASTER_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_ATLAS_MAX_RASTER_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_ATLAS_BATCH_MAX_ACTIVE_JOBS),
            Diagnostic::new(DIAG_ATLAS_TOTAL_GLYPHS),
        ] {
            app.register_diagnostic(diagnostic);
        }

        app.add_systems(Last, publish_perf_diagnostics);
    }
}

fn publish_perf_diagnostics(perf: Res<DiegeticPerfStats>, mut diagnostics: Diagnostics) {
    diagnostics.add_measurement(&DIAG_LAYOUT_COMPUTE_MS, || f64::from(perf.compute_ms));
    diagnostics.add_measurement(&DIAG_LAYOUT_COMPUTE_PANELS, || perf.compute_panels.to_f64());
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_TOTAL_MS, || {
        f64::from(perf.panel_text.total_ms)
    });
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_SHAPE_MS, || {
        f64::from(perf.panel_text.shape_ms)
    });
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_PARLEY_MS, || {
        f64::from(perf.panel_text.parley_ms)
    });
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_ATLAS_LOOKUP_MS, || {
        f64::from(perf.panel_text.atlas_lookup_ms)
    });
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_MESH_BUILD_MS, || {
        f64::from(perf.panel_text.mesh_build_ms)
    });
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_SHAPED_PANELS, || {
        perf.panel_text.shaped_panels.to_f64()
    });
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_QUEUED_GLYPHS, || {
        perf.panel_text.queued_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_PENDING_GLYPHS, || {
        perf.panel_text.pending_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_POLL_MS, || f64::from(perf.atlas.poll_ms));
    diagnostics.add_measurement(&DIAG_ATLAS_SYNC_MS, || f64::from(perf.atlas.sync_ms));
    diagnostics.add_measurement(&DIAG_ATLAS_COMPLETED_GLYPHS, || {
        perf.atlas.completed_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_INSERTED_GLYPHS, || {
        perf.atlas.inserted_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_INVISIBLE_GLYPHS, || {
        perf.atlas.invisible_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_PAGES_ADDED, || perf.atlas.pages_added.to_f64());
    diagnostics.add_measurement(&DIAG_ATLAS_DIRTY_PAGES, || perf.atlas.dirty_pages.to_f64());
    diagnostics.add_measurement(&DIAG_ATLAS_IN_FLIGHT_GLYPHS, || {
        perf.atlas.in_flight_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_ACTIVE_JOBS, || perf.atlas.active_jobs.to_f64());
    diagnostics.add_measurement(&DIAG_ATLAS_PEAK_ACTIVE_JOBS, || {
        perf.atlas.peak_active_jobs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_WORKER_THREADS, || {
        perf.atlas.worker_threads.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_AVG_RASTER_MS, || {
        f64::from(perf.atlas.avg_raster_ms)
    });
    diagnostics.add_measurement(&DIAG_ATLAS_MAX_RASTER_MS, || {
        f64::from(perf.atlas.max_raster_ms)
    });
    diagnostics.add_measurement(&DIAG_ATLAS_BATCH_MAX_ACTIVE_JOBS, || {
        perf.atlas.batch_max_active_jobs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_TOTAL_GLYPHS, || {
        perf.atlas.total_glyphs.to_f64()
    });
}
