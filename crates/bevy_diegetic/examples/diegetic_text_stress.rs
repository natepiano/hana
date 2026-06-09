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
//!   M — toggle the bottom-left render-timing meter; the title-bar `Meter`
//!     segment highlights while it is on
//!   O — toggle stable transparency / OIT; the title-bar `OIT` segment
//!     highlights while it is on
//!
//! A left screen overlay, placed immediately below the title bar, reports the
//! frame as two additive blocks, one per thread, each row with a 5-second peak
//! column. Main thread: `ms/frame` is the sum of `layout`, `reconcile`,
//! `shaping`, `mesh`, `other`, `wait for render`, `extract`, and
//! `frame slack`. Render thread: `render cycle` is the end-to-end frame N
//! cycle from one render schedule start to the next: `assets`, `prep`,
//! `wait for GPU`, `render graph`, `cleanup`, `return`, and
//! `extract handoff`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::time::Instant;

use bevy::camera::primitives::Aabb;
use bevy::core_pipeline::Core3d;
use bevy::core_pipeline::Core3dSystems;
use bevy::diagnostic::Diagnostic;
use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::pbr::Shadow;
use bevy::prelude::*;
use bevy::render::Extract;
use bevy::render::ExtractSchedule;
use bevy::render::Render;
use bevy::render::RenderApp;
use bevy::render::RenderStartup;
use bevy::render::RenderSystems;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::extract_component::ExtractComponentPlugin;
use bevy::render::render_phase::ViewBinnedRenderPhases;
use bevy::render::renderer::RenderContext;
use bevy::render::renderer::RenderDevice;
use bevy::render::renderer::RenderQueue;
use bevy::render::renderer::ViewQuery;
use bevy::window::PrimaryWindow;
use bevy::window::Window;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::CoordinateSpace;
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
use bevy_diegetic::PanelDimensions;
use bevy_diegetic::PanelDimensionsChanged;
use bevy_diegetic::Percent;
use bevy_diegetic::ScreenPosition;
use bevy_diegetic::Sizing;
use bevy_diegetic::StableTransparency;
use bevy_diegetic::TextAlign;
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

// ── App — plugin wiring, resources, startup/update systems, shortcuts ────────

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
                cam.radius = Some(CAMERA_INITIAL_RADIUS);
                cam.yaw = Some(0.0);
                cam.pitch = Some(0.18);
            },
            OrbitCamPreset::BlenderLike,
        )
        .with_stable_transparency()
        .with_camera_home()
        .yaw(0.0)
        .pitch(0.18)
        .with_title_bar(text_stress_title_bar())
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
        .wire_chip_to_state::<OitState, _>(OIT_CHIP, |oit| chip_activation(oit.0))
        .with_camera_control_panel()
        .add_plugins((
            RenderThreadTimingPlugin,
            GpuFrameTimingPlugin,
            DrawCountPlugin,
        ))
        .init_resource::<FrameCounter>()
        .init_resource::<Mutating>()
        .init_resource::<OitState>()
        .init_resource::<WaterfallShown>()
        .init_resource::<LastDisplayedStatus>()
        .init_resource::<LastDisplayedBatchStats>()
        .insert_resource(MainSpanStart(Instant::now()))
        .insert_resource(MainScheduleEnd(Instant::now()))
        .init_resource::<MainThreadMs>()
        .add_systems(First, mark_main_span_start)
        .add_systems(Last, publish_main_span)
        .add_observer(place_status_panel_below_title_bar)
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
        .with_shortcut(KeyCode::KeyO, toggle_oit)
        .run();
}

fn text_stress_title_bar() -> TitleBar {
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
        ))
        .control(TitleBarControl::segmented(
            "O",
            [TitleBarSegment::new(OIT_CHIP, "OIT")],
        ))
}

// ── Text write path — DiegeticText labels retext each frame via DiegeticTextMut ───

// How it works: `spawn_labels` runs once at startup, spawning a
// GRID_SIDE × GRID_SIDE grid of standalone `DiegeticText` world labels, each
// tagged `StressLabel(index)`. Each frame `toggle_mutation` reads Space to flip
// `Mutating`, `advance_frame` bumps `FrameCounter`, and `mutate_labels` walks
// every label through `DiegeticTextMut::for_each_mut`, rewriting its string with
// `set_text` — a tree-authoritative write plus relayout on all 100 labels, the
// O(n_changed) worst case the perf gate targets.

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

const GRID_FOCUS: Vec3 = Vec3::new(
    0.0,
    GRID_BASE_Y + (GRID_SIDE_F32 - 1.0) * CELL_SPACING * 0.5,
    0.0,
);

/// Marks a stress label and carries its grid index, so `for_each_mut` can write
/// a distinct string per label.
#[derive(Component, Clone, Copy)]
struct StressLabel(usize);

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

/// The per-label string: a fixed two-digit index and the live three-digit frame
/// counter, so every label's text changes each frame while its width stays
/// stable.
fn label_text(index: usize, frame: u64) -> String { format!("{index:02} {:03}", frame % 1000) }

// ── Camera home & ground — home-frame target and ground-plane size ───────────

/// Keeps the label grid framed 10% farther away in the start and home views.
const CAMERA_DISTANCE_SCALE: f32 = 1.1;
const CAMERA_INITIAL_RADIUS: f32 = 8.5 * CAMERA_DISTANCE_SCALE;

/// World-space half-extents of the static camera-home region, centered on
/// [`GRID_FOCUS`]. Covers the label-anchor span (half the grid each way) plus a
/// few label heights so the glyphs sit inside the framed box, then scales the
/// box to match the farther starting camera radius. The home fit adds its own
/// screen-fraction margin on top.
const HOME_REGION_HALF_EXTENTS: Vec3 = Vec3::new(
    ((GRID_SIDE_F32 - 1.0) * CELL_SPACING * 0.5 + LABEL_SIZE * 3.0) * CAMERA_DISTANCE_SCALE,
    ((GRID_SIDE_F32 - 1.0) * CELL_SPACING * 0.5 + LABEL_SIZE * 1.5) * CAMERA_DISTANCE_SCALE,
    LABEL_SIZE * CAMERA_DISTANCE_SCALE,
);

const GROUND_SIZE: f32 = GRID_SIDE_F32 * CELL_SPACING + 1.0;

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

// ── Title-bar controls — pause, anti-alias cycle, meter and OIT toggles ──────

/// Title-bar segment id for the pause indicator.
const PAUSE_CHIP: &str = "pause";

/// Title-bar segment id for the meter (waterfall panel) indicator.
const METER_CHIP: &str = "meter";

/// Title-bar segment id for the stable-transparency / OIT indicator.
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

/// Whether the main camera uses [`StableTransparency`].
#[derive(Resource)]
struct OitState(bool);

impl Default for OitState {
    fn default() -> Self { Self(true) }
}

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

/// Toggles [`StableTransparency`] / OIT on the main scene camera.
fn toggle_oit(
    mut state: ResMut<OitState>,
    cameras: Query<Entity, With<OrbitCam>>,
    mut commands: Commands,
) {
    state.0 = !state.0;
    for camera in &cameras {
        if state.0 {
            commands.entity(camera).insert(StableTransparency);
        } else {
            commands.entity(camera).remove::<StableTransparency>();
        }
    }
}

// ── Measurement — main-thread span: frame wall time and the recv block ───────

/// Start instant of the current main-world frame, recorded in `First`.
#[derive(Resource)]
struct MainSpanStart(Instant);

/// Wall time of the previous main-world schedule run (`First` → `Last`) in
/// milliseconds. The `other` row is this minus the four measured diegetic
/// spans; the `wait` row is the frame time minus this.
#[derive(Resource, Default)]
struct MainThreadMs(f32);

/// Instant the main schedule finished (end of `Last`). Read on the main thread
/// inside `ExtractSchedule` (which runs during `renderer_extract`) to measure
/// the `recv` block: `extract_begin - main_schedule_end`.
#[derive(Resource)]
struct MainScheduleEnd(Instant);

fn mark_main_span_start(mut start: ResMut<MainSpanStart>) { start.0 = Instant::now(); }

fn publish_main_span(
    start: Res<MainSpanStart>,
    mut main_thread: ResMut<MainThreadMs>,
    mut schedule_end: ResMut<MainScheduleEnd>,
    spans: Res<RenderThreadSpans>,
) {
    let end = Instant::now();
    main_thread.0 = duration_ms(start.0, end);
    schedule_end.0 = end;
    spans.0.store_offset(&spans.0.main_start_ms, start.0);
    spans.0.store_offset(&spans.0.main_end_ms, end);
}

