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
//!   Space — pause / resume per-frame mutation (compare moving vs idle cost)
//!
//! A bottom-left screen overlay reports fps, frame ms, and the `layout` /
//! `shaping` / `mesh` timings from `DiegeticPerfStats` plus a `remainder` row
//! (frame time minus those three), each with a 5-second peak column.

use std::collections::VecDeque;

use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
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
use bevy_diegetic::Sizing;
use bevy_diegetic::TextStyle;
use bevy_kana::ToF32;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::TitleBar;
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

// ── Overlay (screen-space) constants ──────────────────────────────────────────

const OVERLAY_FONT_SIZE: f32 = 13.0;
const STATUS_TEXT_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.9);
const STATUS_LABEL_COLOR: Color = Color::srgba(0.7, 0.78, 0.92, 0.85);
/// Wide enough to contain the longest label (`remainder`) without colliding
/// into the value columns.
const LABEL_COLUMN_WIDTH: f32 = 92.0;
/// Wide enough to contain the `5s max` header on one line.
const VALUE_COLUMN_WIDTH: f32 = 72.0;
const TABLE_COL_GAP: f32 = 8.0;
const TABLE_ROW_GAP: f32 = 2.0;
const FPS_UPDATE_INTERVAL: f32 = 1.0;
const PERF_PEAK_WINDOW_SECS: f32 = 5.0;

/// Diagnostic table rows, in display order. `remainder` = `ms` − (`layout` +
/// `shaping` + `mesh`): the per-frame time not covered by the diegetic CPU rows
/// above (reconcile, render, present, and every other system).
const METRIC_ROWS: [&str; 6] = ["fps", "ms", "layout", "shaping", "mesh", "remainder"];
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

/// Monotonic frame counter driving the per-label text; wrapped to three digits so
/// label widths stay bounded.
#[derive(Resource, Default)]
struct FrameCounter(u64);

/// Whether the per-frame mutation is running. Toggled with Space.
#[derive(Resource)]
struct Mutating(bool);

impl Default for Mutating {
    fn default() -> Self { Self(true) }
}

#[derive(Clone, Copy)]
struct PerfSnapshot {
    timestamp:    f32,
    fps:          f32,
    frame_ms:     f32,
    layout_ms:    f32,
    shaping_ms:   f32,
    mesh_ms:      f32,
    remainder_ms: f32,
}

impl PerfSnapshot {
    const ZERO: Self = Self {
        timestamp:    0.0,
        fps:          0.0,
        frame_ms:     0.0,
        layout_ms:    0.0,
        shaping_ms:   0.0,
        mesh_ms:      0.0,
        remainder_ms: 0.0,
    };
}

/// Last displayed overlay string, so the overlay tree is rebuilt only when a
/// shown value changes.
#[derive(Resource, Default)]
struct LastDisplayedStatus {
    text: String,
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
                .control("Space pause"),
        )
        .with_camera_control_panel()
        .init_resource::<FrameCounter>()
        .init_resource::<Mutating>()
        .init_resource::<LastDisplayedStatus>()
        .add_systems(Startup, (spawn_labels, spawn_status_overlay))
        .add_systems(
            Update,
            (
                toggle_mutation,
                advance_frame,
                mutate_labels,
                update_status_panel,
            )
                .chain(),
        )
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

fn label_cell(builder: &mut LayoutBuilder, text: &str) {
    builder.with(
        El::new()
            .width(Sizing::fixed(LABEL_COLUMN_WIDTH))
            .height(Sizing::FIT)
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
    label: &str,
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
            label_cell(builder, label);
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
                    table_row(builder, "", "now", "5s max", CellEmphasis::Dim);
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
                        "labels",
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

fn update_status_panel(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    mutating: Res<Mutating>,
    diegetic_perf: Res<DiegeticPerfStats>,
    panels: Query<Entity, With<StatusPanel>>,
    mut last_displayed: ResMut<LastDisplayedStatus>,
    mut commands: Commands,
    mut timer: Local<Option<Timer>>,
    mut history: Local<VecDeque<PerfSnapshot>>,
) {
    let frames_per_second = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(bevy::diagnostic::Diagnostic::smoothed);
    let frame_milliseconds = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(bevy::diagnostic::Diagnostic::smoothed);

    // Sample every frame so the smoothed `now` mean and the 5-second peak see
    // every value, not one arbitrary frame per second. The diegetic rows read
    // the per-frame `DiegeticPerfStats` resource.
    let frame_ms = frame_milliseconds.unwrap_or(0.0).to_f32();
    let layout_ms = diegetic_perf.compute_ms;
    let shaping_ms = diegetic_perf.panel_text.shape_ms;
    let mesh_ms = diegetic_perf.panel_text.mesh_build_ms;
    // remainder = the per-frame time not covered by the measured diegetic CPU
    // rows (reconcile, render, present, other systems). Clamped at zero: the
    // smoothed frame time can momentarily sit below the instantaneous diegetic
    // sum on a spike.
    let remainder_ms = (frame_ms - layout_ms - shaping_ms - mesh_ms).max(0.0);
    history.push_back(PerfSnapshot {
        timestamp: time.elapsed_secs(),
        fps: frames_per_second.unwrap_or(0.0).to_f32(),
        frame_ms,
        layout_ms,
        shaping_ms,
        mesh_ms,
        remainder_ms,
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

    // Every row's `now` is the window mean, so the column is internally
    // consistent: `ms` = `layout` + `shaping` + `mesh` + `remainder`.
    let now = [
        format!("{:.0}", mean.fps),
        format!("{:.1}", mean.frame_ms),
        format!("{:.2}", mean.layout_ms),
        format!("{:.2}", mean.shaping_ms),
        format!("{:.2}", mean.mesh_ms),
        format!("{:.2}", mean.remainder_ms),
    ];
    let max = [
        format!("{:.0}", peak.fps),
        format!("{:.1}", peak.frame_ms),
        format!("{:.2}", peak.layout_ms),
        format!("{:.2}", peak.shaping_ms),
        format!("{:.2}", peak.mesh_ms),
        format!("{:.2}", peak.remainder_ms),
    ];

    let key = format!("{}|{}|{}", now.join(","), max.join(","), mutating.0);
    if key != last_displayed.text {
        last_displayed.text.clone_from(&key);
        for entity in &panels {
            commands.set_tree(entity, build_overlay_tree(&now, &max, mutating.0));
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
        sum.shaping_ms += sample.shaping_ms;
        sum.mesh_ms += sample.mesh_ms;
        sum.remainder_ms += sample.remainder_ms;
    }
    PerfSnapshot {
        timestamp:    0.0,
        fps:          sum.fps / count,
        frame_ms:     sum.frame_ms / count,
        layout_ms:    sum.layout_ms / count,
        shaping_ms:   sum.shaping_ms / count,
        mesh_ms:      sum.mesh_ms / count,
        remainder_ms: sum.remainder_ms / count,
    }
}

/// Per-metric peak across the sample window — the `5s max` column.
fn window_peak(history: &VecDeque<PerfSnapshot>) -> PerfSnapshot {
    let mut peak = PerfSnapshot::ZERO;
    for sample in history {
        peak.fps = peak.fps.max(sample.fps);
        peak.frame_ms = peak.frame_ms.max(sample.frame_ms);
        peak.layout_ms = peak.layout_ms.max(sample.layout_ms);
        peak.shaping_ms = peak.shaping_ms.max(sample.shaping_ms);
        peak.mesh_ms = peak.mesh_ms.max(sample.mesh_ms);
        peak.remainder_ms = peak.remainder_ms.max(sample.remainder_ms);
    }
    peak
}
