//! Panel performance statistics and diagnostics publishing.

use bevy::diagnostic::Diagnostic;
use bevy::diagnostic::Diagnostics;
use bevy::diagnostic::RegisterDiagnostic;
use bevy::prelude::*;
use bevy_kana::ToF64;

use super::constants::DIAG_LAYOUT_COMPUTE_MS;
use super::constants::DIAG_LAYOUT_COMPUTE_PANELS;
use super::constants::DIAG_MATERIAL_TABLE_CAPACITY;
use super::constants::DIAG_MATERIAL_TABLE_ROWS;
use super::constants::DIAG_MATERIAL_TABLE_UPLOAD_BYTES;
use super::constants::DIAG_PANEL_RECONCILE_MS;
use super::constants::DIAG_PANEL_SDF_BATCHES;
use super::constants::DIAG_PANEL_SDF_RECORDS;
use super::constants::DIAG_PANEL_SDF_UPLOADS;
use super::constants::DIAG_PANEL_SHAPE_BATCHES;
use super::constants::DIAG_PANEL_SHAPE_RECORDS;
use super::constants::DIAG_PANEL_SHAPE_UPLOADS;
use super::constants::DIAG_PANEL_TEXT_MESH_BUILD_MS;
use super::constants::DIAG_PANEL_TEXT_PARLEY_MS;
use super::constants::DIAG_PANEL_TEXT_SHAPE_MS;
use super::constants::DIAG_PANEL_TEXT_SHAPED_PANELS;
use super::constants::DIAG_PANEL_TEXT_TOTAL_MS;
use super::constants::DIAG_TEXT_BATCH_GLYPHS;
use super::constants::DIAG_TEXT_BATCH_INSTANCE_UPLOADS;
use super::constants::DIAG_TEXT_BATCH_RUN_TABLE_UPLOADS;
use super::constants::DIAG_TEXT_BATCH_RUNS;
use super::constants::DIAG_TEXT_BATCHES;

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
    /// Between stages 1 and 2 — `reconcile_panel_text_children` plus
    /// `reconcile_panel_image_children` wall time in milliseconds: re-deriving
    /// text / image child entities from each changed panel's render commands.
    pub reconcile_ms:   f32,
    /// Stages 2 & 3 — panel-text shaping + record-build timings and counts.
    pub panel_text:     PanelTextPerfStats,
    /// Glyph-batch counters, written by `commit_batch_buffers`.
    pub batch:          BatchPerfStats,
    /// Panel-line analytic path batch counters, written by
    /// `commit_panel_shape_batch_buffers`.
    pub line_batch:     PanelShapeBatchPerfStats,
    /// SDF panel surface batch counters, written by `commit_sdf_batch_buffers`.
    pub panel_geometry: PanelGeometryPerfStats,
    /// Frame material-table counters, written after the shared table buffer is current.
    pub material_table: MaterialTablePerfStats,
}

/// Per-frame glyph-batch counters, written by `commit_batch_buffers`.
///
/// The two upload counters are split to match the store's per-buffer dirty
/// flags: a transform-only frame uploads only run tables, a same-count text
/// edit only instance buffers, an unchanged frame nothing.
#[derive(Clone, Debug, Default, Reflect)]
pub struct BatchPerfStats {
    /// Live batch count (one render entity + one draw per pass each).
    pub batches:           usize,
    /// Text runs routed across all batches.
    pub runs:              usize,
    /// Glyph instance records across all batches.
    pub glyph_records:     usize,
    /// Glyph-instance buffer uploads this frame.
    pub instance_uploads:  usize,
    /// Run-table buffer uploads this frame.
    pub run_table_uploads: usize,
}

/// Per-frame panel-line analytic path batch counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
pub struct PanelShapeBatchPerfStats {
    /// Live vector-mark batch count.
    pub batches: usize,
    /// Analytic path instance records routed across all batches.
    pub records: usize,
    /// Analytic path instance/run buffer uploads this frame.
    pub uploads: usize,
}

