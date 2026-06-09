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
//!   M — toggle the bottom-center render-timing meter; the title-bar `Meter`
//!     segment highlights while it is on
//!
//! A bottom-left screen overlay reports the frame as two additive blocks, one
//! per thread, each row with a 5-second peak column. Main thread: `ms` is the
//! sum of `layout`, `reconcile`, `shaping`, `mesh`, `other`, and `wait`.
//! Render thread (overlaps `ms` — pipelined rendering): `render` is the sum
//! of `assets`, `prep`, `gpu wait`, and `graph`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use bevy::camera::primitives::Aabb;
use bevy::core_pipeline::Core3d;
use bevy::core_pipeline::Core3dSystems;
use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::diagnostic::Diagnostic;
use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::pbr::Shadow;
use bevy::prelude::*;
use bevy::render::Render;
use bevy::render::RenderApp;
use bevy::render::RenderStartup;
use bevy::render::RenderSystems;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::extract_component::ExtractComponentPlugin;
use bevy::render::render_phase::ViewBinnedRenderPhases;
use bevy::render::render_phase::ViewSortedRenderPhases;
use bevy::render::renderer::RenderContext;
use bevy::render::renderer::RenderDevice;
use bevy::render::renderer::RenderQueue;
use bevy::render::renderer::ViewQuery;
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
use bevy_diegetic::TextAntiAlias;
use bevy_diegetic::TextStyle;
use bevy_kana::ToF32;
use bevy_kana::ToF64;
use bevy_kana::ToU32;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::TitleBar;
use fairy_dust::TitleBarControl;
use fairy_dust::TitleBarSegment;
use fairy_dust::screen_panel_frame;
use fairy_dust::screen_panel_material;
use wgpu::Buffer;
use wgpu::BufferDescriptor;
use wgpu::BufferUsages;
use wgpu::Extent3d;
use wgpu::LoadOp;
use wgpu::MapMode;
use wgpu::Operations;
use wgpu::PollType;
use wgpu::QuerySet;
use wgpu::QuerySetDescriptor;
use wgpu::QueryType;
use wgpu::RenderPassColorAttachment;
use wgpu::RenderPassDescriptor;
use wgpu::RenderPassTimestampWrites;
use wgpu::StoreOp;
use wgpu::TextureDescriptor;
use wgpu::TextureDimension;
use wgpu::TextureFormat;
use wgpu::TextureUsages;
use wgpu::TextureView;
use wgpu::TextureViewDescriptor;

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

// ── Batch-stats panel constants ───────────────────────────────────────────────

/// Larger text on the label/value header line of each batch-stats group.
const STATS_HEADER_FONT_SIZE: f32 = 15.0;
/// Smaller text on the description line under each header.
const STATS_DESC_FONT_SIZE: f32 = 9.0;
/// Dim color for the description line.
const STATS_DESC_COLOR: Color = Color::srgba(0.60, 0.66, 0.76, 0.68);
/// Thin separator line between groups.
const STATS_SEPARATOR_COLOR: Color = Color::srgba(0.50, 0.56, 0.66, 0.30);
/// Separator line thickness in logical pixels.
const STATS_SEPARATOR_THICKNESS: f32 = 1.0;
/// Fixed width of each group: the label/value header spans it and the
/// description wraps within it. Wider than the diagnostics overlay so most
/// descriptions read on one line.
const STATS_ROW_WIDTH: f32 = 260.0;
/// Vertical gap between the header, description, and separator inside a group.
const STATS_INTRA_GAP: f32 = 2.0;
/// Vertical gap between groups.
const STATS_GROUP_GAP: f32 = 6.0;

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

/// Title-bar segment id for the meter (waterfall panel) indicator.
const METER_CHIP: &str = "meter";

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

// ── GPU frame timer ───────────────────────────────────────────────────────────

/// Nanoseconds per millisecond, for converting timestamp ticks to `ms`.
const NANOSECONDS_PER_MILLISECOND: f64 = 1.0e6;
/// Number of timestamp slots in the per-frame query set.
const GPU_TIMER_QUERY_COUNT: u32 = 2;
/// Byte size of the two-slot timestamp read-back: `2 × u64`.
const GPU_TIMER_BYTES: u64 = 16;
/// Byte size of one timestamp query result.
const GPU_TIMESTAMP_BYTES: usize = std::mem::size_of::<u64>();
/// Read-slice byte length for the two timestamp results.
const GPU_TIMER_READ_BYTES: usize = 2 * GPU_TIMESTAMP_BYTES;
/// Upper bound (ms) on a believable single-frame 3D-pass GPU time. A decode
/// above this means the two timestamps didn't pair cleanly (a stale or
/// out-of-order slot); the sample is dropped rather than poisoning the mean.
const GPU_TIMER_MAX_REASONABLE_MS: f32 = 1_000.0;

/// True GPU duration of the main camera's 3D pass set (opaque + transparent +
/// OIT resolve), in milliseconds, stored as `f32` bits. Written on the render
/// thread once a frame's timestamps have been read back, read by the overlay on
/// the main thread.
#[derive(Resource, Clone)]
struct GpuFrameMs(Arc<AtomicU32>);

/// Marks the one camera whose 3D pass set is bracketed by GPU timestamps.
/// Extracted to the render world so the marker systems run for exactly that
/// view and skip the screen-space overlay cameras (which also run `Core3d`).
#[derive(Component, Clone, Copy, ExtractComponent)]
struct GpuTimedView;

