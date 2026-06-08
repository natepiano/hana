//! Text-write stress test — 100 standalone `DiegeticText` labels, each retext
//! every frame through the `DiegeticTextMut` write path.
//!
//! This is the canonical subject for the write-path half of the perf gate: a
//! 10×10 grid of world-space labels, every one mutated per frame via
//! `DiegeticTextMut::for_each_mut`, so a tree-authoritative write and its
//! relayout fire on all 100 labels each frame — the worst-case
//! `O(n_changed)` load. The companion `diegetic_panel_stress` example profiles
//! the other axis (panel tree-build / `set_tree` churn).
//!
//! Controls:
//!   Space — pause / resume per-frame mutation (compare moving vs idle cost);
//!     the title-bar `Pause` segment highlights while paused
//!   A — cycle the text anti-alias mode (Off → Anisotropic → Supersample →
//!     Both); the title bar highlights the active mode
//!   T — toggle stable transparency (OIT) on the camera (compare the
//!     transparent-pass + `oit_resolve` cost on vs off); the title-bar `OIT`
//!     segment highlights while enabled
//!
//! A bottom-left screen overlay reports the frame as two additive blocks, one
//! per thread, each row with a 5-second peak column. Main thread: `ms` is the
//! sum of `layout`, `reconcile`, `shaping`, `mesh`, `other`, and `wait`.
//! Render thread (overlaps `ms` — pipelined rendering): `render` is the sum
//! of `assets`, `prep`, `gpu wait`, and `graph`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use bevy::camera::primitives::Aabb;
use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::diagnostic::Diagnostic;
use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::pbr::Shadow;
use bevy::prelude::*;
use bevy::render::Render;
use bevy::render::RenderApp;
use bevy::render::RenderSystems;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;
use bevy::render::render_phase::ViewBinnedRenderPhases;
use bevy::render::render_phase::ViewSortedRenderPhases;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::DiegeticPerfStats;
use bevy_diegetic::DiegeticText;
use bevy_diegetic::DiegeticTextMut;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::StableTransparency;
use bevy_diegetic::TextAntiAlias;
use bevy_diegetic::TextStyle;
use bevy_kana::ToF32;
use bevy_kana::ToU32;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::TitleBar;
use fairy_dust::TitleBarControl;
use fairy_dust::TitleBarSegment;
use fairy_dust::screen_panel_frame;
use fairy_dust::screen_panel_material;

// ── Grid layout (meters) ──────────────────────────────────────────────────────

/// Labels per side; `GRID_SIDE * GRID_SIDE` is the total label count.
const GRID_SIDE: usize = 10;
/// `GRID_SIDE` as `f32` for const layout math (kept as a literal so const
/// evaluation needs no `usize as f32` cast).
const GRID_SIDE_F32: f32 = 10.0;
const LABEL_COUNT: usize = GRID_SIDE * GRID_SIDE;
/// Horizontal / vertical spacing between label anchors (meters).
const CELL_SPACING: f32 = 0.55;
/// Label cap height (meters).
const LABEL_SIZE: f32 = 0.12;
/// Height of the grid's bottom row above the ground (meters).
const GRID_BASE_Y: f32 = 0.6;

const LABEL_COLOR: Color = Color::srgb(0.92, 0.92, 0.94);

/// World-space half-extents of the static camera-home region, centered on
/// [`GRID_FOCUS`]. Covers the label-anchor span (half the grid each way) plus a
/// few label heights so the glyphs sit inside the framed box; the home fit adds
/// its own screen-fraction margin on top.
const HOME_REGION_HALF_EXTENTS: Vec3 = Vec3::new(
    (GRID_SIDE_F32 - 1.0) * CELL_SPACING * 0.5 + LABEL_SIZE * 3.0,
    (GRID_SIDE_F32 - 1.0) * CELL_SPACING * 0.5 + LABEL_SIZE * 1.5,
    LABEL_SIZE,
);

// ── Overlay (screen-space) constants ──────────────────────────────────────────

const OVERLAY_FONT_SIZE: f32 = 13.0;
const STATUS_TEXT_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.9);
const STATUS_LABEL_COLOR: Color = Color::srgba(0.7, 0.78, 0.92, 0.85);
/// Wide enough to contain the longest label (`reconcile`) plus the sub-row
/// indent without wrapping (`gpu wait` wraps at 92).
const LABEL_COLUMN_WIDTH: f32 = 110.0;
/// Wide enough to contain the `5s max` header on one line.
const VALUE_COLUMN_WIDTH: f32 = 72.0;
const TABLE_COL_GAP: f32 = 8.0;
const TABLE_ROW_GAP: f32 = 2.0;
/// Left padding on component-row labels, marking what sums into the block
/// header above.
const SUB_ROW_INDENT: f32 = 12.0;
const FPS_UPDATE_INTERVAL: f32 = 1.0;
const PERF_PEAK_WINDOW_SECS: f32 = 5.0;
const MILLISECONDS_PER_SECOND: f32 = 1000.0;

