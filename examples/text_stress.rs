//! Text stress test — add/remove rows to measure per-element rendering cost.
//!
//! Press '+' to add rows, '-' to remove (hold for accelerating repeat).
//! Performance stats shown via `DiegeticPanel` overlays.
//!
//! Rows fill columns left-to-right within a panel. When the panel reaches
//! screen width, it pushes backward and a new panel spawns in front.
//!
//! ## `hue_offset` demo
//!
//! When idle (1 second after releasing keys), the rainbow color scheme
//! scrolls across all panels using [`DiegeticPanel::hue_offset`]. This is
//! a GPU-side effect — the shader rotates all vertex colors uniformly,
//! so the animation has zero CPU cost (no tree rebuilds, no mesh changes).
//! This is a niche feature for animating color schemes without touching
//! layout or mesh data.

use std::collections::VecDeque;
use std::time::Instant;

use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPerfStats;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::HueOffset;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::Unit;
use bevy_kana::ToF32;
use bevy_kana::ToUsize;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::TrackpadInput;

// ── Text / layout constants (meters) ─────────────────────────────────────────

/// Font size for content panel text (in millimeters, matched to `font_unit`).
const FONT_SIZE: f32 = 2.1;
const ROW_HEIGHT: f32 = 0.07;
const ROW_GAP: f32 = 0.05;
const COLUMN_GAP: f32 = 0.05;
const HEADER_HEIGHT: f32 = ROW_HEIGHT + 0.01;

/// Column width in meters.
///
/// This is an explicit layout constraint, not just an estimate. Panel width is
/// budgeted from this value via `MAX_LAYOUT_WIDTH`, and each column is sized to
/// this width in the layout tree.
const COLUMN_WIDTH: f32 = 1.0;
/// Layout height per panel (meters).
const LAYOUT_HEIGHT: f32 = 2.0;
/// Padding on the outer panel in meters.
const PANEL_PADDING: f32 = 0.06;

// ── Scene constants ──────────────────────────────────────────────────────────

/// How many columns per panel.
const MAX_COLUMNS: usize = 8;
/// Max layout width — exactly fits `MAX_COLUMNS` with gaps and padding (meters).
#[allow(
    clippy::cast_precision_loss,
    reason = "const context — trait methods cannot be called in const expressions"
)]
const MAX_LAYOUT_WIDTH: f32 =
    COLUMN_WIDTH * MAX_COLUMNS as f32 + COLUMN_GAP * (MAX_COLUMNS - 1) as f32 + PANEL_PADDING * 2.0;
/// Ground plane size — same as panel width (layout is already in meters).
const GROUND_SIZE: f32 = MAX_LAYOUT_WIDTH;
/// Distance between stacked panels along Z (on the ground plane).
const STACK_DEPTH: f32 = 1.25;

// ── Key repeat ───────────────────────────────────────────────────────────────

/// Rows added per frame during animated column fill.
const ROWS_PER_FRAME: usize = 20;
const FPS_UPDATE_INTERVAL: f32 = 1.0;
const PERF_PEAK_WINDOW_SECS: f32 = 5.0;

// ── Colors ───────────────────────────────────────────────────────────────────

const BORDER_COLOR: Color = Color::srgb(0.39, 0.43, 0.47);
const BG_COLOR: Color = Color::srgb(0.157, 0.173, 0.204);
const DIVIDER_COLOR: Color = Color::srgb(0.235, 0.51, 0.706);

// ── Overlay panel constants ──────────────────────────────────────────────────

/// Background color for overlay panels.
const OVERLAY_BG: Color = Color::srgba(0.1, 0.1, 0.12, 0.85);

/// Border color for overlay panels.
const OVERLAY_BORDER_COLOR: Color = Color::srgb(0.4, 0.4, 0.45);

/// Font size for overlay panel text (in millimeters).
const OVERLAY_FONT_SIZE: f32 = 3.5;

/// Layout dimensions for the status panel (in meters).
const STATUS_LAYOUT_WIDTH: f32 = 0.2;
const STATUS_LAYOUT_HEIGHT: f32 = 0.03;

/// Layout dimensions for the controls panel (in meters).
const CONTROLS_LAYOUT_WIDTH: f32 = 0.08;
const CONTROLS_LAYOUT_HEIGHT: f32 = 0.02;

