//! Panel-domain constants: builder defaults and diagnostic paths.

use bevy::color::Color;
use bevy::diagnostic::DiagnosticPath;

// builder defaults
/// Default camera render order for screen-space overlay panels. Chosen high
/// enough that user-spawned 3D viewport cameras (e.g. a minimap at order 1)
/// don't collide with the screen-space camera on the primary window.
pub(super) const DEFAULT_SCREEN_SPACE_CAMERA_ORDER: isize = 100;
/// Default render layer for screen-space overlay panels.
pub(super) const DEFAULT_SCREEN_SPACE_RENDER_LAYER: usize = 31;
/// Minimum implicit world height assigned to fixed screen panels so that
/// 1 layout pixel maps to at least 1 world unit under the ortho camera.
pub(super) const MIN_PANEL_WORLD_HEIGHT: f32 = 1.0;
/// Hysteresis tolerance for `Fit`-axis world panel resize: skip writing back
/// a clamped width or height if it differs from the current value by less
/// than this.
pub(super) const PANEL_RESIZE_EPSILON: f32 = 0.001;

// gizmos
/// Color used for text-bound debug gizmos.
pub(super) const DEBUG_TEXT_GIZMO_COLOR: Color = Color::srgba(0.9, 0.9, 0.2, 0.2);
/// Line width for text-bound debug gizmos, in pixels.
pub(super) const DEBUG_TEXT_GIZMO_LINE_WIDTH: f32 = 1.0;
/// Pixels-per-meter fallback used when the camera projection cannot be sampled
/// (no camera available, or `Projection::Custom` provides no usable scale).
pub(super) const FALLBACK_PIXELS_PER_METER: f32 = 1000.0;
/// Per-corner segment count for rounded gizmo line joints — controls
/// smoothness of corner curvature on panel border outlines.
pub(super) const GIZMO_LINE_JOINT_SEGMENTS: u32 = 8;
/// Line width for layout gizmos, in pixels.
pub(super) const LAYOUT_GIZMO_LINE_WIDTH: f32 = 1.0;
/// Minimum on-screen border line width, in pixels.
pub(super) const MIN_BORDER_LINE_WIDTH_PIXELS: f32 = 1.0;

// diagnostic paths
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
pub(super) const DIAG_ATLAS_INSERTED_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/inserted_glyphs");
pub(super) const DIAG_ATLAS_IN_FLIGHT_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/atlas/in_flight_glyphs");
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
pub(super) const DIAG_PANEL_TEXT_SHAPED_PANELS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/panel_text/shaped_panels");
pub(super) const DIAG_PANEL_TEXT_SHAPE_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/panel_text/shape_ms");
pub(super) const DIAG_PANEL_TEXT_TOTAL_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/panel_text/total_ms");