/// Per-frame panel geometry counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
pub struct PanelGeometryPerfStats {
    /// Live SDF batch render entities for panel backgrounds, borders, and
    /// divider rectangles.
    pub sdf_batches: usize,
    /// Live SDF records routed across all SDF batches.
    pub sdf_records: usize,
    /// SDF record-buffer uploads this frame.
    pub sdf_uploads: usize,
}

/// Per-frame shared material-table counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
pub struct MaterialTablePerfStats {
    /// Current-frame `MaterialSlotValues` rows appended by render producers.
    pub rows:         usize,
    /// Bytes represented by the current-frame live material rows.
    pub upload_bytes: usize,
    /// Row capacity of the shared material-table storage buffer.
    pub capacity:     usize,
}

/// Panel-text per-frame timings. Covers stages 2 and 3 of the panel pipeline:
///
/// 1. `compute_panel_layouts` → [`DiegeticPerfStats::compute_ms`], then the child reconcile →
///    [`DiegeticPerfStats::reconcile_ms`]
/// 2. `shape_panel_text_children` → [`Self::shape_ms`] (strings → positioned glyphs)
/// 3. `update_panel_text_batches` → [`Self::mesh_build_ms`] (glyphs → batch records)
///
/// Render-pass time is not measured here — Bevy's own diagnostics report it
/// (`FrameTimeDiagnosticsPlugin`, `RenderDiagnosticsPlugin`) and it is outside
/// this crate's control.
///
/// All text is covered: standalone `DiegeticText` labels are one-element
/// panels, so every `TextContent` run flows through this pipeline.
#[derive(Clone, Debug, Default, Reflect)]
pub struct PanelTextPerfStats {
    /// End-to-end panel-text wall time this frame, in milliseconds.
    /// Equals [`Self::shape_ms`] + [`Self::mesh_build_ms`].
    ///
    /// Written twice per frame: first by `shape_panel_text_children` using
    /// the *previous* frame's `mesh_build_ms`, then overwritten by
    /// `update_panel_text_batches` using the current frame's values. The
    /// final value is only correct because the record build is scheduled
    /// `.after(shape_panel_text_children)`; reordering those systems would
    /// leave `total_ms` stale by one frame.
    pub total_ms:      f32,
    /// Stage 2 — wall time of `shape_panel_text_children` this frame.
    /// Covers turning strings into positioned glyphs for every panel-text
    /// entity that changed or is waiting on glyph loading.
    pub shape_ms:      f32,
    /// Inside [`Self::shape_ms`] — time spent in parley text shaping,
    /// summed across entities. If this dominates, the cost is content-side
    /// (many strings, complex scripts, heavy font features).
    pub parley_ms:     f32,
    /// Stage 3 — wall time of `update_panel_text_batches` this frame.
    /// Covers building glyph records, routing runs through the batch store,
    /// and reconciling batch entities and GPU assets.
    pub mesh_build_ms: f32,
    /// Number of panels whose text shaping ran this frame.
    pub shaped_panels: usize,
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
            Diagnostic::new(DIAG_PANEL_RECONCILE_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_MATERIAL_TABLE_ROWS),
            Diagnostic::new(DIAG_MATERIAL_TABLE_UPLOAD_BYTES).with_suffix(" bytes"),
            Diagnostic::new(DIAG_MATERIAL_TABLE_CAPACITY),
            Diagnostic::new(DIAG_PANEL_SDF_BATCHES),
            Diagnostic::new(DIAG_PANEL_SDF_RECORDS),
            Diagnostic::new(DIAG_PANEL_SDF_UPLOADS),
            Diagnostic::new(DIAG_PANEL_SHAPE_BATCHES),
            Diagnostic::new(DIAG_PANEL_SHAPE_RECORDS),
            Diagnostic::new(DIAG_PANEL_SHAPE_UPLOADS),
            Diagnostic::new(DIAG_PANEL_TEXT_TOTAL_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_PANEL_TEXT_SHAPE_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_PANEL_TEXT_PARLEY_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_PANEL_TEXT_MESH_BUILD_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_PANEL_TEXT_SHAPED_PANELS),
            Diagnostic::new(DIAG_TEXT_BATCHES),
            Diagnostic::new(DIAG_TEXT_BATCH_RUNS),
            Diagnostic::new(DIAG_TEXT_BATCH_GLYPHS),
            Diagnostic::new(DIAG_TEXT_BATCH_INSTANCE_UPLOADS),
            Diagnostic::new(DIAG_TEXT_BATCH_RUN_TABLE_UPLOADS),
        ] {
            app.register_diagnostic(diagnostic);
        }

        app.add_systems(Last, publish_perf_diagnostics);
    }
}