/// Source text for row values.
const SOURCE_TEXT: &str = "bevy diegetic layout engine text rendering msdf atlas glyph quad mesh \
    shader pipeline parley shaping font registry plugin system resource component query transform \
    camera projection orthographic perspective viewport world entity spawn despawn commands insert \
    remove compute propagate sizing grow fit fixed percent padding border direction align children \
    element builder tree result render command rectangle scissor culling material texture sampler \
    uniform binding vertex index normal color alpha blend mask discard fragment screen pixel range \
    distance median clamp smooth antialiasing kerning advance baseline ascent descent bearing \
    rasterize bitmap canonical prepopulate allocator shelf etagere unicode bidi cluster glyph";

// ── Resources / components ───────────────────────────────────────────────────

#[derive(Resource)]
struct StressControls {
    row_count:        usize,
    /// Target row count — the row count we're animating toward.
    /// When `target > row_count`, rows are added at [`ROWS_PER_FRAME`].
    /// When `target < row_count`, rows are removed at [`ROWS_PER_FRAME`].
    target_row_count: usize,
    /// Color rotation angle in radians.
    hue_angle:        f32,
}

impl Default for StressControls {
    fn default() -> Self {
        Self {
            row_count:        0,
            target_row_count: 0,
            hue_angle:        0.0,
        }
    }
}

#[derive(Component)]
struct StressPanel(usize);

#[derive(Component)]
struct GroundPlane;

/// Marker for the combined status overlay panel (FPS + row/panel counts).
#[derive(Component)]
struct StatusPanel;

/// Marker for the controls help panel.
#[derive(Component)]
struct ControlsPanel;

#[derive(Resource, Default)]
struct StressPerfStats {
    panel_update_ms: f32,
    tree_build_ms:   f32,
    tree_builds:     usize,
    panel_count:     usize,
}

#[derive(Clone, Copy)]
struct PerfSnapshot {
    timestamp: f32,
    fps:       f32,
    frame_ms:  f32,
    update_ms: f32,
    tree_ms:   f32,
    layout_ms: f32,
    text_ms:   f32,
}

/// Tracks the last displayed status values so we only rebuild when they change.
#[derive(Resource, Default)]
struct LastDisplayedStatus {
    text: String,
}

// ── App ──────────────────────────────────────────────────────────────────────

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault))
        .add_plugins(DiegeticUiPlugin)
        .add_plugins(LagrangePlugin)
        .init_resource::<StressControls>()
        .init_resource::<StressPerfStats>()
        .init_resource::<LastDisplayedStatus>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                handle_input,
                animate_row_count,
                advance_color_rotation,
                update_status_panel,
                update_panels,
                resize_ground_plane,
            )
                .chain(),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground plane.
    commands.spawn((
        GroundPlane,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE, STACK_DEPTH))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.08, 0.08, 0.08, 0.5),
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
    ));

    // Directional lights.
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.5, 1.5, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(-0.5, 1.5, -1.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Camera.
    commands.spawn((
        AmbientLight {
            color: Color::WHITE,
            brightness: 1000.0,
            ..default()
        },
        OrbitCam {
            focus: Vec3::new(0.0, 1.0, GROUND_SIZE * 0.25),
            radius: Some(8.0),
            yaw: Some(0.0),
            pitch: Some(0.35),
            input_control: Some(InputControl {
                trackpad: Some(TrackpadInput {
                    behavior:    TrackpadBehavior::blender_default(),
                    sensitivity: 0.5,
                }),
                ..default()
            }),
            ..default()
        },
    ));

    // Status panel (combined FPS + row/panel counts) — top-right area.
    commands.spawn((
        StatusPanel,
        DiegeticPanel {
            tree: build_status_panel("fps: --  ms: --  rows: 0  panels: 0"),
            width: STATUS_LAYOUT_WIDTH,
            height: STATUS_LAYOUT_HEIGHT,
            font_unit: Some(Unit::Millimeters),
            ..default()
        },
        Transform::from_xyz(3.9, 5.015, 2.0),
    ));

    // Controls panel (static help text) — bottom-left area.
    commands.spawn((
        ControlsPanel,
        DiegeticPanel {
            tree: build_controls_panel(),
            width: CONTROLS_LAYOUT_WIDTH,
            height: CONTROLS_LAYOUT_HEIGHT,
            font_unit: Some(Unit::Millimeters),
            ..default()
        },
        Transform::from_xyz(-4.04, 0.51, 3.0),
    ));
}