/// Read-back cycle position of the single timestamp buffer. Capture starts only
/// from `Idle`, so a frame whose buffer is still mapped is skipped — capture
/// lands roughly every other frame, which the second-scale overlay ignores.
enum GpuTimerPhase {
    /// Free to record a new timestamp pair.
    Idle,
    /// Timestamps resolved + copied this frame; the read buffer needs mapping.
    Pending,
    /// `map_async` is in flight; waiting for the GPU to finish the copy.
    Mapping,
}

/// Render-world GPU timestamp resources, created once in `RenderStartup`.
#[derive(Resource)]
struct GpuTimer {
    /// Two-slot timestamp query set: slot 0 written at the *end* of the start
    /// marker pass (just before the opaque pass), slot 1 at the *beginning* of
    /// the end marker pass (just after the OIT resolve).
    query_set:      QuerySet,
    /// 1×1 throwaway color target the two marker render passes draw into. A
    /// render pass needs an attachment, and a real (non-empty) pass is what makes
    /// Apple Silicon actually record the stage-boundary timestamp — an empty
    /// compute pass records nothing. Isolated from the real frame so it can't
    /// disturb rendering; the marker passes carry no draws, only the timestamp
    /// writes that bracket the main 3D passes in submission order.
    dummy_view:     TextureView,
    /// `QUERY_RESOLVE | COPY_SRC` buffer the query set resolves into.
    resolve_buffer: Buffer,
    /// `COPY_DST | MAP_READ` buffer the resolve buffer is copied into for CPU
    /// read-back.
    read_buffer:    Buffer,
    /// Nanoseconds per timestamp tick, from `Queue::get_timestamp_period`.
    period_ns:      f32,
    /// Read-back cycle position; see [`GpuTimerPhase`].
    phase:          GpuTimerPhase,
    /// Set by the `map_async` callback once the read buffer is mapped.
    mapped:         Arc<AtomicBool>,
}

/// Brackets the main camera's 3D pass set with two GPU timestamps — written at
/// the stage boundaries of two minimal marker render passes, the only place
/// Apple Silicon samples GPU counters — then resolves, reads them back, and
/// publishes the duration to the main world through [`GpuFrameMs`].
struct GpuFrameTimingPlugin;

impl Plugin for GpuFrameTimingPlugin {
    fn build(&self, app: &mut App) {
        let shared = GpuFrameMs(Arc::new(AtomicU32::new(0.0_f32.to_bits())));
        app.insert_resource(shared.clone());
        app.add_plugins(ExtractComponentPlugin::<GpuTimedView>::default());
        // Runs every frame until the camera exists and is marked — the `OrbitCam`
        // is spawned after `Startup`, so a one-shot `Startup` system would miss it.
        app.add_systems(Update, mark_gpu_timed_camera);
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.insert_resource(shared);
        render_app.add_systems(RenderStartup, init_gpu_timer);
        render_app.add_systems(
            Core3d,
            (
                gpu_timer_start
                    .after(Core3dSystems::Prepass)
                    .before(Core3dSystems::MainPass),
                gpu_timer_end
                    .after(Core3dSystems::MainPass)
                    .before(Core3dSystems::PostProcess),
            ),
        );
        render_app.add_systems(Render, gpu_timer_readback.after(RenderSystems::PostCleanup));
    }
}

/// Inserts [`GpuTimedView`] on the main `OrbitCam` so the marker systems bracket
/// its 3D pass set and not the overlay cameras'. Idempotent: the
/// `Without<GpuTimedView>` filter makes it a no-op once the marker is set.
fn mark_gpu_timed_camera(
    mut commands: Commands,
    cameras: Query<Entity, (With<OrbitCam>, Without<GpuTimedView>)>,
) {
    for entity in &cameras {
        commands.entity(entity).insert(GpuTimedView);
    }
}