/// First system in `ExtractSchedule` (runs on the main thread the instant the
/// `recv().await` in `renderer_extract` unblocks). Stores the wait the main
/// thread just spent blocked on the render thread returning the previous
/// frame's render app — the true Main-lane `wait`.
fn mark_extract_begin(main_end: Extract<Res<MainScheduleEnd>>, spans: Res<RenderThreadSpans>) {
    let now = Instant::now();
    let recv = duration_ms(main_end.0, now);
    let extract_begin_ms = duration_ms(spans.0.epoch, now);
    let render_end_ms = span_ms(&spans.0.render_end_ms);
    let return_gap = timeline_duration_ms(render_end_ms, extract_begin_ms).unwrap_or(0.0);
    spans.0.recv.store(recv.to_bits(), Ordering::Relaxed);
    spans
        .0
        .extract_begin_ms
        .store(extract_begin_ms.to_bits(), Ordering::Relaxed);
    spans
        .0
        .return_gap
        .store(return_gap.to_bits(), Ordering::Relaxed);
}

// ── Measurement — render-thread spans: per-stage render timings ──────────────

/// Latest render-thread segment values in milliseconds, stored as `f32` bits.
/// Written at the end of each `Render` schedule run on the render thread and
/// read by the overlay on the main thread. The segments sum to `render`.
struct RenderSpanBits {
    /// Shared epoch for every timeline offset written by either thread.
    epoch:            Instant,
    /// Whole `Render` schedule.
    render:           AtomicU32,
    /// The `PrepareAssets` stage: re-uploading every mesh / image / buffer
    /// asset modified this frame.
    assets:           AtomicU32,
    /// CPU before `PrepareViews` / render graph: extract-commands apply,
    /// prepare meshes and views, specialize, queue, phase sort, bind groups.
    prep:             AtomicU32,
    /// The `PrepareViews` stage containing the swapchain acquire — where the
    /// render thread blocks when the GPU is behind.
    gpu_wait:         AtomicU32,
    /// The `Render` stage: render-graph execution — pass encoding, submit,
    /// present.
    render_graph:     AtomicU32,
    /// Render cleanup and schedule closeout after the render graph has run.
    cleanup:          AtomicU32,
    /// Main-thread block in `renderer_extract`: the `recv().await` that waits
    /// for the render thread to return the previous frame's render app (after
    /// its whole `Render` schedule, render graph and cleanup included) before
    /// extract can run.
    /// Measured on the main thread as `extract_begin - main_schedule_end`; this
    /// is what the Main lane's `wait` actually is.
    recv:             AtomicU32,
    /// Render thread parked after a complete `Render` schedule, waiting for the
    /// main thread to extract and return the render app.
    wait_for_extract: AtomicU32,
    /// Gap between render-schedule publication and the main thread beginning
    /// extract: schedule closeout, return-app handoff, and main `recv` unblock
    /// overhead.
    return_gap:       AtomicU32,
    /// Main `First` mark, as milliseconds since [`Self::epoch`].
    main_start_ms:    AtomicU32,
    /// Main `Last` mark, as milliseconds since [`Self::epoch`].
    main_end_ms:      AtomicU32,
    /// Main-thread `ExtractSchedule` begin mark, as milliseconds since
    /// [`Self::epoch`].
    extract_begin_ms: AtomicU32,
    /// Render schedule start mark, as milliseconds since [`Self::epoch`].
    render_start_ms:  AtomicU32,
    /// Render schedule published-end mark, as milliseconds since [`Self::epoch`].
    render_end_ms:    AtomicU32,
}

impl RenderSpanBits {
    const fn new(epoch: Instant) -> Self {
        Self {
            epoch,
            render: AtomicU32::new(0),
            assets: AtomicU32::new(0),
            prep: AtomicU32::new(0),
            gpu_wait: AtomicU32::new(0),
            render_graph: AtomicU32::new(0),
            cleanup: AtomicU32::new(0),
            recv: AtomicU32::new(0),
            wait_for_extract: AtomicU32::new(0),
            return_gap: AtomicU32::new(0),
            main_start_ms: AtomicU32::new(0),
            main_end_ms: AtomicU32::new(0),
            extract_begin_ms: AtomicU32::new(0),
            render_start_ms: AtomicU32::new(0),
            render_end_ms: AtomicU32::new(0),
        }
    }

    fn store_offset(&self, bits: &AtomicU32, instant: Instant) {
        bits.store(
            duration_ms(self.epoch, instant).to_bits(),
            Ordering::Relaxed,
        );
    }
}

/// Main-world handle to the shared [`RenderSpanBits`].
#[derive(Resource, Clone)]
struct RenderThreadSpans(Arc<RenderSpanBits>);

/// Render-world `Instant` marks at `Render`-schedule set boundaries.
#[derive(Resource)]
struct RenderMarks {
    start:               Instant,
    before_assets:       Instant,
    after_assets:        Instant,
    before_views:        Instant,
    after_views:         Instant,
    before_render_graph: Instant,
    after_render_graph:  Instant,
    end:                 Instant,
    completed:           bool,
}

impl Default for RenderMarks {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            start:               now,
            before_assets:       now,
            after_assets:        now,
            before_views:        now,
            after_views:         now,
            before_render_graph: now,
            after_render_graph:  now,
            end:                 now,
            completed:           false,
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
        let shared = RenderThreadSpans(Arc::new(RenderSpanBits::new(Instant::now())));
        app.insert_resource(shared.clone());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.insert_resource(shared);
        render_app.init_resource::<RenderMarks>();
        render_app.add_systems(ExtractSchedule, mark_extract_begin);
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
                mark_before_render_graph
                    .after(RenderSystems::Prepare)
                    .before(RenderSystems::Render),
                mark_after_render_graph
                    .after(RenderSystems::Render)
                    .before(RenderSystems::Cleanup),
                publish_render_spans.after(RenderSystems::PostCleanup),
            ),
        );
    }
}

fn mark_render_start(mut marks: ResMut<RenderMarks>, spans: Res<RenderThreadSpans>) {
    let now = Instant::now();
    if marks.completed {
        let wait_for_extract = duration_ms(marks.end, now);
        spans
            .0
            .wait_for_extract
            .store(wait_for_extract.to_bits(), Ordering::Relaxed);
    }
    marks.start = now;
    spans.0.store_offset(&spans.0.render_start_ms, now);
}

fn mark_before_assets(mut marks: ResMut<RenderMarks>) { marks.before_assets = Instant::now(); }

fn mark_after_assets(mut marks: ResMut<RenderMarks>) { marks.after_assets = Instant::now(); }

fn mark_before_views(mut marks: ResMut<RenderMarks>) { marks.before_views = Instant::now(); }

fn mark_after_views(mut marks: ResMut<RenderMarks>) { marks.after_views = Instant::now(); }

fn mark_before_render_graph(mut marks: ResMut<RenderMarks>) {
    marks.before_render_graph = Instant::now();
}

fn mark_after_render_graph(mut marks: ResMut<RenderMarks>) {
    marks.after_render_graph = Instant::now();
}

fn publish_render_spans(mut marks: ResMut<RenderMarks>, spans: Res<RenderThreadSpans>) {
    let end = Instant::now();
    let render = duration_ms(marks.start, end);
    let assets = duration_ms(marks.before_assets, marks.after_assets);
    let gpu_wait = duration_ms(marks.before_views, marks.after_views);
    let render_graph = duration_ms(marks.before_render_graph, marks.after_render_graph);
    let cleanup = duration_ms(marks.after_render_graph, end);
    let prep = (render - assets - gpu_wait - render_graph - cleanup).max(0.0);
    spans.0.render.store(render.to_bits(), Ordering::Relaxed);
    spans.0.assets.store(assets.to_bits(), Ordering::Relaxed);
    spans.0.prep.store(prep.to_bits(), Ordering::Relaxed);
    spans
        .0
        .gpu_wait
        .store(gpu_wait.to_bits(), Ordering::Relaxed);
    spans
        .0
        .render_graph
        .store(render_graph.to_bits(), Ordering::Relaxed);
    spans.0.cleanup.store(cleanup.to_bits(), Ordering::Relaxed);
    spans.0.store_offset(&spans.0.render_end_ms, end);
    marks.end = end;
    marks.completed = true;
}

/// One shared segment value, decoded from its `f32` bits.
fn span_ms(bits: &AtomicU32) -> f32 { f32::from_bits(bits.load(Ordering::Relaxed)) }

fn duration_ms(start: Instant, end: Instant) -> f32 {
    end.saturating_duration_since(start).as_secs_f32() * MILLISECONDS_PER_SECOND
}

fn timeline_duration_ms(start_ms: f32, end_ms: f32) -> Option<f32> {
    if has_timeline_mark(start_ms) && has_timeline_mark(end_ms) && end_ms >= start_ms {
        Some(end_ms - start_ms)
    } else {
        None
    }
}

fn relative_timeline_offset_ms(anchor_ms: f32, mark_ms: f32) -> f32 {
    if has_timeline_mark(anchor_ms) && has_timeline_mark(mark_ms) {
        (mark_ms - anchor_ms).max(0.0)
    } else {
        0.0
    }
}