fn build_status_panel(text: &str) -> LayoutTree {
    let mut builder = LayoutBuilder::new(STATUS_LAYOUT_WIDTH, STATUS_LAYOUT_HEIGHT);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .padding(Padding::all(0.002))
            .direction(Direction::TopToBottom)
            .child_gap(0.001)
            .background(OVERLAY_BG)
            .border(Border::all(0.0005, OVERLAY_BORDER_COLOR)),
        |b| {
            b.text(
                text,
                LayoutTextStyle::new(OVERLAY_FONT_SIZE)
                    .with_color(Color::srgba(1.0, 1.0, 1.0, 0.9))
                    .with_shadow_mode(GlyphShadowMode::None),
            );
        },
    );
    builder.build()
}

fn build_controls_panel() -> LayoutTree {
    let mut builder = LayoutBuilder::new(CONTROLS_LAYOUT_WIDTH, CONTROLS_LAYOUT_HEIGHT);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .padding(Padding::all(0.002))
            .direction(Direction::TopToBottom)
            .child_gap(0.001)
            .background(OVERLAY_BG)
            .border(Border::all(0.0005, OVERLAY_BORDER_COLOR)),
        |b| {
            b.text(
                "'+' add  '-' remove",
                LayoutTextStyle::new(OVERLAY_FONT_SIZE)
                    .with_color(Color::srgba(1.0, 1.0, 1.0, 0.7))
                    .with_shadow_mode(GlyphShadowMode::None),
            );
            b.text(
                "(hold to accelerate)",
                LayoutTextStyle::new(OVERLAY_FONT_SIZE)
                    .with_color(Color::srgba(1.0, 1.0, 1.0, 0.5))
                    .with_shadow_mode(GlyphShadowMode::None),
            );
        },
    );
    builder.build()
}

// ── Input ────────────────────────────────────────────────────────────────────

/// Sets the target row count. Each key press/hold advances the target by
/// one column's worth of rows. The actual `row_count` animates toward the
/// target at [`ROWS_PER_FRAME`] rows per frame.
fn handle_input(keyboard: Res<ButtonInput<KeyCode>>, mut state: ResMut<StressControls>) {
    let adding = keyboard.pressed(KeyCode::Equal);
    let removing = keyboard.pressed(KeyCode::Minus);

    if !adding && !removing {
        // Stop animation when key is released.
        state.target_row_count = state.row_count;
        return;
    }

    let rpc = rows_per_column(state.target_row_count == 0);

    if adding {
        state.target_row_count += rpc;
    }
    if removing {
        state.target_row_count = state.target_row_count.saturating_sub(rpc);
    }
}

/// Animates `row_count` toward `target_row_count` at [`ROWS_PER_FRAME`].
fn animate_row_count(mut state: ResMut<StressControls>) {
    if state.row_count < state.target_row_count {
        let step = ROWS_PER_FRAME.min(state.target_row_count - state.row_count);
        state.row_count += step;
    } else if state.row_count > state.target_row_count {
        let step = ROWS_PER_FRAME.min(state.row_count - state.target_row_count);
        state.row_count -= step;
    }
}

// ── Color rotation ──────────────────────────────────────────────────────────

/// Advances the rainbow hue rotation every frame at a fixed angular velocity.
///
/// Uses [`HueOffset`] component — a separate component from [`DiegeticPanel`],
/// so changing it does not trigger layout recomputation or text mesh rebuilds.
/// The library's `sync_panel_hue_offset` system propagates it to the shared
/// GPU material automatically.
fn advance_color_rotation(
    mut state: ResMut<StressControls>,
    panels: Query<Entity, With<StressPanel>>,
    mut commands: Commands,
) {
    if state.row_count == 0 {
        return;
    }

    // Fixed angular velocity — visual speed stays constant regardless of
    // row count. One full rotation every ~5 seconds at 60 FPS.
    let step = std::f32::consts::TAU / 300.0;
    state.hue_angle = (state.hue_angle + step) % std::f32::consts::TAU;

    for entity in &panels {
        commands.entity(entity).insert(HueOffset(state.hue_angle));
    }
}

