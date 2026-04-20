//! Constants shared across the diegetic plugin modules.

use bevy::diagnostic::DiagnosticPath;

// Atlas config
/// Average glyph coverage ratio — most glyphs use roughly this fraction of
/// the canonical size.
pub(super) const AVERAGE_GLYPH_COVERAGE: f32 = 0.75;
/// Default auto-selected glyph raster worker count on sufficiently parallel machines.
pub(super) const DEFAULT_AUTO_GLYPH_WORKER_THREADS: usize = 6;
/// Default glyphs per atlas page.
pub(super) const DEFAULT_GLYPHS_PER_PAGE: u16 = 100;
/// Glyph padding used during MSDF rasterization.
pub(super) const GLYPH_PADDING: u32 = 2;
/// Maximum canonical rasterization size in pixels.
pub(super) const MAX_CUSTOM_RASTER_SIZE: u32 = 256;
/// Maximum glyphs per atlas page.
pub(super) const MAX_GLYPHS_PER_PAGE: u16 = 2000;
/// Minimum canonical rasterization size in pixels.
pub(super) const MIN_CUSTOM_RASTER_SIZE: u32 = 8;
/// Minimum glyphs per atlas page.
pub(super) const MIN_GLYPHS_PER_PAGE: u16 = 10;
/// SDF distance range used during MSDF rasterization.
pub(super) const SDF_RANGE: u32 = 4;
/// Estimated packing efficiency for a shelf-based atlas allocator.
pub(super) const SHELF_PACKING_EFFICIENCY: f32 = 0.80;

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
pub(super) const DIAG_TEXT_ATLAS_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/atlas_lookup_ms");
pub(super) const DIAG_TEXT_EXTRACT_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/extract_ms");
pub(super) const DIAG_TEXT_EXTRACT_PANELS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/extract_panels");
pub(super) const DIAG_TEXT_PENDING_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/pending_glyphs");
pub(super) const DIAG_TEXT_QUEUED_GLYPHS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/queued_glyphs");
pub(super) const DIAG_TEXT_SHAPE_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/shape_ms");
pub(super) const DIAG_TEXT_SPAWN_MS: DiagnosticPath =
    DiagnosticPath::const_new("bevy_diegetic/text/spawn_ms");

// Screen space
/// Default camera order for screen-space overlay cameras.
pub(super) const DEFAULT_SCREEN_SPACE_CAMERA_ORDER: isize = 1;
/// Default render layer for screen-space panels.
pub(super) const DEFAULT_SCREEN_SPACE_RENDER_LAYER: usize = 31;