fn has_timeline_mark(mark_ms: f32) -> bool { mark_ms.is_finite() && mark_ms > 0.0 }

// ── Measurement — GPU frame timer: timestamp brackets on the 3D pass ─────────

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

// ── Measurement — shadow draw counts: per-view shadow phase items ────────────

/// Latest per-frame phase-item counts, written on the render thread after
/// `PhaseSort` and read by the overlay on the main thread. Path-agnostic — it
/// counts whichever toggle state is active — so toggle-off vs toggle-on reads
/// come from one session.
#[derive(Default)]
struct DrawCountBits {
    /// Shadow-phase caster draws summed across shadow views (batchable +
    /// unbatchable bins, plus one per multidrawable batch set).
    shadow:          AtomicU32,
    /// Caster draws per shadow view (one view per cascade per shadow-casting
    /// light), largest first — the components that sum to `shadow`.
    shadow_per_view: Mutex<Vec<u32>>,
}

/// Main-world handle to the shared [`DrawCountBits`].
#[derive(Resource, Clone)]
struct DrawCounts(Arc<DrawCountBits>);

/// Counts per-view shadow phase items after `RenderSystems::PhaseSort` into
/// the shared [`DrawCountBits`] the upper-right overlay reads.
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

fn count_phase_items(shadow_phases: Res<ViewBinnedRenderPhases<Shadow>>, counts: Res<DrawCounts>) {
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
    shadow_per_view.sort_unstable_by(|a, b| b.cmp(a));

    let shadow: u32 = shadow_per_view.iter().sum();
    counts.0.shadow.store(shadow, Ordering::Relaxed);
    if let Ok(mut guard) = counts.0.shadow_per_view.lock() {
        *guard = shadow_per_view;
    }
}

// ── Overlay — perf table: status panel and upper-right batch stats ───────────

/// Label/value text size for header-style rows in the left perf panel.
const PERF_HEADER_ROW_FONT_SIZE: f32 = 13.0;
/// Label/value text size for normal rows in the left perf panel.
const PERF_ROW_FONT_SIZE: f32 = 11.0;
const STATUS_TEXT_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.9);
const STATUS_LABEL_COLOR: Color = Color::srgba(0.7, 0.78, 0.92, 0.85);
/// Separator color shared by screen-panel frames, camera-panel dividers, and
/// metric panel section rules.
const PANEL_SEPARATOR_COLOR: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
/// Separator line thickness in logical pixels.
const PANEL_SEPARATOR_THICKNESS: f32 = 1.0;
/// Wide enough to contain the longest timeline label plus the sub-row indent
/// without leaving a large gap before the numeric columns.
const LABEL_COLUMN_WIDTH: f32 = 144.0;
/// Wide enough to contain the `5s max` header on one line.
const VALUE_COLUMN_WIDTH: f32 = 72.0;
const TABLE_COL_GAP: f32 = 8.0;
const TABLE_ROW_GAP: f32 = 2.0;
/// Full status table width, used by section titles and separators.
const STATUS_TABLE_WIDTH: f32 = LABEL_COLUMN_WIDTH + VALUE_COLUMN_WIDTH * 2.0 + TABLE_COL_GAP * 2.0;
/// Left padding on component-row labels, marking what sums into the block
/// header above.
const SUB_ROW_INDENT: f32 = 12.0;
const FPS_UPDATE_INTERVAL: f32 = 1.0;
const PERF_PEAK_WINDOW_SECS: f32 = 5.0;
const MILLISECONDS_PER_SECOND: f32 = 1000.0;
const STATUS_TITLE_BAR_GAP: f32 = 1.0;
const STATUS_POSITION_EPSILON: f32 = 0.5;

/// Larger text on the label/value header line of each batch-stats group.
const STATS_HEADER_FONT_SIZE: f32 = 15.0;
/// Smaller text on the description line under each header.
const STATS_DESC_FONT_SIZE: f32 = 9.0;
/// Dim color for the description line.
const STATS_DESC_COLOR: Color = Color::srgba(0.60, 0.66, 0.76, 0.68);
/// Fixed width of each group: the label/value header spans it and the
/// description wraps within it. Wider than the diagnostics overlay so most
/// descriptions read on one line.
const STATS_ROW_WIDTH: f32 = 260.0;
/// Vertical gap between the header, description, and separator inside a group.
const STATS_INTRA_GAP: f32 = 2.0;
/// Vertical gap between groups.
const STATS_GROUP_GAP: f32 = 6.0;

/// Label hierarchy for the perf table.
#[derive(Clone, Copy)]
enum RowIndent {
    None,
    Phase,
    Detail,
}

impl RowIndent {
    const fn left_padding(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Phase => SUB_ROW_INDENT,
            Self::Detail => SUB_ROW_INDENT * 2.0,
        }
    }
}

#[derive(Clone, Copy)]
struct MetricRow {
    label:  &'static str,
    indent: RowIndent,
    accent: Option<Color>,
}

impl MetricRow {
    const fn new(label: &'static str, indent: RowIndent) -> Self {
        Self {
            label,
            indent,
            accent: None,
        }
    }

    const fn accented(label: &'static str, indent: RowIndent, accent: Color) -> Self {
        Self {
            label,
            indent,
            accent: Some(accent),
        }
    }
}

/// Diagnostic table rows, in display order — two additive blocks, one per
/// timeline.
///
/// Main thread: `ms` (frame wall time) = `layout` + `reconcile` + `shaping` +
/// `mesh` (the measured diegetic spans) + `other` (the rest of the main
/// schedules: cascade, transform propagation, every other system) +
/// `wait for render` (blocked in `renderer_extract` waiting for the render
/// thread to return the render app) + `extract` (main world → render world copy
/// and send-back) + `frame slack` (the outer app-frame residual not covered by
/// those measured spans).
///
/// Render lane, frame N: `render cycle` = `assets` (the `PrepareAssets` stage —
/// re-uploading every mesh / image / buffer asset modified this frame) +
/// `prep` (extract-commands apply, prepare meshes and views, specialize,
/// queue, sort, bind groups) + `wait for GPU` (the `PrepareViews` stage
/// containing the swapchain acquire — where the render thread blocks when the
/// GPU is behind) + `render graph` (render-graph execution: pass encoding,
/// submit, present) + `cleanup` (render cleanup and schedule closeout after
/// graph) + `return` (post-schedule app-return handoff and main `recv` unblock
/// gap) + `extract handoff` (main extracts into the render world and sends the
/// render app back, allowing the next render schedule to start).
///
/// [`RowIndent`] controls display hierarchy only; the metric values stay in
/// this fixed array order.
const METRIC_ROWS: [MetricRow; 18] = [
    MetricRow::new("fps", RowIndent::None),
    MetricRow::new("ms/frame", RowIndent::None),
    MetricRow::accented("layout", RowIndent::Detail, WATERFALL_WORK_COLOR),
    MetricRow::accented("reconcile", RowIndent::Detail, WATERFALL_WORK_COLOR),
    MetricRow::accented("shaping", RowIndent::Detail, WATERFALL_WORK_COLOR),
    MetricRow::accented("mesh", RowIndent::Detail, WATERFALL_WORK_COLOR),
    MetricRow::accented("other", RowIndent::Detail, WATERFALL_WORK_COLOR),
    MetricRow::accented("wait for render", RowIndent::Phase, WATERFALL_WAIT_COLOR),
    MetricRow::accented("extract", RowIndent::Phase, WATERFALL_EXTRACT_COLOR),
    MetricRow::accented("frame slack", RowIndent::Phase, WATERFALL_FRAME_SLACK_COLOR),
    MetricRow::new("render cycle", RowIndent::None),
    MetricRow::accented("assets", RowIndent::Detail, WATERFALL_WORK_COLOR),
    MetricRow::accented("prep", RowIndent::Detail, WATERFALL_WORK_COLOR),
    MetricRow::accented("wait for GPU", RowIndent::Phase, WATERFALL_WAIT_COLOR),
    MetricRow::accented(
        "render graph",
        RowIndent::Phase,
        WATERFALL_RENDER_GRAPH_COLOR,
    ),
    MetricRow::accented("cleanup", RowIndent::Phase, WATERFALL_CLEANUP_COLOR),
    MetricRow::accented("return", RowIndent::Phase, WATERFALL_RETURN_COLOR),
    MetricRow::accented(
        "extract handoff",
        RowIndent::Phase,
        WATERFALL_IDLE_LABEL_COLOR,
    ),
];
const METRIC_COUNT: usize = METRIC_ROWS.len();
const FPS_METRIC_INDEX: usize = 0;
const MAIN_WORLD_METRIC_START: usize = 1;
const RENDER_WORLD_METRIC_START: usize = 10;
const INITIAL_METRICS: [&str; METRIC_COUNT] = ["--"; METRIC_COUNT];

/// Marker for the screen-space diagnostic overlay panel.
#[derive(Component)]
struct StatusPanel;