// ── Status panel ─────────────────────────────────────────────────────────────

fn update_status_panel(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    state: Res<StressControls>,
    stress_perf: Res<StressPerfStats>,
    diegetic_perf: Res<DiegeticPerfStats>,
    mut panels: Query<&mut DiegeticPanel, With<StatusPanel>>,
    mut last_displayed: ResMut<LastDisplayedStatus>,
    mut timer: Local<Option<Timer>>,
    mut history: Local<VecDeque<PerfSnapshot>>,
) {
    let timer =
        timer.get_or_insert_with(|| Timer::from_seconds(FPS_UPDATE_INTERVAL, TimerMode::Repeating));
    timer.tick(time.delta());
    if !timer.just_finished() {
        return;
    }
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(bevy::diagnostic::Diagnostic::smoothed);
    let frame_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(bevy::diagnostic::Diagnostic::smoothed);
    let fps_str = fps.map_or_else(|| "--".to_string(), |v| format!("{v:.0}"));
    let fps_value = fps.unwrap_or(0.0).to_f32();
    let frame_ms_value = frame_ms.unwrap_or(0.0).to_f32();
    let ms_str = frame_ms.map_or_else(|| "--".to_string(), |v| format!("{v:.1}"));

    history.push_back(PerfSnapshot {
        timestamp: time.elapsed_secs(),
        fps:       fps_value,
        frame_ms:  frame_ms_value,
        update_ms: stress_perf.panel_update_ms,
        tree_ms:   stress_perf.tree_build_ms,
        layout_ms: diegetic_perf.last_compute_ms,
        text_ms:   diegetic_perf.last_text_extract_ms,
    });

    let cutoff = time.elapsed_secs() - PERF_PEAK_WINDOW_SECS;
    while history
        .front()
        .is_some_and(|sample| sample.timestamp < cutoff)
    {
        history.pop_front();
    }

    let mut max_fps = 0.0_f32;
    let mut max_frame_ms = 0.0_f32;
    let mut max_update_ms = 0.0_f32;
    let mut max_tree_ms = 0.0_f32;
    let mut max_layout_ms = 0.0_f32;
    let mut max_text_ms = 0.0_f32;
    for sample in &history {
        max_fps = max_fps.max(sample.fps);
        max_frame_ms = max_frame_ms.max(sample.frame_ms);
        max_update_ms = max_update_ms.max(sample.update_ms);
        max_tree_ms = max_tree_ms.max(sample.tree_ms);
        max_layout_ms = max_layout_ms.max(sample.layout_ms);
        max_text_ms = max_text_ms.max(sample.text_ms);
    }

    let new_text = format!(
        "{tag_now:<7} fps: {fps:>4}  ms: {frame:>5}  upd: {upd:>5}ms  tree: {tree:>5}ms  layout: {layout:>5}ms  text: {text_ms:>5}ms\n{tag_max:<7} fps: {max_fps:>4}  ms: {max_frame:>5}  upd: {max_upd:>5}ms  tree: {max_tree:>5}ms  layout: {max_layout:>5}ms  text: {max_text:>5}ms\npanels: {}  rows: {}",
        stress_perf.panel_count,
        state.row_count,
        tag_now = "now",
        tag_max = "5s max",
        fps = fps_str,
        frame = ms_str,
        upd = format!("{:.1}", stress_perf.panel_update_ms),
        tree = format!("{:.1}", stress_perf.tree_build_ms),
        layout = format!("{:.1}", diegetic_perf.last_compute_ms),
        text_ms = format!("{:.1}", diegetic_perf.last_text_extract_ms),
        max_fps = format!("{:.0}", max_fps),
        max_frame = format!("{:.1}", max_frame_ms),
        max_upd = format!("{:.1}", max_update_ms),
        max_tree = format!("{:.1}", max_tree_ms),
        max_layout = format!("{:.1}", max_layout_ms),
        max_text = format!("{:.1}", max_text_ms),
    );

    if new_text != last_displayed.text {
        last_displayed.text.clone_from(&new_text);
        for mut panel in &mut panels {
            panel.tree = build_status_panel(&new_text);
        }
    }
}