/// Diagnostic table rows, in display order — two additive blocks, one per
/// thread, with indented rows summing to their block header.
///
/// Main thread: `ms` (frame wall time) = `layout` + `reconcile` + `shaping` +
/// `mesh` (the measured diegetic spans) + `other` (the rest of the main
/// schedules: cascade, transform propagation, every other system) + `wait`
/// (outside the main schedules: blocked handing off to the render thread,
/// plus the extract copy).
///
/// Render thread, overlaps `ms` (pipelined rendering — it renders frame N
/// while the main world runs N+1, so it bounds `ms` without adding to it):
/// `render` (whole `Render` schedule) = `assets` (the `PrepareAssets` stage —
/// re-uploading every mesh / image / buffer asset modified this frame) +
/// `prep` (extract-commands apply, prepare meshes and views, specialize,
/// queue, sort, bind groups, cleanup) + `gpu wait` (the `PrepareViews` stage
/// containing the swapchain acquire — where the render thread blocks when the
/// GPU is behind) + `graph` (render-graph execution: pass encoding, submit,
/// present).
///
/// The `bool` marks an indented component row (sums into the block header
/// above it); indentation is layout padding, not leading spaces, which text
/// shaping would trim.
const METRIC_ROWS: [(&str, bool); 13] = [
    ("fps", false),
    ("ms", false),
    ("layout", true),
    ("reconcile", true),
    ("shaping", true),
    ("mesh", true),
    ("other", true),
    ("wait", true),
    ("render", false),
    ("assets", true),
    ("prep", true),
    ("gpu wait", true),
    ("graph", true),
];
const METRIC_COUNT: usize = METRIC_ROWS.len();
const INITIAL_METRICS: [&str; METRIC_COUNT] = ["--"; METRIC_COUNT];

// ── Components / resources ────────────────────────────────────────────────────

/// Marks a stress label and carries its grid index, so `for_each_mut` can write
/// a distinct string per label.
#[derive(Component, Clone, Copy)]
struct StressLabel(usize);

/// Marker for the screen-space diagnostic overlay panel.
#[derive(Component)]
struct StatusPanel;

/// Marker for the upper-right glyph-batch stats panel (Step-2 proof
/// counters), separate from the waterfall so its wide rows don't stretch it.
#[derive(Component)]
struct BatchStatsPanel;

/// Monotonic frame counter driving the per-label text; wrapped to three digits so
/// label widths stay bounded.
#[derive(Resource, Default)]
struct FrameCounter(u64);

/// Whether the per-frame mutation is running. Toggled with Space; the
/// title-bar `Pause` segment highlights while paused.
#[derive(Resource)]
struct Mutating(bool);

impl Default for Mutating {
    fn default() -> Self { Self(true) }
}

/// Title-bar segment id for the pause indicator.
const PAUSE_CHIP: &str = "pause";

/// Whether `StableTransparency` (OIT) is enabled on the orbit camera. Toggled
/// with `T`; the title-bar `OIT` segment highlights while enabled. The default
/// matches `with_stable_transparency()` in `main`, which turns it on at spawn.
/// `apply_stable_transparency` reconciles the camera component to this state.
#[derive(Resource)]
struct StableTransparencyOn(bool);

impl Default for StableTransparencyOn {
    fn default() -> Self { Self(true) }
}

/// Title-bar segment id for the OIT indicator.
const OIT_CHIP: &str = "oit";

/// The in-shader [`TextAntiAlias`] modes in `A`-key cycle order: title-bar
/// segment id, visible label, and the mode itself. One source of truth for
/// the chips, the chip wiring, and the cycle step.
const AA_MODES: [(&str, &str, TextAntiAlias); 4] = [
    ("aa-off", "Off", TextAntiAlias::Off),
    ("aa-anisotropic", "Anisotropic", TextAntiAlias::Anisotropic),
    ("aa-supersample", "Supersample", TextAntiAlias::Supersample),
    ("aa-both", "Both", TextAntiAlias::Both),
];

/// Maps a boolean to the title-bar activation it represents.
const fn chip_activation(active: bool) -> ControlActivation {
    if active {
        ControlActivation::Active
    } else {
        ControlActivation::Inactive
    }
}

/// Advances [`TextAntiAlias`] one step through [`AA_MODES`], wrapping at the
/// end. The change propagates to every text material via the engine's
/// `sync_text_anti_alias` system, and to the title-bar chips via the
/// per-mode wiring in `main`.
fn cycle_text_anti_alias(mut anti_alias: ResMut<TextAntiAlias>) {
    let current = AA_MODES
        .iter()
        .position(|(_, _, mode)| *mode == *anti_alias)
        .unwrap_or(0);
    *anti_alias = AA_MODES[(current + 1) % AA_MODES.len()].2;
}

#[derive(Clone, Copy)]
struct PerfSnapshot {
    timestamp:    f32,
    fps:          f32,
    frame_ms:     f32,
    layout_ms:    f32,
    reconcile_ms: f32,
    shaping_ms:   f32,
    mesh_ms:      f32,
    other_ms:     f32,
    wait_ms:      f32,
    render_ms:    f32,
    assets_ms:    f32,
    prep_ms:      f32,
    gpu_wait_ms:  f32,
    graph_ms:     f32,
}

impl PerfSnapshot {
    const ZERO: Self = Self {
        timestamp:    0.0,
        fps:          0.0,
        frame_ms:     0.0,
        layout_ms:    0.0,
        reconcile_ms: 0.0,
        shaping_ms:   0.0,
        mesh_ms:      0.0,
        other_ms:     0.0,
        wait_ms:      0.0,
        render_ms:    0.0,
        assets_ms:    0.0,
        prep_ms:      0.0,
        gpu_wait_ms:  0.0,
        graph_ms:     0.0,
    };
}