/// Marker for the upper-right glyph-batch stats panel (Step-2 proof
/// counters), separate from the waterfall so its wide rows don't stretch it.
#[derive(Component)]
struct BatchStatsPanel;

#[derive(Clone, Copy)]
struct PerfSnapshot {
    timestamp:          f32,
    fps:                f32,
    frame_ms:           f32,
    layout_ms:          f32,
    reconcile_ms:       f32,
    shaping_ms:         f32,
    mesh_ms:            f32,
    other_ms:           f32,
    wait_for_render_ms: f32,
    extract_ms:         f32,
    frame_slack_ms:     f32,
    render_cycle_ms:    f32,
    assets_ms:          f32,
    prep_ms:            f32,
    wait_for_gpu_ms:    f32,
    render_graph_ms:    f32,
    cleanup_ms:         f32,
    return_ms:          f32,
    extract_handoff_ms: f32,
}

impl PerfSnapshot {
    const ZERO: Self = Self {
        timestamp:          0.0,
        fps:                0.0,
        frame_ms:           0.0,
        layout_ms:          0.0,
        reconcile_ms:       0.0,
        shaping_ms:         0.0,
        mesh_ms:            0.0,
        other_ms:           0.0,
        wait_for_render_ms: 0.0,
        extract_ms:         0.0,
        frame_slack_ms:     0.0,
        render_cycle_ms:    0.0,
        assets_ms:          0.0,
        prep_ms:            0.0,
        wait_for_gpu_ms:    0.0,
        render_graph_ms:    0.0,
        cleanup_ms:         0.0,
        return_ms:          0.0,
        extract_handoff_ms: 0.0,
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

fn spawn_status_overlay(mut commands: Commands) {
    let unlit = screen_panel_material();
    let built = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::TopLeft)
        .screen_position(0.0, 0.0)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_overlay_tree(
            &INITIAL_METRICS.map(String::from),
            &INITIAL_METRICS.map(String::from),
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

#[derive(Clone, Copy)]
struct ScreenPanelBounds {
    left:   f32,
    bottom: f32,
}

fn place_status_panel_below_title_bar(
    event: On<PanelDimensionsChanged>,
    windows: Query<&Window, With<PrimaryWindow>>,
    title_bars: Query<&DiegeticPanel, (With<TitleBar>, Without<StatusPanel>)>,
    mut status_panels: Query<&mut DiegeticPanel, With<StatusPanel>>,
) {
    let event = event.event();
    let Ok(title_panel) = title_bars.get(event.entity) else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };
    let window_size = Vec2::new(window.width(), window.height());
    if window_size.x <= 0.0 || window_size.y <= 0.0 {
        return;
    }

    let Some(title_bounds) = screen_panel_bounds(title_panel, event.dimensions, window_size) else {
        return;
    };
    let target = Vec2::new(
        title_bounds.left,
        title_bounds.bottom + STATUS_TITLE_BAR_GAP,
    );

    for mut status_panel in &mut status_panels {
        if screen_position_matches(&status_panel, target) {
            continue;
        }
        let _ = status_panel.set_screen_position(target);
    }
}

fn screen_panel_bounds(
    panel: &DiegeticPanel,
    dimensions: PanelDimensions,
    window_size: Vec2,
) -> Option<ScreenPanelBounds> {
    let CoordinateSpace::Screen { position, .. } = panel.coordinate_space() else {
        return None;
    };
    let width = dimensions.resolved_size.x;
    let height = dimensions.resolved_size.y;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    let (anchor_x, anchor_y) = panel.anchor().offset_fraction();
    let anchor_position = match *position {
        ScreenPosition::Screen => Vec2::new(anchor_x * window_size.x, anchor_y * window_size.y),
        ScreenPosition::At(position) => position,
    };
    let left = anchor_x.mul_add(-width, anchor_position.x);
    let top = anchor_y.mul_add(-height, anchor_position.y);
    Some(ScreenPanelBounds {
        left,
        bottom: top + height,
    })
}

fn screen_position_matches(panel: &DiegeticPanel, target: Vec2) -> bool {
    let CoordinateSpace::Screen {
        position: ScreenPosition::At(current),
        ..
    } = panel.coordinate_space()
    else {
        return false;
    };
    (*current - target).length_squared() <= STATUS_POSITION_EPSILON * STATUS_POSITION_EPSILON
}

fn status_label_style() -> TextStyle {
    TextStyle::new(PERF_ROW_FONT_SIZE)
        .with_color(STATUS_LABEL_COLOR)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn status_value_style() -> TextStyle {
    TextStyle::new(PERF_ROW_FONT_SIZE)
        .with_color(STATUS_TEXT_COLOR)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn status_label_style_with_color(color: Color) -> TextStyle {
    TextStyle::new(PERF_ROW_FONT_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn status_value_style_with_color(color: Color) -> TextStyle {
    TextStyle::new(PERF_ROW_FONT_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn perf_header_label_style() -> TextStyle {
    TextStyle::new(PERF_HEADER_ROW_FONT_SIZE)
        .with_color(STATUS_LABEL_COLOR)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn perf_header_value_style() -> TextStyle {
    TextStyle::new(PERF_HEADER_ROW_FONT_SIZE)
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
    Header,
}

fn label_cell_with_accent(
    builder: &mut LayoutBuilder,
    text: &str,
    indent: RowIndent,
    emphasis: CellEmphasis,
    accent: Option<Color>,
) {
    let style = match (emphasis, accent) {
        (CellEmphasis::Header, _) => perf_header_label_style(),
        (CellEmphasis::Normal, Some(color)) => status_label_style_with_color(color),
        (CellEmphasis::Dim | CellEmphasis::Normal, _) => status_label_style(),
    };
    builder.with(
        El::new()
            .width(Sizing::fixed(LABEL_COLUMN_WIDTH))
            .height(Sizing::FIT)
            .padding(Padding::new(indent.left_padding(), 0.0, 0.0, 0.0))
            .child_alignment(AlignX::Left, AlignY::Center),
        |builder| {
            builder.text(text, style);
        },
    );
}

fn value_cell_with_accent(
    builder: &mut LayoutBuilder,
    text: &str,
    emphasis: CellEmphasis,
    accent: Option<Color>,
) {
    let style = match (emphasis, accent) {
        (CellEmphasis::Dim, _) => status_label_style(),
        (CellEmphasis::Header, _) => perf_header_value_style(),
        (CellEmphasis::Normal, Some(color)) => status_value_style_with_color(color),
        (CellEmphasis::Normal, _) => status_value_style(),
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
    metric_row: MetricRow,
    now: &str,
    max: &str,
    emphasis: CellEmphasis,
) {
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(TABLE_COL_GAP)
            .child_alignment(AlignX::Left, AlignY::Center),
        |builder| {
            label_cell_with_accent(
                builder,
                metric_row.label,
                metric_row.indent,
                emphasis,
                metric_row.accent,
            );
            value_cell_with_accent(builder, now, emphasis, metric_row.accent);
            value_cell_with_accent(builder, max, emphasis, metric_row.accent);
        },
    );
}

fn table_subheader(
    builder: &mut LayoutBuilder,
    title: &str,
    indent: RowIndent,
    accent: Option<Color>,
) {
    builder.with(
        El::new()
            .width(Sizing::fixed(STATUS_TABLE_WIDTH))
            .height(Sizing::FIT)
            .child_alignment(AlignX::Left, AlignY::Center),
        |builder| {
            label_cell_with_accent(builder, title, indent, CellEmphasis::Normal, accent);
        },
    );
}

fn table_section_separator(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::fixed(STATUS_TABLE_WIDTH))
            .height(Sizing::fixed(PANEL_SEPARATOR_THICKNESS))
            .background(PANEL_SEPARATOR_COLOR),
        |_builder| {},
    );
}

fn table_section_title(builder: &mut LayoutBuilder, title: &str) {
    builder.with(
        El::new()
            .width(Sizing::fixed(STATUS_TABLE_WIDTH))
            .height(Sizing::FIT)
            .child_alignment(AlignX::Left, AlignY::Center),
        |builder| {
            builder.text(title, stats_header_label_style());
        },
    );
}

/// Builds the overlay: a `now` and a `5s max` column of right-aligned numerics,
/// with section titles separating the main-world and render-world rows.
fn build_overlay_tree(now: &[String; METRIC_COUNT], max: &[String; METRIC_COUNT]) -> LayoutTree {
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
                    table_row(
                        builder,
                        MetricRow::new("", RowIndent::None),
                        "now",
                        "5s max",
                        CellEmphasis::Dim,
                    );
                    table_row(
                        builder,
                        METRIC_ROWS[FPS_METRIC_INDEX],
                        &now[FPS_METRIC_INDEX],
                        &max[FPS_METRIC_INDEX],
                        CellEmphasis::Header,
                    );
                    table_section_separator(builder);
                    table_section_title(builder, "main world N+1");
                    table_row(
                        builder,
                        METRIC_ROWS[MAIN_WORLD_METRIC_START],
                        &now[MAIN_WORLD_METRIC_START],
                        &max[MAIN_WORLD_METRIC_START],
                        CellEmphasis::Normal,
                    );
                    table_subheader(
                        builder,
                        "work",
                        RowIndent::Phase,
                        Some(WATERFALL_WORK_COLOR),
                    );
                    for index in (MAIN_WORLD_METRIC_START + 1)..RENDER_WORLD_METRIC_START {
                        table_row(
                            builder,
                            METRIC_ROWS[index],
                            &now[index],
                            &max[index],
                            CellEmphasis::Normal,
                        );
                    }
                    table_section_separator(builder);
                    table_section_title(builder, "render world N");
                    table_row(
                        builder,
                        METRIC_ROWS[RENDER_WORLD_METRIC_START],
                        &now[RENDER_WORLD_METRIC_START],
                        &max[RENDER_WORLD_METRIC_START],
                        CellEmphasis::Normal,
                    );
                    table_subheader(
                        builder,
                        "work",
                        RowIndent::Phase,
                        Some(WATERFALL_WORK_COLOR),
                    );
                    for index in (RENDER_WORLD_METRIC_START + 1)..METRIC_COUNT {
                        table_row(
                            builder,
                            METRIC_ROWS[index],
                            &now[index],
                            &max[index],
                            CellEmphasis::Normal,
                        );
                    }
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
                        .height(Sizing::fixed(PANEL_SEPARATOR_THICKNESS))
                        .background(PANEL_SEPARATOR_COLOR),
                    |_builder| {},
                );
            }
        },
    );
}

fn update_status_panel(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
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
    // the real main-thread CPU everything else consumes. Outside the main
    // schedule, `wait for render` is the measured recv block and `extract` is
    // the measured main->render handoff. `frame slack` is the remaining
    // app-frame time not covered by those spans. Values are clamped because
    // spans are sampled one frame apart, so a hitch can momentarily invert
    // them.
    let main_span_ms = main_thread.0;
    let other_ms = (main_span_ms - layout_ms - reconcile_ms - shaping_ms - mesh_ms).max(0.0);
    let outside_main_ms = (frame_ms - main_span_ms).max(0.0);
    let wait_for_render_ms = span_ms(&render_spans.0.recv).clamp(0.0, outside_main_ms);
    let return_ms = span_ms(&render_spans.0.return_gap);
    let wait_for_extract_ms = span_ms(&render_spans.0.wait_for_extract);
    let extract_handoff_ms = (wait_for_extract_ms - return_ms).max(0.0);
    let extract_ms = extract_handoff_ms;
    let frame_slack_ms = (outside_main_ms - wait_for_render_ms - extract_ms).max(0.0);
    let render_cycle_ms = span_ms(&render_spans.0.render) + wait_for_extract_ms;
    let assets_ms = span_ms(&render_spans.0.assets);
    let prep_ms = span_ms(&render_spans.0.prep);
    let wait_for_gpu_ms = span_ms(&render_spans.0.gpu_wait);
    let render_graph_ms = span_ms(&render_spans.0.render_graph);
    let cleanup_ms = span_ms(&render_spans.0.cleanup);
    history.push_back(PerfSnapshot {
        timestamp: time.elapsed_secs(),
        fps: frames_per_second.unwrap_or(0.0).to_f32(),
        frame_ms,
        layout_ms,
        reconcile_ms,
        shaping_ms,
        mesh_ms,
        other_ms,
        wait_for_render_ms,
        extract_ms,
        frame_slack_ms,
        render_cycle_ms,
        assets_ms,
        prep_ms,
        wait_for_gpu_ms,
        render_graph_ms,
        cleanup_ms,
        return_ms,
        extract_handoff_ms,
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

    let now = format_perf_snapshot(mean);
    let max = format_perf_snapshot(peak);

    let key = format!("{}|{}", now.join(","), max.join(","));
    if key != last_displayed.text {
        last_displayed.text.clone_from(&key);
        for entity in &panels {
            commands.set_tree(entity, build_overlay_tree(&now, &max));
        }
    }
}

fn format_perf_snapshot(snapshot: PerfSnapshot) -> [String; METRIC_COUNT] {
    [
        format!("{:.0}", snapshot.fps),
        format!("{:.1}", snapshot.frame_ms),
        format!("{:.2}", snapshot.layout_ms),
        format!("{:.2}", snapshot.reconcile_ms),
        format!("{:.2}", snapshot.shaping_ms),
        format!("{:.2}", snapshot.mesh_ms),
        format!("{:.2}", snapshot.other_ms),
        format!("{:.2}", snapshot.wait_for_render_ms),
        format!("{:.2}", snapshot.extract_ms),
        format!("{:.2}", snapshot.frame_slack_ms),
        format!("{:.1}", snapshot.render_cycle_ms),
        format!("{:.2}", snapshot.assets_ms),
        format!("{:.2}", snapshot.prep_ms),
        format!("{:.2}", snapshot.wait_for_gpu_ms),
        format!("{:.2}", snapshot.render_graph_ms),
        format!("{:.2}", snapshot.cleanup_ms),
        format!("{:.2}", snapshot.return_ms),
        format!("{:.2}", snapshot.extract_handoff_ms),
    ]
}

/// The Step-2 proof-counter values shown in the upper-right panel.
#[derive(Default)]
struct BatchStatsValues {
    batches:         usize,
    runs:            usize,
    glyphs:          usize,
    shadow_items:    u32,
    /// Caster draws per shadow view (largest first).
    shadow_per_view: Vec<u32>,
}

/// One batch-stats group: label/value header plus zero or more detail lines.
struct BatchStatsRow {
    label:   &'static str,
    value:   String,
    details: Vec<String>,
}

/// Label, value, and detail lines for each batch-stats group. The `batches`
/// details enumerate the batch keys this scene routes to (fixed); the `shadow`
/// details are the live per-view breakdown that produced the number this frame,
/// read from the render phases.
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
            label:   "world text labels",
            value:   LABEL_COUNT.to_string(),
            details: Vec::new(),
        },
        BatchStatsRow {
            label:   "text draw batches",
            value:   values.batches.to_string(),
            details: vec![
                "1. world labels".to_string(),
                "2. screen UI overlays".to_string(),
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
            label:   "shadow",
            value:   values.shadow_items.to_string(),
            details: shadow_details(&values.shadow_per_view),
        },
    ]
}

/// The live breakdown behind the `shadow` number. This scene has one
/// shadow-casting key light and two casters: the text batch and the ground
/// plane.
fn shadow_details(per_view: &[u32]) -> Vec<String> {
    if per_view.is_empty() {
        return vec![
            "keylight shadow pass".to_string(),
            "casters: text batch & ground plane".to_string(),
            "no shadow cascades yet".to_string(),
        ];
    }
    let draws_per_cascade = per_view.first().copied().unwrap_or_default();
    vec![
        "keylight shadow pass".to_string(),
        "casters: text batch & ground plane".to_string(),
        format!(
            "{} cascades * {} draws (1 for each caster)",
            per_view.len(),
            draws_per_cascade
        ),
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
        batches:         batch.batches,
        runs:            batch.runs,
        glyphs:          batch.glyph_records,
        shadow_items:    draw_counts.0.shadow.load(Ordering::Relaxed),
        shadow_per_view: draw_counts
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
        sum.wait_for_render_ms += sample.wait_for_render_ms;
        sum.extract_ms += sample.extract_ms;
        sum.frame_slack_ms += sample.frame_slack_ms;
        sum.render_cycle_ms += sample.render_cycle_ms;
        sum.assets_ms += sample.assets_ms;
        sum.prep_ms += sample.prep_ms;
        sum.wait_for_gpu_ms += sample.wait_for_gpu_ms;
        sum.render_graph_ms += sample.render_graph_ms;
        sum.cleanup_ms += sample.cleanup_ms;
        sum.return_ms += sample.return_ms;
        sum.extract_handoff_ms += sample.extract_handoff_ms;
    }
    PerfSnapshot {
        timestamp:          0.0,
        fps:                sum.fps / count,
        frame_ms:           sum.frame_ms / count,
        layout_ms:          sum.layout_ms / count,
        reconcile_ms:       sum.reconcile_ms / count,
        shaping_ms:         sum.shaping_ms / count,
        mesh_ms:            sum.mesh_ms / count,
        other_ms:           sum.other_ms / count,
        wait_for_render_ms: sum.wait_for_render_ms / count,
        extract_ms:         sum.extract_ms / count,
        frame_slack_ms:     sum.frame_slack_ms / count,
        render_cycle_ms:    sum.render_cycle_ms / count,
        assets_ms:          sum.assets_ms / count,
        prep_ms:            sum.prep_ms / count,
        wait_for_gpu_ms:    sum.wait_for_gpu_ms / count,
        render_graph_ms:    sum.render_graph_ms / count,
        cleanup_ms:         sum.cleanup_ms / count,
        return_ms:          sum.return_ms / count,
        extract_handoff_ms: sum.extract_handoff_ms / count,
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
        peak.wait_for_render_ms = peak.wait_for_render_ms.max(sample.wait_for_render_ms);
        peak.extract_ms = peak.extract_ms.max(sample.extract_ms);
        peak.frame_slack_ms = peak.frame_slack_ms.max(sample.frame_slack_ms);
        peak.render_cycle_ms = peak.render_cycle_ms.max(sample.render_cycle_ms);
        peak.assets_ms = peak.assets_ms.max(sample.assets_ms);
        peak.prep_ms = peak.prep_ms.max(sample.prep_ms);
        peak.wait_for_gpu_ms = peak.wait_for_gpu_ms.max(sample.wait_for_gpu_ms);
        peak.render_graph_ms = peak.render_graph_ms.max(sample.render_graph_ms);
        peak.cleanup_ms = peak.cleanup_ms.max(sample.cleanup_ms);
        peak.return_ms = peak.return_ms.max(sample.return_ms);
        peak.extract_handoff_ms = peak.extract_handoff_ms.max(sample.extract_handoff_ms);
    }
    peak
}

// ── Overlay — waterfall meter: bottom main/render/GPU timeline lanes ─────────

// colors
/// GPU-busy blocks. The wide block finishes frame N (ends at present); the tail
/// block starts frame N+1 after the render graph submits. Width comes from the
/// CPU-clock present anchor, not the timestamp query — see [`gpu_lane_segments`].
const WATERFALL_GPU_COLOR: Color = Color::srgb(0.0, 0.92, 1.0);
/// Idle / parked spans: GPU idle gap and render-world extract handoff.
const WATERFALL_GAP_COLOR: Color = Color::srgb(0.72, 0.78, 0.82);
/// Main and render lane `work` segments.
const WATERFALL_WORK_COLOR: Color = Color::srgb(0.30, 0.60, 0.95);
/// Render world N `render graph` segment.
const WATERFALL_RENDER_GRAPH_COLOR: Color = Color::srgb(0.35, 0.85, 0.45);
/// Render world N post-graph cleanup work.
const WATERFALL_CLEANUP_COLOR: Color = Color::srgb(1.0, 0.22, 0.10);
/// Render world N return-app / main-unblock gap before extract starts.
const WATERFALL_RETURN_COLOR: Color = Color::srgb(0.95, 0.40, 0.56);
/// Track tint behind each lane — the empty tail past the drawn segments.
const WATERFALL_TRACK_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.06);
/// Shared between main world N+1 `wait for render` and render world N `wait for GPU`.
const WATERFALL_WAIT_COLOR: Color = Color::srgb(0.95, 0.65, 0.20);
/// Main lane `extract` — the main->render copy and send-back.
const WATERFALL_EXTRACT_COLOR: Color = Color::srgb(0.72, 0.54, 1.0);
/// Unclassified app-frame slack in the perf table; not measured work.
const WATERFALL_FRAME_SLACK_COLOR: Color = WATERFALL_GAP_COLOR;
/// Segment label color for dark backgrounds.
const WATERFALL_LIGHT_LABEL_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 1.0);
/// Perf-table color for rows whose waterfall segment uses [`WATERFALL_GAP_COLOR`].
const WATERFALL_IDLE_LABEL_COLOR: Color = WATERFALL_GAP_COLOR;
/// Segment label color for light backgrounds.
const WATERFALL_DARK_LABEL_COLOR: Color = Color::srgba(0.03, 0.04, 0.06, 1.0);
/// WCAG luminance weight for linear red.
const WATERFALL_LUMINANCE_RED_WEIGHT: f32 = 0.2126;
/// WCAG luminance weight for linear green.
const WATERFALL_LUMINANCE_GREEN_WEIGHT: f32 = 0.7152;
/// WCAG luminance weight for linear blue.
const WATERFALL_LUMINANCE_BLUE_WEIGHT: f32 = 0.0722;
/// sRGB breakpoint for converting to linear light.
const WATERFALL_SRGB_LINEAR_BREAKPOINT: f32 = 0.04045;
/// Linear divisor below [`WATERFALL_SRGB_LINEAR_BREAKPOINT`].
const WATERFALL_SRGB_LINEAR_DIVISOR: f32 = 12.92;
/// sRGB offset used by the IEC transfer function.
const WATERFALL_SRGB_TRANSFER_OFFSET: f32 = 0.055;
/// sRGB transfer-function divisor.
const WATERFALL_SRGB_TRANSFER_DIVISOR: f32 = 1.055;
/// sRGB transfer-function exponent.
const WATERFALL_SRGB_TRANSFER_EXPONENT: f32 = 2.4;
/// WCAG contrast-ratio luminance offset.
const WATERFALL_CONTRAST_LUMINANCE_OFFSET: f32 = 0.05;

// layout
/// Gap between the label column and the bar column in panel pixels.
const WATERFALL_LABEL_GAP: f32 = 6.0;
/// Vertical gap between lanes in panel pixels.
const WATERFALL_LANE_GAP: f32 = 4.0;
/// Lane bar thickness in panel pixels.
const WATERFALL_LANE_HEIGHT: f32 = 14.0;
/// Lane label text size in panel pixels.
const WATERFALL_LANE_LABEL_FONT_SIZE: f32 = 13.0;
/// Lane row height in panel pixels — the label cell and the bar wrapper share
/// it so the two columns align; tall enough not to clip the label text.
const WATERFALL_LANE_ROW_HEIGHT: f32 = 20.0;
/// Axis floor (ms) — keeps the bar scale finite before the first frame samples
/// and on a stall.
const WATERFALL_MIN_AXIS_MS: f32 = 1.0;
/// Timeline spacers thinner than this track fraction are dropped.
const WATERFALL_MIN_SEGMENT_FRACTION: f32 = 0.001;
/// Screen-width fraction occupied by the waterfall panel.
const WATERFALL_PANEL_WIDTH_FRACTION: f32 = 0.8;
/// Segment label font size in panel pixels.
const WATERFALL_SEGMENT_LABEL_FONT_SIZE: f32 = STATS_DESC_FONT_SIZE;
/// Edge padding for leading- and trailing-aligned segment labels.
const WATERFALL_SEGMENT_EDGE_LABEL_PADDING: f32 = 6.0;

// timing
/// How long each on-screen picture is sampled before it refreshes (seconds).
/// Per-frame values are averaged over this window so the held picture is the
/// second's mean, not one noisy frame.
const WATERFALL_SAMPLE_PERIOD: f32 = 1.0;
/// How long the bars take to slide from the old picture to the new one at each
/// refresh (seconds). After the morph the picture holds for the rest of the
/// sample period (≈800 ms), giving a still frame to read.
const WATERFALL_MORPH_DURATION: f32 = 0.2;

/// Marker for the bottom-left waterfall bar panel.
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
/// [`WATERFALL_MORPH_DURATION`]. The lane values are converted into labeled
/// [`TimelineSegment`]s when the tree is rebuilt.
#[derive(Clone, Copy, Default)]
struct WaterfallBars {
    /// Shared window width in milliseconds, anchored at the render start mark.
    axis:             f32,
    /// Main-lane start offset relative to the render frame's start mark.
    main_offset:      f32,
    main_work:        f32,
    prep:             f32,
    gpu_wait:         f32,
    render_graph:     f32,
    cleanup:          f32,
    /// Gap from render-schedule publication to main extract begin.
    return_gap:       f32,
    /// Render lane's right-side parked span, measured from the previous render
    /// schedule end to the next render start.
    wait_for_extract: f32,
    /// Measured GPU time of the `OrbitCam` opaque `MainPass` only (`GpuFrameMs`) — a
    /// sliver of the frame's GPU work (it excludes the prepass, the OIT resolve,
    /// and every overlay camera), so it does NOT drive the GPU lane. The lane is
    /// built from the present anchor instead; this is kept for a future
    /// "main-pass GPU" sub-readout.
    gpu:              f32,
}

impl WaterfallBars {
    /// Field-wise sum, for accumulating the per-second mean.
    fn add(self, other: Self) -> Self {
        Self {
            axis:             self.axis + other.axis,
            main_offset:      self.main_offset + other.main_offset,
            main_work:        self.main_work + other.main_work,
            prep:             self.prep + other.prep,
            gpu_wait:         self.gpu_wait + other.gpu_wait,
            render_graph:     self.render_graph + other.render_graph,
            cleanup:          self.cleanup + other.cleanup,
            return_gap:       self.return_gap + other.return_gap,
            wait_for_extract: self.wait_for_extract + other.wait_for_extract,
            gpu:              self.gpu + other.gpu,
        }
    }