// ── Panel management ─────────────────────────────────────────────────────────

/// Rows that fit in one column.
fn rows_per_column(is_header_column: bool) -> usize {
    let available = if is_header_column {
        LAYOUT_HEIGHT - HEADER_HEIGHT
    } else {
        LAYOUT_HEIGHT
    };
    let count = (available / ROW_HEIGHT).to_usize();
    count.max(1)
}

/// Rows that fit in one full panel (all `MAX_COLUMNS` columns).
fn rows_per_panel() -> usize {
    let first = rows_per_column(true);
    let other = rows_per_column(false);
    first + (MAX_COLUMNS - 1) * other
}

fn update_panels(
    state: Res<StressControls>,
    existing: Query<(Entity, &StressPanel)>,
    mut panels: Query<(&mut DiegeticPanel, &mut Transform)>,
    mut commands: Commands,
    mut perf: ResMut<StressPerfStats>,
    mut last_panel_count: Local<usize>,
    mut last_row_count: Local<Option<usize>>,
) {
    if last_row_count.as_ref() == Some(&state.row_count) {
        return;
    }
    *last_row_count = Some(state.row_count);
    let update_start = Instant::now();
    let mut tree_build_ms = 0.0_f32;
    let mut tree_builds = 0_usize;

    let rpp = rows_per_panel();
    let words: Vec<&str> = SOURCE_TEXT.split_whitespace().collect();

    let needed = if state.row_count == 0 {
        1
    } else {
        state.row_count.div_ceil(rpp)
    };

    // Despawn excess.
    for (entity, sp) in &existing {
        if sp.0 >= needed {
            commands.entity(entity).despawn();
        }
    }

    let ww = MAX_LAYOUT_WIDTH;
    let wh = LAYOUT_HEIGHT;

    // Spawn missing.
    for idx in *last_panel_count..needed {
        let tree_start = Instant::now();
        let tree = build_panel_tree(&state, idx, rpp, &words);
        tree_build_ms += tree_start.elapsed().as_secs_f32() * 1000.0;
        tree_builds += 1;
        commands.spawn((
            StressPanel(idx),
            DiegeticPanel {
                tree,
                width: MAX_LAYOUT_WIDTH,
                height: LAYOUT_HEIGHT,
                font_unit: Some(Unit::Millimeters),
                ..default()
            },
            panel_transform(idx, needed, ww, wh),
        ));
    }
    *last_panel_count = needed;

    // Update existing panels.
    let active_panel_idx = needed - 1;
    let panel_count_changed = needed != existing.iter().count();

    for (entity, sp) in &existing {
        if sp.0 < needed
            && let Ok((mut panel, mut transform)) = panels.get_mut(entity)
        {
            if sp.0 == active_panel_idx {
                // Active panel — content changed, rebuild tree.
                let tree_start = Instant::now();
                panel.tree = build_panel_tree(&state, sp.0, rpp, &words);
                tree_build_ms += tree_start.elapsed().as_secs_f32() * 1000.0;
                tree_builds += 1;
            } else if panel_count_changed {
                // Panel count changed — rebuild frozen panels once to
                // redistribute hue spacing against the new row_count.
                // Between boundary crossings, hue_offset on the shader
                // handles color rotation with zero CPU cost.
                panel.tree = build_panel_tree(&state, sp.0, rpp, &words);
            }
            *transform = panel_transform(sp.0, needed, ww, wh);
        }
    }

    perf.panel_update_ms = update_start.elapsed().as_secs_f32() * 1000.0;
    perf.tree_build_ms = tree_build_ms;
    perf.tree_builds = tree_builds;
    perf.panel_count = needed;
}

