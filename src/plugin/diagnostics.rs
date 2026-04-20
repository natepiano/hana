//! Internal diagnostics registration and publishing for diegetic UI.

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
use super::constants::DIAG_TEXT_ATLAS_MS;
use super::constants::DIAG_TEXT_EXTRACT_MS;
use super::constants::DIAG_TEXT_EXTRACT_PANELS;
use super::constants::DIAG_TEXT_PENDING_GLYPHS;
use super::constants::DIAG_TEXT_QUEUED_GLYPHS;
use super::constants::DIAG_TEXT_SHAPE_MS;
use super::constants::DIAG_TEXT_SPAWN_MS;
use super::systems::DiegeticPerfStats;

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
    diagnostics.add_measurement(&DIAG_LAYOUT_COMPUTE_MS, || f64::from(perf.last_compute_ms));
    diagnostics.add_measurement(&DIAG_LAYOUT_COMPUTE_PANELS, || {
        perf.last_compute_panels.to_f64()
    });
    diagnostics.add_measurement(&DIAG_TEXT_EXTRACT_MS, || {
        f64::from(perf.last_text_extract_ms)
    });
    diagnostics.add_measurement(&DIAG_TEXT_EXTRACT_PANELS, || {
        perf.last_text_extract_panels.to_f64()
    });
    diagnostics.add_measurement(&DIAG_TEXT_SHAPE_MS, || f64::from(perf.last_text_shape_ms));
    diagnostics.add_measurement(&DIAG_TEXT_ATLAS_MS, || f64::from(perf.last_text_atlas_ms));
    diagnostics.add_measurement(&DIAG_TEXT_SPAWN_MS, || f64::from(perf.last_text_spawn_ms));
    diagnostics.add_measurement(&DIAG_TEXT_QUEUED_GLYPHS, || {
        perf.last_text_queued_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_TEXT_PENDING_GLYPHS, || {
        perf.last_text_pending_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_POLL_MS, || f64::from(perf.last_atlas_poll_ms));
    diagnostics.add_measurement(&DIAG_ATLAS_SYNC_MS, || f64::from(perf.last_atlas_sync_ms));
    diagnostics.add_measurement(&DIAG_ATLAS_COMPLETED_GLYPHS, || {
        perf.last_atlas_completed_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_INSERTED_GLYPHS, || {
        perf.last_atlas_inserted_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_INVISIBLE_GLYPHS, || {
        perf.last_atlas_invisible_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_PAGES_ADDED, || {
        perf.last_atlas_pages_added.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_DIRTY_PAGES, || {
        perf.last_atlas_dirty_pages.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_IN_FLIGHT_GLYPHS, || {
        perf.last_atlas_in_flight_glyphs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_ACTIVE_JOBS, || {
        perf.last_atlas_active_jobs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_PEAK_ACTIVE_JOBS, || {
        perf.last_atlas_peak_active_jobs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_WORKER_THREADS, || {
        perf.last_atlas_worker_threads.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_AVG_RASTER_MS, || {
        f64::from(perf.last_atlas_avg_raster_ms)
    });
    diagnostics.add_measurement(&DIAG_ATLAS_MAX_RASTER_MS, || {
        f64::from(perf.last_atlas_max_raster_ms)
    });
    diagnostics.add_measurement(&DIAG_ATLAS_BATCH_MAX_ACTIVE_JOBS, || {
        perf.last_atlas_batch_max_active_jobs.to_f64()
    });
    diagnostics.add_measurement(&DIAG_ATLAS_TOTAL_GLYPHS, || {
        perf.last_atlas_total_glyphs.to_f64()
    });
}