    /// Field-wise scale, dividing the accumulated sum by the sample count.
    fn scale(self, factor: f32) -> Self {
        Self {
            axis:             self.axis * factor,
            main_offset:      self.main_offset * factor,
            main_work:        self.main_work * factor,
            prep:             self.prep * factor,
            gpu_wait:         self.gpu_wait * factor,
            render_graph:     self.render_graph * factor,
            cleanup:          self.cleanup * factor,
            return_gap:       self.return_gap * factor,
            wait_for_extract: self.wait_for_extract * factor,
            gpu:              self.gpu * factor,
        }
    }

    /// Field-wise lerp from `self` toward `to` by fraction `t`, for the morph.
    const fn lerp(self, to: Self, t: f32) -> Self {
        Self {
            axis:             lerp(self.axis, to.axis, t),
            main_offset:      lerp(self.main_offset, to.main_offset, t),
            main_work:        lerp(self.main_work, to.main_work, t),
            prep:             lerp(self.prep, to.prep, t),
            gpu_wait:         lerp(self.gpu_wait, to.gpu_wait, t),
            render_graph:     lerp(self.render_graph, to.render_graph, t),
            cleanup:          lerp(self.cleanup, to.cleanup, t),
            return_gap:       lerp(self.return_gap, to.return_gap, t),
            wait_for_extract: lerp(self.wait_for_extract, to.wait_for_extract, t),
            gpu:              lerp(self.gpu, to.gpu, t),
        }
    }

