//! Panel performance statistics and diagnostics publishing.

use bevy::diagnostic::Diagnostic;
use bevy::diagnostic::Diagnostics;
use bevy::diagnostic::RegisterDiagnostic;
use bevy::prelude::*;
use bevy_kana::ToF64;

use super::constants::DIAG_LAYOUT_COMPUTE_MS;
use super::constants::DIAG_LAYOUT_COMPUTE_PANELS;
use super::constants::DIAG_PANEL_TEXT_MESH_BUILD_MS;
use super::constants::DIAG_PANEL_TEXT_PARLEY_MS;
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
    /// Stages 2 & 3 — panel-text shaping + mesh-build timings and counts.
    pub panel_text:     PanelTextPerfStats,
}

/// Panel-text per-frame timings. Covers stages 2 and 3 of the panel pipeline:
///
/// 1. `compute_panel_layouts` → [`DiegeticPerfStats::compute_ms`]
/// 2. `shape_panel_text_children` → [`Self::shape_ms`] (strings → positioned glyphs)
/// 3. `build_panel_text_meshes` → [`Self::mesh_build_ms`] (glyphs → meshes)
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
    /// `build_panel_text_meshes` using the current frame's values. The
    /// final value is only correct because mesh build is scheduled
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
    /// Stage 3 — wall time of `build_panel_text_meshes` this frame.
    /// Covers building glyph meshes and despawning stale meshes.
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
            Diagnostic::new(DIAG_PANEL_TEXT_TOTAL_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_PANEL_TEXT_SHAPE_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_PANEL_TEXT_PARLEY_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_PANEL_TEXT_MESH_BUILD_MS).with_suffix(" ms"),
            Diagnostic::new(DIAG_PANEL_TEXT_SHAPED_PANELS),
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
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_MESH_BUILD_MS, || {
        f64::from(perf.panel_text.mesh_build_ms)
    });
    diagnostics.add_measurement(&DIAG_PANEL_TEXT_SHAPED_PANELS, || {
        perf.panel_text.shaped_panels.to_f64()
    });
}
