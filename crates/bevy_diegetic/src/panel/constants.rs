//! Panel-domain constants: builder defaults and diagnostic paths.

use bevy::diagnostic::DiagnosticPath;

// Builder defaults
/// Default camera render order for screen-space overlay panels.
pub(super) const DEFAULT_SCREEN_SPACE_CAMERA_ORDER: isize = 1;
/// Default render layer for screen-space overlay panels.
pub(super) const DEFAULT_SCREEN_SPACE_RENDER_LAYER: usize = 31;
/// Minimum implicit world height assigned to fixed screen panels so that
/// 1 layout pixel maps to at least 1 world unit under the ortho camera.
pub(super) const MIN_PANEL_WORLD_HEIGHT: f32 = 1.0;
/// Hysteresis tolerance for `Fit`-axis world panel resize: skip writing back
/// a clamped width or height if it differs from the current value by less
/// than this.
pub(super) const PANEL_RESIZE_EPSILON: f32 = 0.001;

// Diagnostic paths
pub(super) const DIAG_ATLAS_ACTIVE_JOBS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/active_jobs");
pub(super) const DIAG_ATLAS_AVG_RASTER_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/avg_raster_ms");
pub(super) const DIAG_ATLAS_BATCH_MAX_ACTIVE_JOBS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/batch_max_active_jobs");
pub(super) const DIAG_ATLAS_COMPLETED_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/completed_glyphs");
pub(super) const DIAG_ATLAS_DIRTY_PAGES: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/dirty_pages");
pub(super) const DIAG_ATLAS_IN_FLIGHT_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/in_flight_glyphs");
pub(super) const DIAG_ATLAS_INSERTED_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/inserted_glyphs");
pub(super) const DIAG_ATLAS_INVISIBLE_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/invisible_glyphs");
pub(super) const DIAG_ATLAS_MAX_RASTER_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/max_raster_ms");
pub(super) const DIAG_ATLAS_PAGES_ADDED: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/pages_added");
pub(super) const DIAG_ATLAS_PEAK_ACTIVE_JOBS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/peak_active_jobs");
pub(super) const DIAG_ATLAS_POLL_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/poll_ms");
pub(super) const DIAG_ATLAS_SYNC_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/sync_ms");
pub(super) const DIAG_ATLAS_TOTAL_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/total_glyphs");
pub(super) const DIAG_ATLAS_WORKER_THREADS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/worker_threads");
pub(super) const DIAG_LAYOUT_COMPUTE_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/layout/compute_ms");
pub(super) const DIAG_LAYOUT_COMPUTE_PANELS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/layout/compute_panels");
pub(super) const DIAG_PANEL_TEXT_ATLAS_LOOKUP_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/panel_text/atlas_lookup_ms");
pub(super) const DIAG_PANEL_TEXT_MESH_BUILD_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/panel_text/mesh_build_ms");
pub(super) const DIAG_PANEL_TEXT_PARLEY_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/panel_text/parley_ms");
pub(super) const DIAG_PANEL_TEXT_PENDING_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/panel_text/pending_glyphs");
pub(super) const DIAG_PANEL_TEXT_QUEUED_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/panel_text/queued_glyphs");
pub(super) const DIAG_PANEL_TEXT_SHAPE_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/panel_text/shape_ms");
pub(super) const DIAG_PANEL_TEXT_SHAPED_PANELS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/panel_text/shaped_panels");
pub(super) const DIAG_PANEL_TEXT_TOTAL_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/panel_text/total_ms");
