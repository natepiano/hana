//! Internal diagnostics registration and publishing for diegetic UI.

use bevy::diagnostic::Diagnostic;
use bevy::diagnostic::DiagnosticPath;
use bevy::diagnostic::Diagnostics;
use bevy::diagnostic::RegisterDiagnostic;
use bevy::prelude::*;

use super::systems::DiegeticPerfStats;

pub(super) const DIAG_LAYOUT_COMPUTE_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/layout/compute_ms");
pub(super) const DIAG_LAYOUT_COMPUTE_PANELS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/layout/compute_panels");
pub(super) const DIAG_TEXT_EXTRACT_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/extract_ms");
pub(super) const DIAG_TEXT_EXTRACT_PANELS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/extract_panels");
pub(super) const DIAG_TEXT_SHAPE_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/shape_ms");
pub(super) const DIAG_TEXT_ATLAS_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/atlas_lookup_ms");
pub(super) const DIAG_TEXT_SPAWN_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/spawn_ms");
pub(super) const DIAG_TEXT_QUEUED_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/queued_glyphs");
pub(super) const DIAG_TEXT_PENDING_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/pending_glyphs");
pub(super) const DIAG_ATLAS_POLL_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/poll_ms");
pub(super) const DIAG_ATLAS_SYNC_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/sync_ms");
pub(super) const DIAG_ATLAS_COMPLETED_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/completed_glyphs");
pub(super) const DIAG_ATLAS_INSERTED_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/inserted_glyphs");
pub(super) const DIAG_ATLAS_INVISIBLE_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/invisible_glyphs");
pub(super) const DIAG_ATLAS_PAGES_ADDED: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/pages_added");
pub(super) const DIAG_ATLAS_DIRTY_PAGES: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/dirty_pages");
pub(super) const DIAG_ATLAS_IN_FLIGHT_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/in_flight_glyphs");
pub(super) const DIAG_ATLAS_ACTIVE_JOBS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/active_jobs");
pub(super) const DIAG_ATLAS_PEAK_ACTIVE_JOBS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/peak_active_jobs");
pub(super) const DIAG_ATLAS_WORKER_THREADS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/worker_threads");
pub(super) const DIAG_ATLAS_AVG_RASTER_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/avg_raster_ms");
pub(super) const DIAG_ATLAS_MAX_RASTER_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/max_raster_ms");
pub(super) const DIAG_ATLAS_BATCH_MAX_ACTIVE_JOBS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/batch_max_active_jobs");
pub(super) const DIAG_ATLAS_TOTAL_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/total_glyphs");

#[derive(Resource, Default)]
struct DiegeticDiagnosticsRegistered;

pub(super) fn install(app: &mut App) {
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

fn publish_perf_diagnostics(perf: Res<DiegeticPerfStats>, mut diagnostics: Diagnostics) {
    diagnostics.add_measurement(&DIAG_LAYOUT_COMPUTE_MS, || f64::from(perf.last_compute_ms));
    diagnostics.add_measurement(&DIAG_LAYOUT_COMPUTE_PANELS, || {
        perf.last_compute_panels as f64
    });
    diagnostics.add_measurement(&DIAG_TEXT_EXTRACT_MS, || {
        f64::from(perf.last_text_extract_ms)
    });
    diagnostics.add_measurement(&DIAG_TEXT_EXTRACT_PANELS, || {
        perf.last_text_extract_panels as f64
    });
    diagnostics.add_measurement(&DIAG_TEXT_SHAPE_MS, || f64::from(perf.last_text_shape_ms));
    diagnostics.add_measurement(&DIAG_TEXT_ATLAS_MS, || f64::from(perf.last_text_atlas_ms));
    diagnostics.add_measurement(&DIAG_TEXT_SPAWN_MS, || f64::from(perf.last_text_spawn_ms));
    diagnostics.add_measurement(&DIAG_TEXT_QUEUED_GLYPHS, || {
        perf.last_text_queued_glyphs as f64
    });
    diagnostics.add_measurement(&DIAG_TEXT_PENDING_GLYPHS, || {
        perf.last_text_pending_glyphs as f64
    });
    diagnostics.add_measurement(&DIAG_ATLAS_POLL_MS, || f64::from(perf.last_atlas_poll_ms));
    diagnostics.add_measurement(&DIAG_ATLAS_SYNC_MS, || f64::from(perf.last_atlas_sync_ms));
    diagnostics.add_measurement(&DIAG_ATLAS_COMPLETED_GLYPHS, || {
        perf.last_atlas_completed_glyphs as f64
    });
    diagnostics.add_measurement(&DIAG_ATLAS_INSERTED_GLYPHS, || {
        perf.last_atlas_inserted_glyphs as f64
    });
    diagnostics.add_measurement(&DIAG_ATLAS_INVISIBLE_GLYPHS, || {
        perf.last_atlas_invisible_glyphs as f64
    });
    diagnostics.add_measurement(&DIAG_ATLAS_PAGES_ADDED, || {
        perf.last_atlas_pages_added as f64
    });
    diagnostics.add_measurement(&DIAG_ATLAS_DIRTY_PAGES, || {
        perf.last_atlas_dirty_pages as f64
    });
    diagnostics.add_measurement(&DIAG_ATLAS_IN_FLIGHT_GLYPHS, || {
        perf.last_atlas_in_flight_glyphs as f64
    });
    diagnostics.add_measurement(&DIAG_ATLAS_ACTIVE_JOBS, || {
        perf.last_atlas_active_jobs as f64
    });
    diagnostics.add_measurement(&DIAG_ATLAS_PEAK_ACTIVE_JOBS, || {
        perf.last_atlas_peak_active_jobs as f64
    });
    diagnostics.add_measurement(&DIAG_ATLAS_WORKER_THREADS, || {
        perf.last_atlas_worker_threads as f64
    });
    diagnostics.add_measurement(&DIAG_ATLAS_AVG_RASTER_MS, || {
        f64::from(perf.last_atlas_avg_raster_ms)
    });
    diagnostics.add_measurement(&DIAG_ATLAS_MAX_RASTER_MS, || {
        f64::from(perf.last_atlas_max_raster_ms)
    });
    diagnostics.add_measurement(&DIAG_ATLAS_BATCH_MAX_ACTIVE_JOBS, || {
        perf.last_atlas_batch_max_active_jobs as f64
    });
    diagnostics.add_measurement(&DIAG_ATLAS_TOTAL_GLYPHS, || {
        perf.last_atlas_total_glyphs as f64
    });
}
