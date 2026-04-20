//! Panel performance statistics and diagnostics publishing.

use bevy::diagnostic::Diagnostic;
use bevy::diagnostic::DiagnosticPath;
use bevy::diagnostic::Diagnostics;
use bevy::diagnostic::RegisterDiagnostic;
use bevy::prelude::*;
use bevy_kana::ToF64;

const DIAG_ATLAS_ACTIVE_JOBS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/active_jobs");
const DIAG_ATLAS_AVG_RASTER_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/avg_raster_ms");
const DIAG_ATLAS_BATCH_MAX_ACTIVE_JOBS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/batch_max_active_jobs");
const DIAG_ATLAS_COMPLETED_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/completed_glyphs");
const DIAG_ATLAS_DIRTY_PAGES: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/dirty_pages");
const DIAG_ATLAS_IN_FLIGHT_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/in_flight_glyphs");
const DIAG_ATLAS_INSERTED_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/inserted_glyphs");
const DIAG_ATLAS_INVISIBLE_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/invisible_glyphs");
const DIAG_ATLAS_MAX_RASTER_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/max_raster_ms");
const DIAG_ATLAS_PAGES_ADDED: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/pages_added");
const DIAG_ATLAS_PEAK_ACTIVE_JOBS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/peak_active_jobs");
const DIAG_ATLAS_POLL_MS: DiagnosticPath = DiagnosticPath::const_new("bevy_diegetic/atlas/poll_ms");
const DIAG_ATLAS_SYNC_MS: DiagnosticPath = DiagnosticPath::const_new("bevy_diegetic/atlas/sync_ms");
const DIAG_ATLAS_TOTAL_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/total_glyphs");
const DIAG_ATLAS_WORKER_THREADS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/worker_threads");
const DIAG_LAYOUT_COMPUTE_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/layout/compute_ms");
const DIAG_LAYOUT_COMPUTE_PANELS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/layout/compute_panels");
const DIAG_TEXT_ATLAS_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/atlas_lookup_ms");
const DIAG_TEXT_EXTRACT_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/extract_ms");
const DIAG_TEXT_EXTRACT_PANELS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/extract_panels");
const DIAG_TEXT_PENDING_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/pending_glyphs");
const DIAG_TEXT_QUEUED_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/queued_glyphs");
const DIAG_TEXT_SHAPE_MS: DiagnosticPath = DiagnosticPath::const_new("bevy_diegetic/text/shape_ms");
const DIAG_TEXT_SPAWN_MS: DiagnosticPath = DiagnosticPath::const_new("bevy_diegetic/text/spawn_ms");

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
    /// Duration of the most recent `compute_panel_layouts` run, in milliseconds.
    pub compute_ms:                  f32,
    /// Number of panels processed by the most recent layout run.
    pub compute_panels:              usize,
    /// Duration of the most recent text extraction run, in milliseconds.
    pub text_extract_ms:             f32,
    /// Number of panels processed by the most recent text extraction run.
    pub text_extract_panels:         usize,
    /// Time spent shaping text during the most recent panel text extraction.
    pub text_shape_ms:               f32,
    /// Time spent in atlas lookups/queueing during the most recent panel text extraction.
    pub text_atlas_ms:               f32,
    /// Time spent spawning mesh/material batches during the most recent panel text extraction.
    pub text_spawn_ms:               f32,
    /// Number of glyphs newly queued for rasterization during the most recent panel text
    /// extraction.
    pub text_queued_glyphs:          usize,
    /// Number of glyphs still pending rasterization during the most recent panel text extraction.
    pub text_pending_glyphs:         usize,
    /// Time spent draining async atlas results in the most recent atlas poll.
    pub atlas_poll_ms:               f32,
    /// Time spent syncing dirty atlas pages to GPU images in the most recent atlas poll.
    pub atlas_sync_ms:               f32,
    /// Number of completed async glyph jobs drained by the most recent atlas poll.
    pub atlas_completed_glyphs:      usize,
    /// Number of visible glyphs inserted into atlas pages by the most recent atlas poll.
    pub atlas_inserted_glyphs:       usize,
    /// Number of invisible glyph entries cached by the most recent atlas poll.
    pub atlas_invisible_glyphs:      usize,
    /// Number of atlas pages added by the most recent atlas poll.
    pub atlas_pages_added:           usize,
    /// Number of dirty atlas pages observed before the most recent GPU sync.
    pub atlas_dirty_pages:           usize,
    /// Number of glyph raster jobs still in flight after the most recent atlas poll.
    pub atlas_in_flight_glyphs:      usize,
    /// Number of glyph raster jobs actively executing at the end of the most recent atlas poll.
    pub atlas_active_jobs:           usize,
    /// Peak concurrently executing glyph raster jobs observed so far.
    pub atlas_peak_active_jobs:      usize,
    /// Number of distinct worker threads that completed jobs in the most recent atlas poll.
    pub atlas_worker_threads:        usize,
    /// Average worker-side glyph raster duration for the most recent drained batch.
    pub atlas_avg_raster_ms:         f32,
    /// Maximum worker-side glyph raster duration for the most recent drained batch.
    pub atlas_max_raster_ms:         f32,
    /// Highest active-job count reported by any job in the most recent drained batch.
    pub atlas_batch_max_active_jobs: usize,
    /// Total number of glyphs currently cached in the atlas.
    pub atlas_total_glyphs:          usize,
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
            Diagnostic::new(DIAG_TEXT_EXTRACT_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_TEXT_EXTRACT_PANELS),
            Diagnostic::new(DIAG_TEXT_SHAPE_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_TEXT_ATLAS_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_TEXT_SPAWN_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_TEXT_QUEUED_GLYPHS),
            Diagnostic::new(DIAG_TEXT_PENDING_GLYPHS),
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
    diagnostics.add_measurement(&DIAG_TEXT_EXTRACT_MS, || f64::from(perf.text_extract_ms));
    diagnostics.add_measurement(&DIAG_TEXT_EXTRACT_PANELS, || {
        perf.text_extract_panels.to_f64()
    });
    diagnostics.add_measurement(&DIAG_TEXT_SHAPE_MS, || f64::from(perf.text_shape_ms));
    diagnostics.add_measurement(&DIAG_TEXT_ATLAS_MS, || f64::from(perf.text_atlas_ms));
    diagnostics.add_measurement(&DIAG_TEXT_SPAWN_MS, || f64::from(perf.text_spawn_ms));
    diagnostics.add_measurement(&DIAG_TEXT_QUEUED_GLYPHS, || {
        perf.text_queued_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_TEXT_PENDING_GLYPHS, || {
        perf.text_pending_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_POLL_MS, || f64::from(perf.atlas_poll_ms));
    diagnostics.add_measurement(&DIAG_ATLAS_SYNC_MS, || f64::from(perf.atlas_sync_ms));
    diagnostics.add_measurement(&DIAG_ATLAS_COMPLETED_GLYPHS, || {
        perf.atlas_completed_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_INSERTED_GLYPHS, || {
        perf.atlas_inserted_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_INVISIBLE_GLYPHS, || {
        perf.atlas_invisible_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_PAGES_ADDED, || perf.atlas_pages_added.to_f64());
    diagnostics.add_measurement(&DIAG_ATLAS_DIRTY_PAGES, || perf.atlas_dirty_pages.to_f64());
    diagnostics.add_measurement(&DIAG_ATLAS_IN_FLIGHT_GLYPHS, || {
        perf.atlas_in_flight_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_ACTIVE_JOBS, || perf.atlas_active_jobs.to_f64());
    diagnostics.add_measurement(&DIAG_ATLAS_PEAK_ACTIVE_JOBS, || {
        perf.atlas_peak_active_jobs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_WORKER_THREADS, || {
        perf.atlas_worker_threads.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_AVG_RASTER_MS, || {
        f64::from(perf.atlas_avg_raster_ms)
    });
    diagnostics.add_measurement(&DIAG_ATLAS_MAX_RASTER_MS, || {
        f64::from(perf.atlas_max_raster_ms)
    });
    diagnostics.add_measurement(&DIAG_ATLAS_BATCH_MAX_ACTIVE_JOBS, || {
        perf.atlas_batch_max_active_jobs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_TOTAL_GLYPHS, || {
        perf.atlas_total_glyphs.to_f64()
    });
}