/// Last displayed overlay string, so the overlay tree is rebuilt only when a
/// shown value changes.
#[derive(Resource, Default)]
struct LastDisplayedStatus {
    text: String,
}

/// Last displayed batch-stats string, so the batch panel rebuilds only when a
/// shown value changes.
#[derive(Resource, Default)]
struct LastDisplayedBatchStats {
    text: String,
}

// ── Main-thread span ──────────────────────────────────────────────────────────

/// Start instant of the current main-world frame, recorded in `First`.
#[derive(Resource)]
struct MainSpanStart(Instant);

/// Wall time of the previous main-world schedule run (`First` → `Last`) in
/// milliseconds. The `other` row is this minus the four measured diegetic
/// spans; the `wait` row is the frame time minus this.
#[derive(Resource, Default)]
struct MainThreadMs(f32);

fn mark_main_span_start(mut start: ResMut<MainSpanStart>) { start.0 = Instant::now(); }

fn publish_main_span(start: Res<MainSpanStart>, mut main_thread: ResMut<MainThreadMs>) {
    main_thread.0 = start.0.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
}

// ── Render-thread spans ───────────────────────────────────────────────────────

/// Latest render-thread segment values in milliseconds, stored as `f32` bits.
/// Written at the end of each `Render` schedule run on the render thread and
/// read by the overlay on the main thread. The segments sum to `render`.
#[derive(Default)]
struct RenderSpanBits {
    /// Whole `Render` schedule.
    render:   AtomicU32,
    /// The `PrepareAssets` stage: re-uploading every mesh / image / buffer
    /// asset modified this frame.
    assets:   AtomicU32,
    /// CPU outside the three named segments: extract-commands apply, prepare
    /// meshes and views, specialize, queue, phase sort, bind groups, cleanup.
    prep:     AtomicU32,
    /// The `PrepareViews` stage containing the swapchain acquire — where the
    /// render thread blocks when the GPU is behind.
    gpu_wait: AtomicU32,
    /// The `Render` stage: render-graph execution — pass encoding, submit,
    /// present.
    graph:    AtomicU32,
}

/// Main-world handle to the shared [`RenderSpanBits`].
#[derive(Resource, Clone)]
struct RenderThreadSpans(Arc<RenderSpanBits>);

/// Render-world `Instant` marks at `Render`-schedule set boundaries.
#[derive(Resource)]
struct RenderMarks {
    start:         Instant,
    before_assets: Instant,
    after_assets:  Instant,
    before_views:  Instant,
    after_views:   Instant,
    before_graph:  Instant,
    after_graph:   Instant,
}

impl Default for RenderMarks {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            start:         now,
            before_assets: now,
            after_assets:  now,
            before_views:  now,
            after_views:   now,
            before_graph:  now,
            after_graph:   now,
        }
    }
}

/// Brackets the render app's `Render` schedule with `Instant` marks at its set
/// boundaries and publishes the segment milliseconds to the main world through
/// [`RenderThreadSpans`].
///
/// The render thread runs in parallel with the next main-world frame, so on a
/// GPU-bound frame `render` approaches the frame time — with the excess
/// sitting in `gpu_wait` — while the main-world rows stay small.
struct RenderThreadTimingPlugin;

impl Plugin for RenderThreadTimingPlugin {
    fn build(&self, app: &mut App) {
        let shared = RenderThreadSpans(Arc::new(RenderSpanBits::default()));
        app.insert_resource(shared.clone());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.insert_resource(shared);
        render_app.init_resource::<RenderMarks>();
        render_app.add_systems(
            Render,
            (
                mark_render_start.before(RenderSystems::ExtractCommands),
                mark_before_assets
                    .after(RenderSystems::ExtractCommands)
                    .before(RenderSystems::PrepareAssets),
                mark_after_assets
                    .after(RenderSystems::PrepareAssets)
                    .before(RenderSystems::PrepareMeshes),
                mark_before_views
                    .after(RenderSystems::Specialize)
                    .before(RenderSystems::PrepareViews),
                mark_after_views
                    .after(RenderSystems::PrepareViews)
                    .before(RenderSystems::Queue),
                mark_before_graph
                    .after(RenderSystems::Prepare)
                    .before(RenderSystems::Render),
                mark_after_graph
                    .after(RenderSystems::Render)
                    .before(RenderSystems::Cleanup),
                publish_render_spans.after(RenderSystems::PostCleanup),
            ),
        );
    }
}

fn mark_render_start(mut marks: ResMut<RenderMarks>) { marks.start = Instant::now(); }

fn mark_before_assets(mut marks: ResMut<RenderMarks>) { marks.before_assets = Instant::now(); }

fn mark_after_assets(mut marks: ResMut<RenderMarks>) { marks.after_assets = Instant::now(); }

fn mark_before_views(mut marks: ResMut<RenderMarks>) { marks.before_views = Instant::now(); }

fn mark_after_views(mut marks: ResMut<RenderMarks>) { marks.after_views = Instant::now(); }

fn mark_before_graph(mut marks: ResMut<RenderMarks>) { marks.before_graph = Instant::now(); }

fn mark_after_graph(mut marks: ResMut<RenderMarks>) { marks.after_graph = Instant::now(); }