/// Creates the timestamp query set and the resolve / read buffers from the
/// render device, reading the timestamp period from the queue.
fn init_gpu_timer(mut commands: Commands, device: Res<RenderDevice>, queue: Res<RenderQueue>) {
    let wgpu_device = device.wgpu_device();
    let query_set = wgpu_device.create_query_set(&QuerySetDescriptor {
        label: Some("gpu_frame_timer"),
        ty:    QueryType::Timestamp,
        count: GPU_TIMER_QUERY_COUNT,
    });
    let resolve_buffer = wgpu_device.create_buffer(&BufferDescriptor {
        label:              Some("gpu_frame_timer_resolve"),
        size:               GPU_TIMER_BYTES,
        usage:              BufferUsages::QUERY_RESOLVE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read_buffer = wgpu_device.create_buffer(&BufferDescriptor {
        label:              Some("gpu_frame_timer_read"),
        size:               GPU_TIMER_BYTES,
        usage:              BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    // 1×1 throwaway target the marker passes draw into; the view keeps the
    // texture alive, so the texture handle can drop here.
    let dummy_view = wgpu_device
        .create_texture(&TextureDescriptor {
            label:           Some("gpu_frame_timer_marker"),
            size:            Extent3d {
                width:                 1,
                height:                1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count:    1,
            dimension:       TextureDimension::D2,
            format:          TextureFormat::Rgba8Unorm,
            usage:           TextureUsages::RENDER_ATTACHMENT,
            view_formats:    &[],
        })
        .create_view(&TextureViewDescriptor::default());
    let period_ns = queue.0.get_timestamp_period();
    commands.insert_resource(GpuTimer {
        query_set,
        dummy_view,
        resolve_buffer,
        read_buffer,
        period_ns,
        phase: GpuTimerPhase::Idle,
        mapped: Arc::new(AtomicBool::new(false)),
    });
}

/// Writes timestamp 0 at the *end* of a marker render pass scheduled just before
/// the main camera's opaque pass, so it stamps the GPU clock right as the 3D
/// work begins. Skips while a prior frame's read-back is still in flight.
fn gpu_timer_start(
    timer: Option<Res<GpuTimer>>,
    _view: ViewQuery<(), With<GpuTimedView>>,
    mut ctx: RenderContext,
) {
    let Some(timer) = timer else {
        return;
    };
    if !matches!(timer.phase, GpuTimerPhase::Idle) {
        return;
    }
    drop(
        ctx.command_encoder()
            .begin_render_pass(&RenderPassDescriptor {
                label:                    Some("gpu_timer_start"),
                color_attachments:        &[Some(RenderPassColorAttachment {
                    view:           &timer.dummy_view,
                    depth_slice:    None,
                    resolve_target: None,
                    ops:            Operations {
                        load:  LoadOp::Clear(wgpu::Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         Some(RenderPassTimestampWrites {
                    query_set:                     &timer.query_set,
                    beginning_of_pass_write_index: None,
                    end_of_pass_write_index:       Some(0),
                }),
                occlusion_query_set:      None,
                multiview_mask:           None,
            }),
    );
}

/// Writes timestamp 1 at the *beginning* of a marker render pass scheduled just
/// after the main camera's OIT resolve, then resolves both timestamps and copies
/// them into the read buffer. Skips while a prior frame's read-back is still in
/// flight.
fn gpu_timer_end(
    timer: Option<ResMut<GpuTimer>>,
    _view: ViewQuery<(), With<GpuTimedView>>,
    mut ctx: RenderContext,
) {
    let Some(mut timer) = timer else {
        return;
    };
    if !matches!(timer.phase, GpuTimerPhase::Idle) {
        return;
    }
    drop(
        ctx.command_encoder()
            .begin_render_pass(&RenderPassDescriptor {
                label:                    Some("gpu_timer_end"),
                color_attachments:        &[Some(RenderPassColorAttachment {
                    view:           &timer.dummy_view,
                    depth_slice:    None,
                    resolve_target: None,
                    ops:            Operations {
                        load:  LoadOp::Clear(wgpu::Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         Some(RenderPassTimestampWrites {
                    query_set:                     &timer.query_set,
                    beginning_of_pass_write_index: Some(1),
                    end_of_pass_write_index:       None,
                }),
                occlusion_query_set:      None,
                multiview_mask:           None,
            }),
    );
    let encoder = ctx.command_encoder();
    encoder.resolve_query_set(
        &timer.query_set,
        0..GPU_TIMER_QUERY_COUNT,
        &timer.resolve_buffer,
        0,
    );
    encoder.copy_buffer_to_buffer(
        &timer.resolve_buffer,
        0,
        &timer.read_buffer,
        0,
        GPU_TIMER_BYTES,
    );
    timer.phase = GpuTimerPhase::Pending;
}

/// Maps the read buffer one frame after the copy, decodes the two timestamps to
/// milliseconds, and publishes to [`GpuFrameMs`]. Runs after the render graph
/// has submitted, so the copy is queued before the map.
fn gpu_timer_readback(
    timer: Option<ResMut<GpuTimer>>,
    device: Res<RenderDevice>,
    gpu_ms: Res<GpuFrameMs>,
) {
    let Some(mut timer) = timer else {
        return;
    };
    match timer.phase {
        GpuTimerPhase::Pending => {
            timer.mapped.store(false, Ordering::Relaxed);
            let mapped = timer.mapped.clone();
            timer
                .read_buffer
                .slice(..)
                .map_async(MapMode::Read, move |result| {
                    if result.is_ok() {
                        mapped.store(true, Ordering::Relaxed);
                    }
                });
            timer.phase = GpuTimerPhase::Mapping;
            let _ = device.poll(PollType::Poll);
        },
        GpuTimerPhase::Mapping => {
            let _ = device.poll(PollType::Poll);
            if timer.mapped.load(Ordering::Relaxed) {
                let sample_ms = {
                    let view = timer.read_buffer.slice(..).get_mapped_range();
                    let start = timestamp_from_bytes(&view[0..GPU_TIMESTAMP_BYTES]);
                    let end =
                        timestamp_from_bytes(&view[GPU_TIMESTAMP_BYTES..GPU_TIMER_READ_BYTES]);
                    // `checked_sub` is `None` when the slots didn't land in order
                    // (a frame where the pair wrapped); drop it, keep last good.
                    end.checked_sub(start).map(|ticks| {
                        (ticks.to_f64() * f64::from(timer.period_ns) / NANOSECONDS_PER_MILLISECOND)
                            .to_f32()
                    })
                };
                timer.read_buffer.unmap();
                if let Some(ms) = sample_ms
                    && ms.is_finite()
                    && ms <= GPU_TIMER_MAX_REASONABLE_MS
                {
                    gpu_ms.0.store(ms.to_bits(), Ordering::Relaxed);
                }
                timer.phase = GpuTimerPhase::Idle;
            }
        },
        GpuTimerPhase::Idle => {},
    }
}

const fn timestamp_from_bytes(bytes: &[u8]) -> u64 {
    let mut timestamp = [0_u8; GPU_TIMESTAMP_BYTES];
    timestamp.copy_from_slice(bytes);
    u64::from_le_bytes(timestamp)
}

// ── Phase-item counts (render thread) ─────────────────────────────────────────

/// Latest per-frame phase-item counts, written on the render thread after
/// `PhaseSort` and read by the overlay on the main thread. Path-agnostic — it
/// counts whichever toggle state is active — so toggle-off vs toggle-on reads
/// come from one session.
#[derive(Default)]
struct DrawCountBits {
    /// `Transparent3d` items summed across camera views. Text always renders
    /// here (blend or OIT — OIT reuses this phase; the resolve pass adds none).
    transparent:          AtomicU32,
    /// Shadow-phase caster draws summed across shadow views (batchable +
    /// unbatchable bins, plus one per multidrawable batch set).
    shadow:               AtomicU32,
    /// `Transparent3d` items per camera view, largest first — the components
    /// that sum to `transparent`, so the overlay can show the breakdown.
    transparent_per_view: Mutex<Vec<u32>>,
    /// Caster draws per shadow view (one view per cascade per shadow-casting
    /// light), largest first — the components that sum to `shadow`.
    shadow_per_view:      Mutex<Vec<u32>>,
}

/// Main-world handle to the shared [`DrawCountBits`].
#[derive(Resource, Clone)]
struct DrawCounts(Arc<DrawCountBits>);

/// Counts per-view phase items after `RenderSystems::PhaseSort` into the shared
/// [`DrawCountBits`] the upper-right overlay reads — the `t3d` / `shadow`
/// totals and the per-view breakdown shown beneath each.
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
    let mut transparent_per_view: Vec<u32> = transparent_phases
        .values()
        .map(|phase| phase.items.len().to_u32())
        .collect();
    let mut shadow_per_view: Vec<u32> = shadow_phases
        .values()
        .map(|phase| {
            (phase
                .batchable_meshes
                .values()
                .map(|bin| bin.entities().len())
                .sum::<usize>()
                + phase
                    .unbatchable_meshes
                    .values()
                    .map(|bin| bin.entities.len())
                    .sum::<usize>()
                + phase.multidrawable_meshes.len())
            .to_u32()
        })
        .collect();
    // Phase-map iteration order is not stable; sort largest-first so the
    // on-screen breakdown stays in a fixed order frame to frame.
    transparent_per_view.sort_unstable_by(|a, b| b.cmp(a));
    shadow_per_view.sort_unstable_by(|a, b| b.cmp(a));

    let transparent: u32 = transparent_per_view.iter().sum();
    let shadow: u32 = shadow_per_view.iter().sum();
    counts.0.transparent.store(transparent, Ordering::Relaxed);
    counts.0.shadow.store(shadow, Ordering::Relaxed);
    if let Ok(mut guard) = counts.0.transparent_per_view.lock() {
        *guard = transparent_per_view;
    }
    if let Ok(mut guard) = counts.0.shadow_per_view.lock() {
        *guard = shadow_per_view;
    }
}

// ── App ───────────────────────────────────────────────────────────────────────

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example_gpu_timestamps`, which also requests the
    // wgpu timestamp features so `RenderDiagnosticsPlugin` records real
    // `render/<pass>/elapsed_gpu` timings for the meter's GPU lane.
    // `with_brp_extras` brings in `FrameTimeDiagnosticsPlugin` (the overlay
    // reads its FPS / frame-time diagnostic IDs below); `with_perf_mode` uncaps
    // vsync and the unfocused winit throttle so the reported frame time
    // reflects true per-frame cost.
    fairy_dust::sprinkle_example_gpu_timestamps()
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
                    "M",
                    [TitleBarSegment::new(METER_CHIP, "Meter")],
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
        .wire_chip_to_state::<WaterfallShown, _>(METER_CHIP, |shown| {
            chip_activation(matches!(*shown, WaterfallShown::Shown))
        })
        .with_camera_control_panel()
        .add_plugins((
            RenderThreadTimingPlugin,
            GpuFrameTimingPlugin,
            DrawCountPlugin,
        ))
        .init_resource::<FrameCounter>()
        .init_resource::<Mutating>()
        .init_resource::<WaterfallShown>()
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
                spawn_waterfall_overlay,
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
                update_waterfall_panel,
            )
                .chain(),
        )
        // Modifier-guarded, so the Ctrl+Shift+A home-gizmo chord doesn't also
        // cycle the AA mode.
        .with_shortcut(KeyCode::KeyA, cycle_text_anti_alias)
        .with_shortcut(KeyCode::KeyM, toggle_waterfall)
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

fn stats_header_label_style() -> TextStyle {
    TextStyle::new(STATS_HEADER_FONT_SIZE)
        .with_color(STATUS_LABEL_COLOR)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn stats_header_value_style() -> TextStyle {
    TextStyle::new(STATS_HEADER_FONT_SIZE)
        .with_color(STATUS_TEXT_COLOR)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn stats_desc_style() -> TextStyle {
    TextStyle::new(STATS_DESC_FONT_SIZE)
        .with_color(STATS_DESC_COLOR)
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

/// Builds the upper-right batch-stats panel: one group per counter, each a
/// label/value header line, smaller detail lines, and a thin separator.
fn build_batch_stats_tree(rows: &[BatchStatsRow]) -> LayoutTree {
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
                    .child_gap(STATS_GROUP_GAP),
                |builder| {
                    let last = rows.len().saturating_sub(1);
                    for (index, row) in rows.iter().enumerate() {
                        stats_group(builder, row, index == last);
                    }
                },
            );
        },
    );
    builder.build()
}

/// One batch-stats group: a larger label/value header line, smaller detail
/// lines below it, and a thin separator (omitted on the final group).
fn stats_group(builder: &mut LayoutBuilder, row: &BatchStatsRow, last: bool) {
    builder.with(
        El::new()
            .width(Sizing::fixed(STATS_ROW_WIDTH))
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(STATS_INTRA_GAP),
        |builder| {
            // Header: label left, value pushed to the right edge.
            builder.with(
                El::new()
                    .width(Sizing::fixed(STATS_ROW_WIDTH))
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight)
                    .child_alignment(AlignX::Left, AlignY::Center),
                |builder| {
                    builder.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .child_alignment(AlignX::Left, AlignY::Center),
                        |builder| {
                            builder.text(row.label, stats_header_label_style());
                        },
                    );
                    builder.with(
                        El::new()
                            .width(Sizing::FIT)
                            .height(Sizing::FIT)
                            .child_alignment(AlignX::Right, AlignY::Center),
                        |builder| {
                            builder.text(&row.value, stats_header_value_style());
                        },
                    );
                },
            );
            // Detail lines: smaller text, wraps within the group width. Rows
            // without details (e.g. `profile`) only render their header.
            for detail in &row.details {
                builder.with(
                    El::new()
                        .width(Sizing::fixed(STATS_ROW_WIDTH))
                        .height(Sizing::FIT)
                        .child_alignment(AlignX::Left, AlignY::Top),
                    |builder| {
                        builder.text(detail, stats_desc_style());
                    },
                );
            }
            if !last {
                builder.with(
                    El::new()
                        .width(Sizing::fixed(STATS_ROW_WIDTH))
                        .height(Sizing::fixed(STATS_SEPARATOR_THICKNESS))
                        .background(STATS_SEPARATOR_COLOR),
                    |_builder| {},
                );
            }
        },
    );
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
    batches:              usize,
    runs:                 usize,
    glyphs:               usize,
    transparent_items:    u32,
    /// `Transparent3d` items per camera view (largest first).
    transparent_per_view: Vec<u32>,
    shadow_items:         u32,
    /// Caster draws per shadow view (largest first).
    shadow_per_view:      Vec<u32>,
}

/// One batch-stats group: label/value header plus zero or more detail lines.
struct BatchStatsRow {
    label:   &'static str,
    value:   String,
    details: Vec<String>,
}

/// Label, value, and detail lines for each batch-stats group. The `batches`
/// details enumerate the batch keys this scene routes to (fixed); the `t3d` and
/// `shadow` details are the live per-view breakdown that produced the number
/// this frame, read from the render phases.
fn batch_stats_rows(values: &BatchStatsValues) -> Vec<BatchStatsRow> {
    vec![
        BatchStatsRow {
            label:   "profile",
            value:   if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .to_string(),
            details: Vec::new(),
        },
        BatchStatsRow {
            label:   "batches",
            value:   values.batches.to_string(),
            details: vec![
                "1. world labels".to_string(),
                "2. screen UI (title bar, camera, perf readouts)".to_string(),
            ],
        },
        BatchStatsRow {
            label:   "runs",
            value:   values.runs.to_string(),
            details: vec!["text runs routed across all batches".to_string()],
        },
        BatchStatsRow {
            label:   "glyphs",
            value:   values.glyphs.to_string(),
            details: vec!["glyph instances across all batches".to_string()],
        },
        BatchStatsRow {
            label:   "t3d",
            value:   values.transparent_items.to_string(),
            details: vec![format!(
                "{} across {} camera views",
                join_counts(&values.transparent_per_view),
                values.transparent_per_view.len()
            )],
        },
        BatchStatsRow {
            label:   "shadow",
            value:   values.shadow_items.to_string(),
            details: vec![shadow_breakdown(&values.shadow_per_view)],
        },
    ]
}

/// Joins per-view counts as `a+b+c` (the slice is sorted largest-first
/// upstream), so the on-screen sum reads as the components that produce a total.
fn join_counts(counts: &[u32]) -> String {
    if counts.is_empty() {
        return "0".to_string();
    }
    counts
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join("+")
}

/// The live breakdown behind the `shadow` number. There is one shadow view per
/// cascade per shadow-casting light (this scene: one key light × 4 cascades;
/// the fill light and point light cast none). Each caster that lands in a view
/// adds one draw — a `Cast` text batch counts once for all its glyphs, the
/// ground plane counts once — so the per-view counts (e.g. `2+2+2+1`) sum to
/// the total. Read from the live `Shadow` render phases.
fn shadow_breakdown(per_view: &[u32]) -> String {
    if per_view.is_empty() {
        return "no shadow views yet".to_string();
    }
    format!(
        "{} across {} shadow views (cascades)",
        join_counts(per_view),
        per_view.len()
    )
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
        batches:              batch.batches,
        runs:                 batch.runs,
        glyphs:               batch.glyph_records,
        transparent_items:    draw_counts.0.transparent.load(Ordering::Relaxed),
        transparent_per_view: draw_counts
            .0
            .transparent_per_view
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default(),
        shadow_items:         draw_counts.0.shadow.load(Ordering::Relaxed),
        shadow_per_view:      draw_counts
            .0
            .shadow_per_view
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default(),
    };
    let rows = batch_stats_rows(&values);
    let mut key = String::new();
    for row in &rows {
        key.push_str(row.label);
        key.push('=');
        key.push_str(&row.value);
        key.push('|');
        for detail in &row.details {
            key.push_str(detail);
            key.push('|');
        }
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

// ── Waterfall bar panel ─────────────────────────────────────────────────────────

// colors
/// GPU-busy blocks. The wide block finishes frame N (ends at present); the tail
/// block starts frame N+1 after submit. Width comes from the CPU-clock present
/// anchor, not the timestamp query — see [`gpu_lane_segments`].
const WATERFALL_GPU_COLOR: Color = Color::srgb(0.62, 0.45, 0.95);
/// Transparent spacer for the GPU lane's idle gap (the render `submit` window,
/// when the GPU has presented frame N but has not yet received frame N+1).
/// [`lane_bars`] draws an alpha-0 segment with no background, so it only consumes
/// width and positions the next block.
const WATERFALL_GAP_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);
/// Lane `main N`, `work` segment (main-thread CPU span).
const WATERFALL_MAIN_WORK_COLOR: Color = Color::srgb(0.30, 0.60, 0.95);
/// Lane `render N-1`, `prep` segment (`assets` + `prep`).
const WATERFALL_PREP_COLOR: Color = Color::srgb(0.28, 0.80, 0.85);
/// Lane `render N-1`, `submit` segment (`graph`).
const WATERFALL_SUBMIT_COLOR: Color = Color::srgb(0.35, 0.85, 0.45);
/// Track tint behind each lane — the empty tail past the drawn segments.
const WATERFALL_TRACK_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.06);
/// Shared between `main N` `wait` and `render N-1` `gpu wait` — both are stalls.
const WATERFALL_WAIT_COLOR: Color = Color::srgb(0.95, 0.65, 0.20);

// layout
/// Full lane width in panel pixels — maps to the dynamic axis
/// ([`WaterfallBars::axis`], the smoothed frame time).
const WATERFALL_BAR_WIDTH: f32 = 240.0;
/// Gap between the label column and the bar column in panel pixels.
const WATERFALL_LABEL_GAP: f32 = 6.0;
/// Vertical gap between lanes in panel pixels.
const WATERFALL_LANE_GAP: f32 = 4.0;
/// Lane bar thickness in panel pixels.
const WATERFALL_LANE_HEIGHT: f32 = 12.0;
/// Lane row height in panel pixels — the label cell and the bar wrapper share
/// it so the two columns align; tall enough not to clip the label text.
const WATERFALL_LANE_ROW_HEIGHT: f32 = 18.0;
/// Axis floor (ms) — keeps the bar scale finite before the first frame samples
/// and on a stall.
const WATERFALL_MIN_AXIS_MS: f32 = 1.0;
/// Segments thinner than this (px) are dropped so zero-width quads aren't spawned.
const WATERFALL_MIN_SEGMENT_PX: f32 = 0.5;

// timing
/// How long each on-screen picture is sampled before it refreshes (seconds).
/// Per-frame values are averaged over this window so the held picture is the
/// second's mean, not one noisy frame.
const WATERFALL_SAMPLE_PERIOD: f32 = 1.0;
/// How long the bars take to slide from the old picture to the new one at each
/// refresh (seconds). After the morph the picture holds for the rest of the
/// sample period (≈800 ms), giving a still frame to read.
const WATERFALL_MORPH_DURATION: f32 = 0.2;

/// Marker for the bottom-center waterfall bar panel.
#[derive(Component)]
struct WaterfallPanel;

/// Whether the waterfall panel is shown; `M` toggles it and the title-bar
/// `Meter` segment highlights while it is on. When hidden, the panel root takes
/// `Visibility::Hidden` and `update_waterfall_panel` skips the rebuild.
#[derive(Resource, Default)]
enum WaterfallShown {
    #[default]
    Shown,
    Hidden,
}

/// Lane values for the waterfall panel, in milliseconds. The update samples a
/// one-second mean into this, then morphs the on-screen copy toward it over
/// [`WATERFALL_MORPH_DURATION`]. Each lane carries only its short static label as
/// text — the bars are colored rects — so a rebuild re-lays three strings, a
/// cost negligible against the per-frame label load.
#[derive(Clone, Copy, Default)]
struct WaterfallBars {
    /// Frame time (ms) the lanes are scaled against — the dynamic axis.
    axis:      f32,
    main_work: f32,
    main_wait: f32,
    prep:      f32,
    gpu_wait:  f32,
    submit:    f32,
    /// Measured GPU time of the `OrbitCam` opaque `MainPass` only (`GpuFrameMs`) — a
    /// sliver of the frame's GPU work (it excludes the prepass, the OIT resolve,
    /// and every overlay camera), so it does NOT drive the GPU lane. The lane is
    /// built from the present anchor instead; this is kept for a future
    /// "main-pass GPU" sub-readout.
    gpu:       f32,
}

impl WaterfallBars {
    /// Field-wise sum, for accumulating the per-second mean.
    fn add(self, other: Self) -> Self {
        Self {
            axis:      self.axis + other.axis,
            main_work: self.main_work + other.main_work,
            main_wait: self.main_wait + other.main_wait,
            prep:      self.prep + other.prep,
            gpu_wait:  self.gpu_wait + other.gpu_wait,
            submit:    self.submit + other.submit,
            gpu:       self.gpu + other.gpu,
        }
    }

    /// Field-wise scale, dividing the accumulated sum by the sample count.
    fn scale(self, factor: f32) -> Self {
        Self {
            axis:      self.axis * factor,
            main_work: self.main_work * factor,
            main_wait: self.main_wait * factor,
            prep:      self.prep * factor,
            gpu_wait:  self.gpu_wait * factor,
            submit:    self.submit * factor,
            gpu:       self.gpu * factor,
        }
    }

    /// Field-wise lerp from `self` toward `to` by fraction `t`, for the morph.
    const fn lerp(self, to: Self, t: f32) -> Self {
        Self {
            axis:      lerp(self.axis, to.axis, t),
            main_work: lerp(self.main_work, to.main_work, t),
            main_wait: lerp(self.main_wait, to.main_wait, t),
            prep:      lerp(self.prep, to.prep, t),
            gpu_wait:  lerp(self.gpu_wait, to.gpu_wait, t),
            submit:    lerp(self.submit, to.submit, t),
            gpu:       lerp(self.gpu, to.gpu, t),
        }
    }
}

fn spawn_waterfall_overlay(mut commands: Commands) {
    let unlit = screen_panel_material();
    let built = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::BottomCenter)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_waterfall_tree(&WaterfallBars::default()))
        .build();
    match built {
        Ok(built) => {
            commands.spawn((WaterfallPanel, built, Transform::default()));
        },
        Err(error) => error!("diegetic_text_stress: failed to build waterfall overlay: {error}"),
    }
}

/// Builds the panel: a left label column (`Fit` width, sized to the widest
/// label and right-flushed) beside a bar column, so every bar starts at the
/// same x. Lane rows in both columns share [`WATERFALL_LANE_ROW_HEIGHT`] to keep
/// them aligned.
fn build_waterfall_tree(bars: &WaterfallBars) -> LayoutTree {
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
                    .direction(Direction::LeftToRight)
                    .child_gap(WATERFALL_LABEL_GAP)
                    .child_alignment(AlignX::Left, AlignY::Top),
                |builder| {
                    builder.with(
                        El::new()
                            .width(Sizing::FIT)
                            .height(Sizing::FIT)
                            .direction(Direction::TopToBottom)
                            .child_gap(WATERFALL_LANE_GAP)
                            .child_alignment(AlignX::Right, AlignY::Center),
                        |builder| {
                            lane_label(builder, "Main world  N+2");
                            lane_label(builder, "Render world  N+1");
                            lane_label(builder, "GPU  N");
                        },
                    );
                    builder.with(
                        El::new()
                            .width(Sizing::FIT)
                            .height(Sizing::FIT)
                            .direction(Direction::TopToBottom)
                            .child_gap(WATERFALL_LANE_GAP),
                        |builder| {
                            lane_bars(
                                builder,
                                bars.axis,
                                &[
                                    (bars.main_work, WATERFALL_MAIN_WORK_COLOR),
                                    (bars.main_wait, WATERFALL_WAIT_COLOR),
                                ],
                            );
                            lane_bars(
                                builder,
                                bars.axis,
                                &[
                                    (bars.prep, WATERFALL_PREP_COLOR),
                                    (bars.gpu_wait, WATERFALL_WAIT_COLOR),
                                    (bars.submit, WATERFALL_SUBMIT_COLOR),
                                ],
                            );
                            lane_bars(builder, bars.axis, &gpu_lane_segments(bars));
                        },
                    );
                },
            );
        },
    );
    builder.build()
}

/// One label cell, sized to its text and vertically centered in a shared lane
/// row. The label column's `Fit` width and `AlignX::Right` flush every label
/// against the bars.
fn lane_label(builder: &mut LayoutBuilder, label: &str) {
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::fixed(WATERFALL_LANE_ROW_HEIGHT))
            .child_alignment(AlignX::Right, AlignY::Center),
        |builder| {
            builder.text(label, status_label_style());
        },
    );
}

/// One lane's bar track: a fixed-width track tinted [`WATERFALL_TRACK_COLOR`]
/// with each `(ms, color)` segment drawn left-to-right at a width proportional
/// to `axis_ms` (the frame time). An alpha-0 color draws a transparent spacer —
/// it consumes width and positions the next block, used by the GPU lane for its
/// leading offset and the gap between its two blocks. The track sits in a shared
/// lane row so it lines up with its label; the empty tail shows the track tint.
fn lane_bars(builder: &mut LayoutBuilder, axis_ms: f32, segments: &[(f32, Color)]) {
    let axis = axis_ms.max(WATERFALL_MIN_AXIS_MS);
    builder.with(
        El::new()
            .width(Sizing::fixed(WATERFALL_BAR_WIDTH))
            .height(Sizing::fixed(WATERFALL_LANE_ROW_HEIGHT))
            .direction(Direction::LeftToRight)
            .child_alignment(AlignX::Left, AlignY::Center),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::fixed(WATERFALL_BAR_WIDTH))
                    .height(Sizing::fixed(WATERFALL_LANE_HEIGHT))
                    .direction(Direction::LeftToRight)
                    .background(WATERFALL_TRACK_COLOR),
                |builder| {
                    let mut used = 0.0;
                    for &(ms, color) in segments {
                        let remaining = WATERFALL_BAR_WIDTH - used;
                        let px = (ms / axis * WATERFALL_BAR_WIDTH).clamp(0.0, remaining);
                        if px < WATERFALL_MIN_SEGMENT_PX {
                            continue;
                        }
                        used += px;
                        let cell = El::new()
                            .width(Sizing::fixed(px))
                            .height(Sizing::fixed(WATERFALL_LANE_HEIGHT));
                        // Transparent gap: no background, only width.
                        let cell = if color.alpha() > 0.0 {
                            cell.background(color)
                        } else {
                            cell
                        };
                        builder.with(cell, |_builder| {});
                    }
                },
            );
        },
    );
}

/// The GPU lane's segments for one period, left to right: the GPU **busy**
/// finishing frame N (ends at the present instant), the short **idle** gap while
/// the render thread records and submits frame N+1's commands, then the GPU
/// **busy** again starting frame N+1.
///
/// Driven by the CPU-clock present anchor, not the timestamp query: `present` =
/// `prep` + `gpu_wait` is the instant the render thread's `gpu wait` (swapchain
/// acquire) releases, which is the instant the GPU finishes frame N and frees an
/// image. So the busy block's right edge lines up vertically with the render
/// lane's `gpu wait` → `submit` boundary — the GPU is busy *through* the render
/// thread's wait, which is why that wait exists. The next busy block starts at
/// `present` + `submit`, lined up with the render lane's `submit` → end boundary:
/// the GPU picks up frame N+1 the instant its commands are submitted. The GPU is
/// the bottleneck under `with_perf_mode` (no vsync), so the only idle is that
/// short `submit` gap.
fn gpu_lane_segments(bars: &WaterfallBars) -> [(f32, Color); 3] {
    let present = bars.prep + bars.gpu_wait;
    let tail = (bars.axis - present - bars.submit).max(0.0);
    [
        (present, WATERFALL_GPU_COLOR),
        (bars.submit, WATERFALL_GAP_COLOR),
        (tail, WATERFALL_GPU_COLOR),
    ]
}

/// Per-second timeline animation state, held across frames in a `Local`.
///
/// Each frame's instantaneous lane values are summed into `accum` over one
/// sample period. At the period boundary the mean becomes `target`, the
/// on-screen `displayed` becomes `from`, and `morph` resets to 0. For the next
/// [`WATERFALL_MORPH_DURATION`] the bars slide `from` → `target`; after that they
/// hold until the following boundary.
#[derive(Default)]
struct WaterfallAnim {
    /// On-screen lane values this frame.
    displayed: WaterfallBars,
    /// Lane values at the start of the current morph.
    from:      WaterfallBars,
    /// The latest one-second mean — the morph destination and held picture.
    target:    WaterfallBars,
    /// Running field-wise sum of this period's frames.
    accum:     WaterfallBars,
    /// Frames summed into `accum`.
    samples:   u32,
    /// Seconds into the current sample period.
    sampled:   f32,
    /// Seconds since the last sample boundary (drives the morph fraction).
    morph:     f32,
    /// Whether the first sample has primed `displayed`/`from`/`target`.
    primed:    bool,
}

/// Samples the three lanes into a one-second mean and animates the panel toward
/// it: a [`WATERFALL_MORPH_DURATION`] slide at each boundary, then a still hold
/// for the rest of the second. `main` = `work` (main-thread span) + `wait`
/// (frame time past it); `render` = `prep` (`assets` + `prep`) + `gpu wait` +
/// `submit` (`graph`); the GPU lane is built from the present anchor
/// (`prep` + `gpu_wait`) — see [`gpu_lane_segments`].
fn update_waterfall_panel(
    time: Res<Time>,
    main_thread: Res<MainThreadMs>,
    render_spans: Res<RenderThreadSpans>,
    gpu_ms: Res<GpuFrameMs>,
    shown: Res<WaterfallShown>,
    panels: Query<Entity, With<WaterfallPanel>>,
    mut commands: Commands,
    mut anim: Local<WaterfallAnim>,
) {
    let delta_secs = time.delta_secs();
    let frame_ms = delta_secs * MILLISECONDS_PER_SECOND;
    let main_span_ms = main_thread.0;
    let instant = WaterfallBars {
        axis:      frame_ms,
        main_work: main_span_ms,
        main_wait: (frame_ms - main_span_ms).max(0.0),
        prep:      span_ms(&render_spans.0.assets) + span_ms(&render_spans.0.prep),
        gpu_wait:  span_ms(&render_spans.0.gpu_wait),
        submit:    span_ms(&render_spans.0.graph),
        gpu:       span_ms(&gpu_ms.0),
    };

    if !anim.primed {
        anim.displayed = instant;
        anim.from = instant;
        anim.target = instant;
        anim.primed = true;
    }

    anim.accum = anim.accum.add(instant);
    anim.samples += 1;
    anim.sampled += delta_secs;
    anim.morph += delta_secs;

    // Period boundary: average the second, start a new morph toward it.
    if anim.sampled >= WATERFALL_SAMPLE_PERIOD {
        let mean = anim.accum.scale(1.0 / anim.samples.max(1).to_f32());
        anim.from = anim.displayed;
        anim.target = mean;
        anim.accum = WaterfallBars::default();
        anim.samples = 0;
        anim.sampled = 0.0;
        anim.morph = 0.0;
    }

    // Smoothstep the morph fraction so the slide eases in and out.
    let raw = (anim.morph / WATERFALL_MORPH_DURATION).clamp(0.0, 1.0);
    let eased = raw * raw * 2.0f32.mul_add(-raw, 3.0);
    anim.displayed = anim.from.lerp(anim.target, eased);

    // Rebuild only while the bars are moving (the morph plus one settle frame);
    // during the hold the picture is unchanged, so the tree is left as-is.
    let moving = anim.morph <= WATERFALL_MORPH_DURATION + delta_secs;
    if !moving || matches!(*shown, WaterfallShown::Hidden) {
        return;
    }
    let displayed = anim.displayed;
    for entity in &panels {
        commands.set_tree(entity, build_waterfall_tree(&displayed));
    }
}

/// Toggles the waterfall panel (`M`): flips [`WaterfallShown`] and sets the
/// panel root's `Visibility` to match. Hidden panels also skip the rebuild in
/// [`update_waterfall_panel`].
fn toggle_waterfall(
    mut shown: ResMut<WaterfallShown>,
    panels: Query<Entity, With<WaterfallPanel>>,
    mut commands: Commands,
) {
    *shown = match *shown {
        WaterfallShown::Shown => WaterfallShown::Hidden,
        WaterfallShown::Hidden => WaterfallShown::Shown,
    };
    let visibility = match *shown {
        WaterfallShown::Shown => Visibility::Inherited,
        WaterfallShown::Hidden => Visibility::Hidden,
    };
    for entity in &panels {
        commands.entity(entity).insert(visibility);
    }
}

/// Linear interpolation from `from` to `to` by fraction `t`.
const fn lerp(from: f32, to: f32, t: f32) -> f32 { from + (to - from) * t }