    fn render_end(self) -> f32 { self.prep + self.gpu_wait + self.render_graph + self.cleanup }

    fn extract_begin(self) -> f32 { self.render_end() + self.return_gap }

    fn main_extract(self) -> f32 { (self.wait_for_extract - self.return_gap).max(0.0) }
}

/// Label placement inside a waterfall segment.
#[derive(Clone, Copy)]
enum SegmentLabelAlignment {
    Center,
    Leading,
    Trailing,
}

impl SegmentLabelAlignment {
    const fn align_x(self) -> AlignX {
        match self {
            Self::Center => AlignX::Center,
            Self::Leading => AlignX::Left,
            Self::Trailing => AlignX::Right,
        }
    }

    const fn text_align(self) -> TextAlign {
        match self {
            Self::Center => TextAlign::Center,
            Self::Leading => TextAlign::Left,
            Self::Trailing => TextAlign::Right,
        }
    }

    fn padding(self) -> Padding {
        match self {
            Self::Center => Padding::default(),
            Self::Leading => Padding::new(WATERFALL_SEGMENT_EDGE_LABEL_PADDING, 0.0, 0.0, 0.0),
            Self::Trailing => Padding::new(0.0, WATERFALL_SEGMENT_EDGE_LABEL_PADDING, 0.0, 0.0),
        }
    }