fn publish_render_spans(marks: Res<RenderMarks>, spans: Res<RenderThreadSpans>) {
    let to_ms = |duration: Duration| duration.as_secs_f32() * MILLISECONDS_PER_SECOND;
    let render = to_ms(marks.start.elapsed());
    let assets = to_ms(marks.after_assets - marks.before_assets);
    let gpu_wait = to_ms(marks.after_views - marks.before_views);
    let graph = to_ms(marks.after_graph - marks.before_graph);
    let prep = (render - assets - gpu_wait - graph).max(0.0);
    spans.0.render.store(render.to_bits(), Ordering::Relaxed);
    spans.0.assets.store(assets.to_bits(), Ordering::Relaxed);
    spans.0.prep.store(prep.to_bits(), Ordering::Relaxed);
    spans
        .0
        .gpu_wait
        .store(gpu_wait.to_bits(), Ordering::Relaxed);
    spans.0.graph.store(graph.to_bits(), Ordering::Relaxed);
}

/// One shared segment value, decoded from its `f32` bits.
fn span_ms(bits: &AtomicU32) -> f32 { f32::from_bits(bits.load(Ordering::Relaxed)) }

// ── Phase-item counts (render thread) ─────────────────────────────────────────

/// Latest per-frame phase-item counts, written on the render thread after
/// `PhaseSort` and read by the overlay on the main thread. Path-agnostic — it
/// counts whichever toggle state is active — so toggle-off vs toggle-on reads
/// come from one session.
#[derive(Default)]
struct DrawCountBits {
    /// `Transparent3d` items summed across views. Text always renders here
    /// (blend or OIT — OIT reuses this phase; the resolve pass adds none).
    transparent: AtomicU32,
    /// Shadow-phase entities summed across light views (batchable +
    /// unbatchable bins, plus one per multidrawable batch set).
    shadow:      AtomicU32,
}

/// Main-world handle to the shared [`DrawCountBits`].
#[derive(Resource, Clone)]
struct DrawCounts(Arc<DrawCountBits>);

/// Counts per-view phase items after `RenderSystems::PhaseSort` into the same
/// shared-atomics channel the waterfall uses — the draws-per-pass number on
/// screen.
struct DrawCountPlugin;

impl Plugin for DrawCountPlugin {
    fn build(&self, app: &mut App) {
        let shared = DrawCounts(Arc::new(DrawCountBits::default()));
        app.insert_resource(shared.clone());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.insert_resource(shared);
        render_app.add_systems(
            Render,
            count_phase_items
                .after(RenderSystems::PhaseSort)
                .before(RenderSystems::Render),
        );
    }
}

fn count_phase_items(
    transparent_phases: Res<ViewSortedRenderPhases<Transparent3d>>,
    shadow_phases: Res<ViewBinnedRenderPhases<Shadow>>,
    counts: Res<DrawCounts>,
) {
    let transparent: usize = transparent_phases
        .values()
        .map(|phase| phase.items.len())
        .sum();
    let shadow: usize = shadow_phases
        .values()
        .map(|phase| {
            phase
                .batchable_meshes
                .values()
                .map(|bin| bin.entities().len())
                .sum::<usize>()
                + phase
                    .unbatchable_meshes
                    .values()
                    .map(|bin| bin.entities.len())
                    .sum::<usize>()
                + phase.multidrawable_meshes.len()
        })
        .sum();
    counts
        .0
        .transparent
        .store(transparent.to_u32(), Ordering::Relaxed);
    counts.0.shadow.store(shadow.to_u32(), Ordering::Relaxed);
}