/// Resizes and repositions the ground plane to cover all stacked panels.
fn resize_ground_plane(
    perf: Res<StressPerfStats>,
    mut ground: Query<(&mut Mesh3d, &mut Transform), With<GroundPlane>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut last_panel_count: Local<usize>,
) {
    if perf.panel_count == *last_panel_count {
        return;
    }
    *last_panel_count = perf.panel_count;

    // Shallow plane: just deep enough to reach one STACK_DEPTH behind the
    // last panel so shadows land on the ground.
    let depth = perf.panel_count.max(1).to_f32() * STACK_DEPTH;
    let width = GROUND_SIZE;

    for (mut mesh3d, mut transform) in &mut ground {
        mesh3d.0 = meshes.add(Plane3d::default().mesh().size(width, depth));
        // Center the plane under the panels. Front edge at z = GROUND_SIZE/2,
        // back edge at z = GROUND_SIZE/2 - depth.
        let center_z = GROUND_SIZE.mul_add(0.5, -(depth * 0.5));
        transform.translation.z = center_z;
    }
}

/// Panel position — aligned with the ground plane's X axis.
/// Panel left edge = plane left edge. Older panels pushed backward along Z.
fn panel_transform(
    panel_idx: usize,
    total: usize,
    _world_width: f32,
    world_height: f32,
) -> Transform {
    let depth_from_front = (total - 1 - panel_idx).to_f32();
    // Front panel at z=0 (forward edge of ground plane), older panels push back.
    let z = GROUND_SIZE.mul_add(0.5, -(depth_from_front * STACK_DEPTH));
    // Panel top-left edge aligns with plane left edge.
    let x = -GROUND_SIZE * 0.5;
    // Panel top sits above the ground (TopLeft anchor: y = top edge).
    let y = world_height + 0.3;
    Transform::from_xyz(x, y, z)
}

fn build_panel_tree(
    state: &StressControls,
    panel_idx: usize,
    rpp: usize,
    words: &[&str],
) -> bevy_diegetic::LayoutTree {
    let panel_start = panel_idx * rpp;
    let panel_rows = state.row_count.saturating_sub(panel_start).min(rpp);
    let first_col_rows = rows_per_column(panel_idx == 0);
    let other_col_rows = rows_per_column(false);

    let cols = if panel_rows <= first_col_rows {
        1
    } else {
        let remaining = panel_rows - first_col_rows;
        1 + remaining.div_ceil(other_col_rows)
    };

    let mut builder = LayoutBuilder::new(MAX_LAYOUT_WIDTH, LAYOUT_HEIGHT);

    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(COLUMN_GAP)
            .padding(Padding::all(0.03))
            .background(BG_COLOR)
            .border(Border::all(0.01, BORDER_COLOR)),
        |b| {
            let mut row_cursor = panel_start;
            for col in 0..cols {
                let is_first = panel_idx == 0 && col == 0;
                let col_rows = if is_first {
                    first_col_rows
                } else {
                    other_col_rows
                };
                let end = (row_cursor + col_rows).min(panel_start + panel_rows);

                b.with(
                    El::new()
                        .width(Sizing::fixed(COLUMN_WIDTH))
                        .height(Sizing::GROW)
                        .direction(Direction::TopToBottom)
                        .child_gap(0.01)
                        .padding(Padding::all(0.02))
                        .border(Border::all(0.01, BORDER_COLOR)),
                    |b| {
                        if is_first {
                            b.text(
                                "'+' add  '-' remove",
                                LayoutTextStyle::new(FONT_SIZE)
                                    .with_shadow_mode(GlyphShadowMode::None),
                            );
                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .height(Sizing::fixed(0.01))
                                    .background(DIVIDER_COLOR),
                                |_| {},
                            );
                        }

                        for i in row_cursor..end {
                            let hue = if state.row_count > 0 {
                                360.0 * (i.to_f32() / state.row_count.to_f32())
                            } else {
                                0.0
                            };
                            let color = Color::hsl(hue, 1.0, 0.7);
                            let label = format!("item {i}:");
                            let value = words[i % words.len()];
                            let config = LayoutTextStyle::new(FONT_SIZE)
                                .with_color(color)
                                .with_shadow_mode(GlyphShadowMode::None);
                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .height(Sizing::FIT)
                                    .direction(Direction::LeftToRight)
                                    .child_gap(ROW_GAP),
                                |b| {
                                    b.text(&label, config.clone());
                                    b.with(
                                        El::new().width(Sizing::GROW).height(Sizing::fixed(0.01)),
                                        |_| {},
                                    );
                                    b.text(value, config);
                                },
                            );
                        }
                        row_cursor = end;
                    },
                );
            }
        },
    );

    builder.build()
}