    const fn label_cell(self, start_fraction: f32, end_fraction: f32) -> (f32, f32) {
        match self {
            Self::Center => (start_fraction, end_fraction - start_fraction),
            Self::Leading => (start_fraction, 1.0 - start_fraction),
            Self::Trailing => (0.0, end_fraction),
        }
    }
}

/// A lane segment on the shared timeline, in milliseconds relative to the
/// render frame's start mark.
#[derive(Clone, Copy)]
struct TimelineSegment {
    start:     f32,
    end:       f32,
    color:     Color,
    label:     &'static str,
    alignment: SegmentLabelAlignment,
}

impl TimelineSegment {
    const fn measured(start: f32, end: f32, color: Color, label: &'static str) -> Self {
        Self {
            start,
            end,
            color,
            label,
            alignment: SegmentLabelAlignment::Center,
        }
    }

    const fn inferred(start: f32, end: f32, color: Color, label: &'static str) -> Self {
        Self {
            start,
            end,
            color,
            label,
            alignment: SegmentLabelAlignment::Center,
        }
    }

    const fn with_label_alignment(mut self, alignment: SegmentLabelAlignment) -> Self {
        self.alignment = alignment;
        self
    }
}

fn spawn_waterfall_overlay(mut commands: Commands) {
    let unlit = screen_panel_material();
    let built = DiegeticPanel::screen()
        .size(Percent(WATERFALL_PANEL_WIDTH_FRACTION), Fit)
        .anchor(Anchor::BottomLeft)
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
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::GROW).height(Sizing::FIT));
    screen_panel_frame(
        &mut builder,
        Sizing::GROW,
        Sizing::FIT,
        DEFAULT_PANEL_BACKGROUND,
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::GROW)
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
                            lane_label(builder, "main world");
                            lane_label(builder, "render world");
                            lane_label_with_color(builder, "GPU", WATERFALL_GPU_COLOR);
                        },
                    );
                    builder.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .direction(Direction::TopToBottom)
                            .child_gap(WATERFALL_LANE_GAP),
                        |builder| {
                            let span = bars.axis.max(WATERFALL_MIN_AXIS_MS);
                            lane_bars(builder, span, &main_lane_segments(bars));
                            lane_bars(builder, span, &render_lane_segments(bars));
                            lane_bars(builder, span, &gpu_lane_segments(bars));
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
    lane_label_with_color(builder, label, STATUS_LABEL_COLOR);
}

fn lane_label_with_color(builder: &mut LayoutBuilder, label: &str, color: Color) {
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::fixed(WATERFALL_LANE_ROW_HEIGHT))
            .child_alignment(AlignX::Right, AlignY::Center),
        |builder| {
            builder.text(label, waterfall_lane_label_style(color));
        },
    );
}

fn waterfall_lane_label_style(color: Color) -> TextStyle {
    TextStyle::new(WATERFALL_LANE_LABEL_FONT_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}

/// One lane's bar track: a grow-width track tinted [`WATERFALL_TRACK_COLOR`]
/// with fractional-offset [`TimelineSegment`] blocks. Empty timeline spans
/// become transparent spacers, so reading straight down at one x compares the
/// same process-monotonic time on every lane.
fn lane_bars(builder: &mut LayoutBuilder, axis_ms: f32, segments: &[TimelineSegment]) {
    let axis = axis_ms.max(WATERFALL_MIN_AXIS_MS);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(WATERFALL_LANE_ROW_HEIGHT))
            .direction(Direction::LeftToRight)
            .child_alignment(AlignX::Left, AlignY::Center),
        |builder| {
            waterfall_track(builder, axis, segments);
        },
    );
}

fn waterfall_track(builder: &mut LayoutBuilder, axis: f32, segments: &[TimelineSegment]) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(WATERFALL_LANE_HEIGHT))
            .direction(Direction::TopToBottom)
            .child_gap(-WATERFALL_LANE_HEIGHT)
            .background(WATERFALL_TRACK_COLOR),
        |builder| {
            waterfall_segment_row(builder, axis, segments);
            for segment in segments {
                waterfall_label_row(builder, axis, *segment);
            }
        },
    );
}

fn waterfall_segment_row(builder: &mut LayoutBuilder, axis: f32, segments: &[TimelineSegment]) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(WATERFALL_LANE_HEIGHT))
            .direction(Direction::LeftToRight),
        |builder| {
            let mut used = 0.0;
            for segment in segments {
                let start_fraction = (segment.start / axis).clamp(0.0, 1.0);
                let end_fraction = (segment.end / axis).clamp(start_fraction, 1.0);
                add_waterfall_spacer(builder, start_fraction - used);
                used = used.max(start_fraction);

                let fraction = end_fraction - used;
                add_waterfall_segment(builder, fraction, *segment);
                used = used.max(end_fraction);
            }
        },
    );
}

fn add_waterfall_spacer(builder: &mut LayoutBuilder, fraction: f32) {
    if fraction < WATERFALL_MIN_SEGMENT_FRACTION {
        return;
    }
    builder.with(
        El::new()
            .width(Sizing::percent(fraction))
            .height(Sizing::fixed(WATERFALL_LANE_HEIGHT)),
        |_builder| {},
    );
}

fn add_waterfall_segment(builder: &mut LayoutBuilder, fraction: f32, segment: TimelineSegment) {
    let cell = El::new()
        .width(Sizing::percent(fraction))
        .height(Sizing::fixed(WATERFALL_LANE_HEIGHT))
        .padding(segment.alignment.padding())
        .child_alignment(segment.alignment.align_x(), AlignY::Center);
    let cell = if segment.color.alpha() > 0.0 {
        cell.background(segment.color)
    } else {
        cell
    };
    builder.with(cell, |_builder| {});
}