// ── App ───────────────────────────────────────────────────────────────────────

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`. `with_brp_extras` brings in
    // `FrameTimeDiagnosticsPlugin` (the overlay reads its FPS / frame-time
    // diagnostic IDs below); `with_perf_mode` uncaps vsync and the unfocused
    // winit throttle so the reported frame time reflects true per-frame cost.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_perf_mode()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .size(GROUND_SIZE)
        .with_orbit_cam_preset(
            |cam| {
                cam.focus = GRID_FOCUS;
                cam.radius = Some(8.5);
                cam.yaw = Some(0.0);
                cam.pitch = Some(0.18);
            },
            OrbitCamPreset::BlenderLike,
        )
        .with_stable_transparency()
        .with_camera_home()
        .yaw(0.0)
        .pitch(0.18)
        .with_title_bar(
            TitleBar::new()
                .with_title("Text Stress")
                .with_anchor(Anchor::TopLeft)
                .control(TitleBarControl::segmented(
                    "Space",
                    [TitleBarSegment::new(PAUSE_CHIP, "Pause")],
                ))
                .control(TitleBarControl::segmented(
                    "A",
                    AA_MODES.map(|(id, label, _)| TitleBarSegment::new(id, label)),
                ))
                .control(TitleBarControl::segmented(
                    "T",
                    [TitleBarSegment::new(OIT_CHIP, "OIT")],
                )),
        )
        .wire_chip_to_state::<Mutating, _>(PAUSE_CHIP, |mutating| chip_activation(!mutating.0))
        .wire_chip_to_state::<TextAntiAlias, _>(AA_MODES[0].0, |anti_alias| {
            chip_activation(*anti_alias == AA_MODES[0].2)
        })
        .wire_chip_to_state::<TextAntiAlias, _>(AA_MODES[1].0, |anti_alias| {
            chip_activation(*anti_alias == AA_MODES[1].2)
        })
        .wire_chip_to_state::<TextAntiAlias, _>(AA_MODES[2].0, |anti_alias| {
            chip_activation(*anti_alias == AA_MODES[2].2)
        })
        .wire_chip_to_state::<TextAntiAlias, _>(AA_MODES[3].0, |anti_alias| {
            chip_activation(*anti_alias == AA_MODES[3].2)
        })
        .wire_chip_to_state::<StableTransparencyOn, _>(OIT_CHIP, |enabled| {
            chip_activation(enabled.0)
        })
        .with_camera_control_panel()
        .add_plugins((
            RenderDiagnosticsPlugin,
            RenderThreadTimingPlugin,
            DrawCountPlugin,
        ))
        .init_resource::<FrameCounter>()
        .init_resource::<Mutating>()
        .init_resource::<StableTransparencyOn>()
        .init_resource::<LastDisplayedStatus>()
        .init_resource::<LastDisplayedBatchStats>()
        .insert_resource(MainSpanStart(Instant::now()))
        .init_resource::<MainThreadMs>()
        .add_systems(First, mark_main_span_start)
        .add_systems(Last, publish_main_span)
        .add_systems(
            Startup,
            (
                spawn_labels,
                spawn_home_target,
                spawn_status_overlay,
                spawn_batch_stats_overlay,
            ),
        )
        .add_systems(
            Update,
            (
                toggle_mutation,
                advance_frame,
                mutate_labels,
                update_status_panel,
                update_batch_stats_panel,
            )
                .chain(),
        )
        .add_systems(Update, apply_stable_transparency)
        // Modifier-guarded, so the Ctrl+Shift+A home-gizmo chord doesn't also
        // cycle the AA mode; `T` toggles stable transparency the same way.
        .with_shortcut(KeyCode::KeyA, cycle_text_anti_alias)
        .with_shortcut(KeyCode::KeyT, toggle_stable_transparency)
        .run();
}

const GROUND_SIZE: f32 = GRID_SIDE_F32 * CELL_SPACING + 1.0;
const GRID_FOCUS: Vec3 = Vec3::new(
    0.0,
    GRID_BASE_Y + (GRID_SIDE_F32 - 1.0) * CELL_SPACING * 0.5,
    0.0,
);

// ── Label grid ────────────────────────────────────────────────────────────────

/// Spawns the `GRID_SIDE × GRID_SIDE` grid of standalone world labels, each a
/// `DiegeticText` carrying a `StressLabel(index)` marker.
fn spawn_labels(mut commands: Commands) {
    let half = (GRID_SIDE.to_f32() - 1.0) * 0.5;
    for index in 0..LABEL_COUNT {
        let col = (index % GRID_SIDE).to_f32();
        let row = (index / GRID_SIDE).to_f32();
        let x = (col - half) * CELL_SPACING;
        let y = row.mul_add(CELL_SPACING, GRID_BASE_Y);
        commands.spawn((
            StressLabel(index),
            DiegeticText::world(label_text(index, 0))
                .size(LABEL_SIZE)
                .color(LABEL_COLOR)
                .transform(Transform::from_xyz(x, y, 0.0))
                .build(),
        ));
    }
}

/// The per-label string: a fixed two-digit index and the live three-digit frame
/// counter, so every label's text changes each frame while its width stays
/// stable.
fn label_text(index: usize, frame: u64) -> String { format!("{index:02} {:03}", frame % 1000) }

/// Spawns the single invisible [`CameraHomeTarget`] that `H` frames: an explicit
/// grid-sized [`Aabb`] at [`GRID_FOCUS`], no mesh, so the home union is one
/// constant box.
fn spawn_home_target(mut commands: Commands) {
    commands.spawn((
        CameraHomeTarget,
        Aabb::from_min_max(
            GRID_FOCUS - HOME_REGION_HALF_EXTENTS,
            GRID_FOCUS + HOME_REGION_HALF_EXTENTS,
        ),
        Transform::default(),
    ));
}

// ── Mutation ──────────────────────────────────────────────────────────────────

fn toggle_mutation(keyboard: Res<ButtonInput<KeyCode>>, mut mutating: ResMut<Mutating>) {
    if keyboard.just_pressed(KeyCode::Space) {
        mutating.0 = !mutating.0;
    }
}

/// Flips the desired [`StableTransparencyOn`] state; bound to `T` in `main`.
/// [`apply_stable_transparency`] reconciles the camera component on the change.
fn toggle_stable_transparency(mut enabled: ResMut<StableTransparencyOn>) { enabled.0 = !enabled.0; }

/// Reconciles the orbit camera's `StableTransparency` component to the desired
/// [`StableTransparencyOn`] state, inserting or removing it when the state
/// changes. The `On<Add>` / `On<Remove>` observers in `bevy_diegetic` install
/// or tear down the OIT settings in response.
fn apply_stable_transparency(
    enabled: Res<StableTransparencyOn>,
    camera: Single<(Entity, Has<StableTransparency>), With<FairyDustOrbitCam>>,
    mut commands: Commands,
) {
    if !enabled.is_changed() {
        return;
    }
    let (entity, present) = *camera;
    match (enabled.0, present) {
        (true, false) => {
            commands.entity(entity).insert(StableTransparency);
        },
        (false, true) => {
            commands.entity(entity).remove::<StableTransparency>();
        },
        _ => {},
    }
}

fn advance_frame(mutating: Res<Mutating>, mut frame: ResMut<FrameCounter>) {
    if mutating.0 {
        frame.0 = frame.0.wrapping_add(1);
    }
}

/// Retexts every label through the `DiegeticTextMut` write path. `for_each_mut`
/// yields each label's marker (its grid index) and a `TextEdit` handle, so all
/// 100 strings change in one pass — the `O(n_changed)` worst case the gate
/// targets.
fn mutate_labels(
    mutating: Res<Mutating>,
    frame: Res<FrameCounter>,
    mut labels: DiegeticTextMut<StressLabel>,
) {
    if !mutating.0 {
        return;
    }
    labels.for_each_mut(|label, edit| {
        edit.set_text(label_text(label.0, frame.0));
    });
}

// ── Status overlay ────────────────────────────────────────────────────────────

fn spawn_status_overlay(mut commands: Commands) {
    let unlit = screen_panel_material();
    let built = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::BottomLeft)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_overlay_tree(
            &INITIAL_METRICS.map(String::from),
            &INITIAL_METRICS.map(String::from),
            true,
        ))
        .build();
    match built {
        Ok(built) => {
            commands.spawn((StatusPanel, built, Transform::default()));
        },
        Err(error) => error!("diegetic_text_stress: failed to build status overlay: {error}"),
    }
}

fn spawn_batch_stats_overlay(mut commands: Commands) {
    let unlit = screen_panel_material();
    let built = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::TopRight)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_batch_stats_tree(&batch_stats_rows(
            &BatchStatsValues::default(),
        )))
        .build();
    match built {
        Ok(built) => {
            commands.spawn((BatchStatsPanel, built, Transform::default()));
        },
        Err(error) => error!("diegetic_text_stress: failed to build batch stats overlay: {error}"),
    }
}

fn status_label_style() -> TextStyle {
    TextStyle::new(OVERLAY_FONT_SIZE)
        .with_color(STATUS_LABEL_COLOR)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn status_value_style() -> TextStyle {
    TextStyle::new(OVERLAY_FONT_SIZE)
        .with_color(STATUS_TEXT_COLOR)
        .with_shadow_mode(GlyphShadowMode::None)
}

/// Whether a value cell renders dimmer (headers / footer) or as a live value.
#[derive(Clone, Copy)]
enum CellEmphasis {
    Normal,
    Dim,
}

fn label_cell(builder: &mut LayoutBuilder, text: &str, indented: bool) {
    let indent = if indented { SUB_ROW_INDENT } else { 0.0 };
    builder.with(
        El::new()
            .width(Sizing::fixed(LABEL_COLUMN_WIDTH))
            .height(Sizing::FIT)
            .padding(Padding::new(indent, 0.0, 0.0, 0.0))
            .child_alignment(AlignX::Left, AlignY::Center),
        |builder| {
            builder.text(text, status_label_style());
        },
    );
}

fn value_cell(builder: &mut LayoutBuilder, text: &str, emphasis: CellEmphasis) {
    let style = match emphasis {
        CellEmphasis::Dim => status_label_style(),
        CellEmphasis::Normal => status_value_style(),
    };
    builder.with(
        El::new()
            .width(Sizing::fixed(VALUE_COLUMN_WIDTH))
            .height(Sizing::FIT)
            .child_alignment(AlignX::Right, AlignY::Center),
        |builder| {
            builder.text(text, style);
        },
    );
}

fn table_row(
    builder: &mut LayoutBuilder,
    label: (&str, bool),
    now: &str,
    max: &str,
    emphasis: CellEmphasis,
) {
    let (text, indented) = label;
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(TABLE_COL_GAP)
            .child_alignment(AlignX::Left, AlignY::Center),
        |builder| {
            label_cell(builder, text, indented);
            value_cell(builder, now, emphasis);
            value_cell(builder, max, emphasis);
        },
    );
}

/// Builds the overlay: a `now` and a `5s max` column of right-aligned numerics,
/// one row per metric, with a labels / state footer.
fn build_overlay_tree(
    now: &[String; METRIC_COUNT],
    max: &[String; METRIC_COUNT],
    mutating: bool,
) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    screen_panel_frame(
        &mut builder,
        Sizing::FIT,
        Sizing::FIT,
        DEFAULT_PANEL_BACKGROUND,
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .direction(Direction::TopToBottom)
                    .child_gap(TABLE_ROW_GAP),
                |builder| {
                    table_row(builder, ("", false), "now", "5s max", CellEmphasis::Dim);
                    for index in 0..METRIC_COUNT {
                        table_row(
                            builder,
                            METRIC_ROWS[index],
                            &now[index],
                            &max[index],
                            CellEmphasis::Normal,
                        );
                    }
                    let state = if mutating { "moving" } else { "paused" };
                    table_row(
                        builder,
                        ("labels", false),
                        &LABEL_COUNT.to_string(),
                        state,
                        CellEmphasis::Dim,
                    );
                },
            );
        },
    );
    builder.build()
}

/// Builds the upper-right batch-stats panel: one label/value row per Step-2
/// proof counter.
fn build_batch_stats_tree(rows: &[(&str, String)]) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    screen_panel_frame(
        &mut builder,
        Sizing::FIT,
        Sizing::FIT,
        DEFAULT_PANEL_BACKGROUND,
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .direction(Direction::TopToBottom)
                    .child_gap(TABLE_ROW_GAP),
                |builder| {
                    for (label, value) in rows {
                        builder.with(
                            El::new()
                                .width(Sizing::FIT)
                                .height(Sizing::FIT)
                                .direction(Direction::LeftToRight)
                                .child_gap(TABLE_COL_GAP)
                                .child_alignment(AlignX::Left, AlignY::Center),
                            |builder| {
                                label_cell(builder, label, false);
                                value_cell(builder, value, CellEmphasis::Normal);
                            },
                        );
                    }
                },
            );
        },
    );
    builder.build()
}

fn update_status_panel(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    mutating: Res<Mutating>,
    diegetic_perf: Res<DiegeticPerfStats>,
    main_thread: Res<MainThreadMs>,
    render_spans: Res<RenderThreadSpans>,
    panels: Query<Entity, With<StatusPanel>>,
    mut last_displayed: ResMut<LastDisplayedStatus>,
    mut commands: Commands,
    mut timer: Local<Option<Timer>>,
    mut history: Local<VecDeque<PerfSnapshot>>,
) {
    let frames_per_second = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(Diagnostic::smoothed);

    // Sample every frame so the smoothed `now` mean and the 5-second peak see
    // every value, not one arbitrary frame per second. `ms` is the raw frame
    // delta (not the smoothed diagnostic) so the additive rows derived from it
    // are per-frame exact; the displayed `now` column is window-meaned anyway.
    let frame_ms = time.delta_secs() * MILLISECONDS_PER_SECOND;
    let layout_ms = diegetic_perf.compute_ms;
    let reconcile_ms = diegetic_perf.reconcile_ms;
    let shaping_ms = diegetic_perf.panel_text.shape_ms;
    let mesh_ms = diegetic_perf.panel_text.mesh_build_ms;
    // other = main-schedule wall time minus the four measured diegetic spans:
    // the real main-thread CPU everything else consumes. wait = frame time
    // minus the main schedules: the main thread blocked handing off to the
    // render thread, plus the extract copy. Both clamped at zero — the spans
    // are sampled one frame apart, so a hitch can momentarily invert them.
    let main_span_ms = main_thread.0;
    let other_ms = (main_span_ms - layout_ms - reconcile_ms - shaping_ms - mesh_ms).max(0.0);
    let wait_ms = (frame_ms - main_span_ms).max(0.0);
    let render_ms = span_ms(&render_spans.0.render);
    let assets_ms = span_ms(&render_spans.0.assets);
    let prep_ms = span_ms(&render_spans.0.prep);
    let gpu_wait_ms = span_ms(&render_spans.0.gpu_wait);
    let graph_ms = span_ms(&render_spans.0.graph);
    history.push_back(PerfSnapshot {
        timestamp: time.elapsed_secs(),
        fps: frames_per_second.unwrap_or(0.0).to_f32(),
        frame_ms,
        layout_ms,
        reconcile_ms,
        shaping_ms,
        mesh_ms,
        other_ms,
        wait_ms,
        render_ms,
        assets_ms,
        prep_ms,
        gpu_wait_ms,
        graph_ms,
    });

    let cutoff = time.elapsed_secs() - PERF_PEAK_WINDOW_SECS;
    while history
        .front()
        .is_some_and(|sample| sample.timestamp < cutoff)
    {
        history.pop_front();
    }

    // Rebuild the overlay tree at most once per second. The overlay is itself a
    // panel, so rebuilding it every frame would add its own shaping / mesh work
    // to the very numbers being measured.
    let timer =
        timer.get_or_insert_with(|| Timer::from_seconds(FPS_UPDATE_INTERVAL, TimerMode::Repeating));
    timer.tick(time.delta());
    if !timer.just_finished() {
        return;
    }

    let mean = window_mean(&history);
    let peak = window_peak(&history);

    // Every row's `now` is the window mean, and means are linear, so each
    // block's indented rows sum to their header: `ms` = the six main-thread
    // rows, `render` = `prep` + `gpu wait` + `graph`. The render block
    // overlaps `ms` (it runs on the render thread) and is not part of its sum.
    let now = [
        format!("{:.0}", mean.fps),
        format!("{:.1}", mean.frame_ms),
        format!("{:.2}", mean.layout_ms),
        format!("{:.2}", mean.reconcile_ms),
        format!("{:.2}", mean.shaping_ms),
        format!("{:.2}", mean.mesh_ms),
        format!("{:.2}", mean.other_ms),
        format!("{:.2}", mean.wait_ms),
        format!("{:.1}", mean.render_ms),
        format!("{:.2}", mean.assets_ms),
        format!("{:.2}", mean.prep_ms),
        format!("{:.2}", mean.gpu_wait_ms),
        format!("{:.2}", mean.graph_ms),
    ];
    let max = [
        format!("{:.0}", peak.fps),
        format!("{:.1}", peak.frame_ms),
        format!("{:.2}", peak.layout_ms),
        format!("{:.2}", peak.reconcile_ms),
        format!("{:.2}", peak.shaping_ms),
        format!("{:.2}", peak.mesh_ms),
        format!("{:.2}", peak.other_ms),
        format!("{:.2}", peak.wait_ms),
        format!("{:.1}", peak.render_ms),
        format!("{:.2}", peak.assets_ms),
        format!("{:.2}", peak.prep_ms),
        format!("{:.2}", peak.gpu_wait_ms),
        format!("{:.2}", peak.graph_ms),
    ];

    let key = format!("{}|{}|{}", now.join(","), max.join(","), mutating.0);
    if key != last_displayed.text {
        last_displayed.text.clone_from(&key);
        for entity in &panels {
            commands.set_tree(entity, build_overlay_tree(&now, &max, mutating.0));
        }
    }
}

/// The Step-2 proof-counter values shown in the upper-right panel.
#[derive(Default)]
struct BatchStatsValues {
    batches:           usize,
    runs:              usize,
    glyphs:            usize,
    instance_uploads:  usize,
    run_table_uploads: usize,
    transparent_items: u32,
    shadow_items:      u32,
}

/// Label/value rows for the batch-stats panel.
fn batch_stats_rows(values: &BatchStatsValues) -> Vec<(&str, String)> {
    vec![
        ("batches", values.batches.to_string()),
        ("runs", values.runs.to_string()),
        ("glyphs", values.glyphs.to_string()),
        ("upload i", values.instance_uploads.to_string()),
        ("upload rt", values.run_table_uploads.to_string()),
        ("t3d", values.transparent_items.to_string()),
        ("shadow", values.shadow_items.to_string()),
    ]
}

/// Refreshes the upper-right batch-stats panel: batch store contents and
/// per-pass phase items.
fn update_batch_stats_panel(
    diegetic_perf: Res<DiegeticPerfStats>,
    draw_counts: Res<DrawCounts>,
    panels: Query<Entity, With<BatchStatsPanel>>,
    mut last_displayed: ResMut<LastDisplayedBatchStats>,
    mut commands: Commands,
    mut timer: Local<Option<Timer>>,
    time: Res<Time>,
) {
    let timer =
        timer.get_or_insert_with(|| Timer::from_seconds(FPS_UPDATE_INTERVAL, TimerMode::Repeating));
    timer.tick(time.delta());
    if !timer.just_finished() {
        return;
    }

    let batch = &diegetic_perf.batch;
    let values = BatchStatsValues {
        batches:           batch.batches,
        runs:              batch.runs,
        glyphs:            batch.glyph_records,
        instance_uploads:  batch.instance_uploads,
        run_table_uploads: batch.run_table_uploads,
        transparent_items: draw_counts.0.transparent.load(Ordering::Relaxed),
        shadow_items:      draw_counts.0.shadow.load(Ordering::Relaxed),
    };
    let rows = batch_stats_rows(&values);
    let mut key = String::new();
    for (label, value) in &rows {
        key.push_str(label);
        key.push('=');
        key.push_str(value);
        key.push('|');
    }
    if key != last_displayed.text {
        last_displayed.text.clone_from(&key);
        for entity in &panels {
            commands.set_tree(entity, build_batch_stats_tree(&rows));
        }
    }
}

/// Per-frame mean of each metric across the sample window — the smoothed `now`
/// column. Returns [`PerfSnapshot::ZERO`] for an empty window.
fn window_mean(history: &VecDeque<PerfSnapshot>) -> PerfSnapshot {
    let count = history.len().to_f32();
    if count == 0.0 {
        return PerfSnapshot::ZERO;
    }
    let mut sum = PerfSnapshot::ZERO;
    for sample in history {
        sum.fps += sample.fps;
        sum.frame_ms += sample.frame_ms;
        sum.layout_ms += sample.layout_ms;
        sum.reconcile_ms += sample.reconcile_ms;
        sum.shaping_ms += sample.shaping_ms;
        sum.mesh_ms += sample.mesh_ms;
        sum.other_ms += sample.other_ms;
        sum.wait_ms += sample.wait_ms;
        sum.render_ms += sample.render_ms;
        sum.assets_ms += sample.assets_ms;
        sum.prep_ms += sample.prep_ms;
        sum.gpu_wait_ms += sample.gpu_wait_ms;
        sum.graph_ms += sample.graph_ms;
    }
    PerfSnapshot {
        timestamp:    0.0,
        fps:          sum.fps / count,
        frame_ms:     sum.frame_ms / count,
        layout_ms:    sum.layout_ms / count,
        reconcile_ms: sum.reconcile_ms / count,
        shaping_ms:   sum.shaping_ms / count,
        mesh_ms:      sum.mesh_ms / count,
        other_ms:     sum.other_ms / count,
        wait_ms:      sum.wait_ms / count,
        render_ms:    sum.render_ms / count,
        assets_ms:    sum.assets_ms / count,
        prep_ms:      sum.prep_ms / count,
        gpu_wait_ms:  sum.gpu_wait_ms / count,
        graph_ms:     sum.graph_ms / count,
    }
}

/// Per-metric peak across the sample window — the `5s max` column.
fn window_peak(history: &VecDeque<PerfSnapshot>) -> PerfSnapshot {
    let mut peak = PerfSnapshot::ZERO;
    for sample in history {
        peak.fps = peak.fps.max(sample.fps);
        peak.frame_ms = peak.frame_ms.max(sample.frame_ms);
        peak.layout_ms = peak.layout_ms.max(sample.layout_ms);
        peak.reconcile_ms = peak.reconcile_ms.max(sample.reconcile_ms);
        peak.shaping_ms = peak.shaping_ms.max(sample.shaping_ms);
        peak.mesh_ms = peak.mesh_ms.max(sample.mesh_ms);
        peak.other_ms = peak.other_ms.max(sample.other_ms);
        peak.wait_ms = peak.wait_ms.max(sample.wait_ms);
        peak.render_ms = peak.render_ms.max(sample.render_ms);
        peak.assets_ms = peak.assets_ms.max(sample.assets_ms);
        peak.prep_ms = peak.prep_ms.max(sample.prep_ms);
        peak.gpu_wait_ms = peak.gpu_wait_ms.max(sample.gpu_wait_ms);
        peak.graph_ms = peak.graph_ms.max(sample.graph_ms);
    }
    peak
}