fn publish_perf_diagnostics(perf: Res<DiegeticPerfStats>, mut diagnostics: Diagnostics) {
    diagnostics.add_measurement(&DIAG_LAYOUT_COMPUTE_MS, || f64::from(perf.compute_ms));
    diagnostics.add_measurement(&DIAG_LAYOUT_COMPUTE_PANELS, || perf.compute_panels.to_f64());
    diagnostics.add_measurement(&DIAG_PANEL_RECONCILE_MS, || f64::from(perf.reconcile_ms));
    diagnostics.add_measurement(&DIAG_MATERIAL_TABLE_ROWS, || {
        perf.material_table.rows.to_f64()
    });
    diagnostics.add_measurement(&DIAG_MATERIAL_TABLE_UPLOAD_BYTES, || {
        perf.material_table.upload_bytes.to_f64()
    });
    diagnostics.add_measurement(&DIAG_MATERIAL_TABLE_CAPACITY, || {
        perf.material_table.capacity.to_f64()
    });
    diagnostics.add_measurement(&DIAG_PANEL_SDF_BATCHES, || {
        perf.panel_geometry.sdf_batches.to_f64()
    });
    diagnostics.add_measurement(&DIAG_PANEL_SDF_RECORDS, || {
        perf.panel_geometry.sdf_records.to_f64()
    });
    diagnostics.add_measurement(&DIAG_PANEL_SDF_UPLOADS, || {
        perf.panel_geometry.sdf_uploads.to_f64()
    });
    diagnostics.add_measurement(&DIAG_PANEL_SHAPE_BATCHES, || {
        perf.line_batch.batches.to_f64()
    });
    diagnostics.add_measurement(&DIAG_PANEL_SHAPE_RECORDS, || {
        perf.line_batch.records.to_f64()
    });
    diagnostics.add_measurement(&DIAG_PANEL_SHAPE_UPLOADS, || {
        perf.line_batch.uploads.to_f64()
    });
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_TOTAL_MS, || {
        f64::from(perf.panel_text.total_ms)
    });
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_SHAPE_MS, || {
        f64::from(perf.panel_text.shape_ms)
    });
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_PARLEY_MS, || {
        f64::from(perf.panel_text.parley_ms)
    });
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_MESH_BUILD_MS, || {
        f64::from(perf.panel_text.mesh_build_ms)
    });
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_SHAPED_PANELS, || {
        perf.panel_text.shaped_panels.to_f64()
    });
    diagnostics.add_measurement(&DIAG_TEXT_BATCHES, || perf.batch.batches.to_f64());
    diagnostics.add_measurement(&DIAG_TEXT_BATCH_RUNS, || perf.batch.runs.to_f64());
    diagnostics.add_measurement(&DIAG_TEXT_BATCH_GLYPHS, || {
        perf.batch.glyph_records.to_f64()
    });
    diagnostics.add_measurement(&DIAG_TEXT_BATCH_INSTANCE_UPLOADS, || {
        perf.batch.instance_uploads.to_f64()
    });
    diagnostics.add_measurement(&DIAG_TEXT_BATCH_RUN_TABLE_UPLOADS, || {
        perf.batch.run_table_uploads.to_f64()
    });
}