fn waterfall_label_row(builder: &mut LayoutBuilder, axis: f32, segment: TimelineSegment) {
    if segment.label.is_empty() {
        return;
    }
    let start_fraction = (segment.start / axis).clamp(0.0, 1.0);
    let end_fraction = (segment.end / axis).clamp(start_fraction, 1.0);
    let (spacer_fraction, label_fraction) =
        segment.alignment.label_cell(start_fraction, end_fraction);
    if label_fraction <= 0.0 {
        return;
    }
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(WATERFALL_LANE_HEIGHT))
            .direction(Direction::LeftToRight)
            .child_alignment(AlignX::Left, AlignY::Center),
        |builder| {
            add_waterfall_position_spacer(builder, spacer_fraction);
            builder.with(
                El::new()
                    .width(Sizing::percent(label_fraction))
                    .height(Sizing::fixed(WATERFALL_LANE_HEIGHT))
                    .padding(segment.alignment.padding())
                    .child_alignment(segment.alignment.align_x(), AlignY::Center),
                |builder| {
                    builder.text(segment.label, waterfall_segment_label_style(segment));
                },
            );
        },
    );
}

fn add_waterfall_position_spacer(builder: &mut LayoutBuilder, fraction: f32) {
    if fraction <= 0.0 {
        return;
    }
    builder.with(
        El::new()
            .width(Sizing::percent(fraction))
            .height(Sizing::fixed(WATERFALL_LANE_HEIGHT)),
        |_builder| {},
    );
}

fn waterfall_segment_label_style(segment: TimelineSegment) -> TextStyle {
    TextStyle::new(WATERFALL_SEGMENT_LABEL_FONT_SIZE)
        .bold()
        .with_color(waterfall_segment_label_color(segment))
        .with_align(segment.alignment.text_align())
        .with_shadow_mode(GlyphShadowMode::None)
        .no_wrap()
}

fn waterfall_segment_label_color(segment: TimelineSegment) -> Color {
    if segment.color.alpha() <= 0.0 {
        return WATERFALL_LIGHT_LABEL_COLOR;
    }
    let luminance = waterfall_relative_luminance(segment.color);
    let light_contrast = waterfall_contrast_ratio(
        luminance,
        waterfall_relative_luminance(WATERFALL_LIGHT_LABEL_COLOR),
    );
    let dark_contrast = waterfall_contrast_ratio(
        luminance,
        waterfall_relative_luminance(WATERFALL_DARK_LABEL_COLOR),
    );
    if dark_contrast > light_contrast {
        WATERFALL_DARK_LABEL_COLOR
    } else {
        WATERFALL_LIGHT_LABEL_COLOR
    }
}

fn waterfall_contrast_ratio(background_luminance: f32, label_luminance: f32) -> f32 {
    let lighter = background_luminance.max(label_luminance);
    let darker = background_luminance.min(label_luminance);
    (lighter + WATERFALL_CONTRAST_LUMINANCE_OFFSET) / (darker + WATERFALL_CONTRAST_LUMINANCE_OFFSET)
}

fn waterfall_relative_luminance(color: Color) -> f32 {
    let color = color.to_srgba();
    waterfall_linear_srgb(color.red).mul_add(
        WATERFALL_LUMINANCE_RED_WEIGHT,
        waterfall_linear_srgb(color.green).mul_add(
            WATERFALL_LUMINANCE_GREEN_WEIGHT,
            waterfall_linear_srgb(color.blue) * WATERFALL_LUMINANCE_BLUE_WEIGHT,
        ),
    )
}

fn waterfall_linear_srgb(channel: f32) -> f32 {
    if channel <= WATERFALL_SRGB_LINEAR_BREAKPOINT {
        channel / WATERFALL_SRGB_LINEAR_DIVISOR
    } else {
        ((channel + WATERFALL_SRGB_TRANSFER_OFFSET) / WATERFALL_SRGB_TRANSFER_DIVISOR)
            .powf(WATERFALL_SRGB_TRANSFER_EXPONENT)
    }
}

/// Main lane, frame N+1 on the shared timeline: `work` (main schedule span) ·
/// `wait` (the measured `recv` block — main blocked until the render thread
/// returns the previous frame's app) · `extract` (the main→render copy and
/// send-back before the next render schedule begins).
fn main_lane_segments(b: &WaterfallBars) -> Vec<TimelineSegment> {
    let period = b.axis.max(WATERFALL_MIN_AXIS_MS);
    let extract_start = b.extract_begin().clamp(0.0, period);
    let work_start = b.main_offset.clamp(0.0, extract_start);
    let work_end = (work_start + b.main_work).clamp(work_start, extract_start);
    let extract_end = (extract_start + b.main_extract()).clamp(extract_start, period);
    vec![
        TimelineSegment::measured(work_start, work_end, WATERFALL_WORK_COLOR, "work N+1")
            .with_label_alignment(SegmentLabelAlignment::Leading),
        TimelineSegment::measured(
            work_end,
            extract_start,
            WATERFALL_WAIT_COLOR,
            "wait for render",
        ),
        TimelineSegment::measured(
            extract_start,
            extract_end,
            WATERFALL_EXTRACT_COLOR,
            "extract",
        )
        .with_label_alignment(SegmentLabelAlignment::Trailing),
    ]
}

/// Render lane, frame N: `work` (`assets` + prepare) · `wait for GPU`
/// (swapchain acquire — the stall waiting on the GPU below) · `render graph` ·
/// cleanup block · return block · extract-handoff block. The short tail blocks
/// are unlabeled in the waterfall; their names and colors live in the perf
/// panel rows.
fn render_lane_segments(b: &WaterfallBars) -> Vec<TimelineSegment> {
    let period = b.axis.max(WATERFALL_MIN_AXIS_MS);
    let prep_end = b.prep.clamp(0.0, period);
    let wait_end = (prep_end + b.gpu_wait).clamp(prep_end, period);
    let graph_end = (wait_end + b.render_graph).clamp(wait_end, period);
    let cleanup_end = (graph_end + b.cleanup).clamp(graph_end, period);
    let return_end = b.extract_begin().clamp(cleanup_end, period);
    let extract_end = (return_end + b.main_extract()).clamp(return_end, period);
    vec![
        TimelineSegment::measured(0.0, prep_end, WATERFALL_WORK_COLOR, "work N")
            .with_label_alignment(SegmentLabelAlignment::Leading),
        TimelineSegment::measured(prep_end, wait_end, WATERFALL_WAIT_COLOR, "wait for GPU"),
        TimelineSegment::measured(
            wait_end,
            graph_end,
            WATERFALL_RENDER_GRAPH_COLOR,
            "render graph",
        ),
        TimelineSegment::measured(graph_end, cleanup_end, WATERFALL_CLEANUP_COLOR, ""),
        TimelineSegment::measured(cleanup_end, return_end, WATERFALL_RETURN_COLOR, ""),
        TimelineSegment::measured(return_end, extract_end, WATERFALL_GAP_COLOR, ""),
    ]
}

/// GPU lane, one frame: `current` (busy, ending the instant the render lane's
/// `wait for GPU` releases) · idle render-graph gap · busy `next` starting
/// the next frame. The current block's right edge lines up under the render lane's
/// `wait for GPU` → `render graph` boundary — that vertical line is the GPU
/// making the render thread wait.
fn gpu_lane_segments(b: &WaterfallBars) -> Vec<TimelineSegment> {
    let period = b.axis.max(WATERFALL_MIN_AXIS_MS);
    let current_end = (b.prep + b.gpu_wait).clamp(0.0, period);
    let idle_end = (current_end + b.render_graph).clamp(current_end, period);
    vec![
        TimelineSegment::inferred(0.0, current_end, WATERFALL_GPU_COLOR, "current")
            .with_label_alignment(SegmentLabelAlignment::Leading),
        TimelineSegment::measured(current_end, idle_end, WATERFALL_GAP_COLOR, "idle"),
        TimelineSegment::inferred(idle_end, period, WATERFALL_GPU_COLOR, "next")
            .with_label_alignment(SegmentLabelAlignment::Trailing),
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
/// for the rest of the second. Main and render marks are stored as offsets from
/// one shared epoch, then drawn relative to the render frame's start mark.
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
    let render_start_ms = span_ms(&render_spans.0.render_start_ms);
    let main_start_ms = span_ms(&render_spans.0.main_start_ms);
    let main_end_ms = span_ms(&render_spans.0.main_end_ms);
    let main_span_ms = timeline_duration_ms(main_start_ms, main_end_ms).unwrap_or(main_thread.0);
    let prep = span_ms(&render_spans.0.assets) + span_ms(&render_spans.0.prep);
    let gpu_wait = span_ms(&render_spans.0.gpu_wait);
    let render_graph = span_ms(&render_spans.0.render_graph);
    let cleanup = span_ms(&render_spans.0.cleanup);
    let return_gap = span_ms(&render_spans.0.return_gap);
    let render = span_ms(&render_spans.0.render);
    let wait_for_extract = span_ms(&render_spans.0.wait_for_extract);
    let main_offset = relative_timeline_offset_ms(render_start_ms, main_start_ms);
    let render_period = render + wait_for_extract;
    let instant = WaterfallBars {
        axis: render_period,
        main_offset,
        main_work: main_span_ms,
        prep,
        gpu_wait,
        render_graph,
        cleanup,
        return_gap,
        wait_for_extract,
        gpu: span_ms(&gpu_ms.0),
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
