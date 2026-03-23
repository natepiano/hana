//! Text stress test — add/remove rows to measure per-element rendering cost.
//!
//! Press '+' to add rows, '-' to remove (hold for accelerating repeat).
//! FPS shown via 2D overlay.
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
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextConfig;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;

// ── Text / layout constants ──────────────────────────────────────────────────

const FONT_SIZE: f32 = 6.0;
const ROW_HEIGHT: f32 = FONT_SIZE + 1.0;
const ROW_GAP: f32 = 5.0;
const COLUMN_GAP: f32 = 5.0;
const HEADER_HEIGHT: f32 = ROW_HEIGHT + 1.0;

/// Column width in layout units.
///
/// This is an explicit layout constraint, not just an estimate. Panel width is
/// budgeted from this value via `MAX_LAYOUT_WIDTH`, and each column is sized to
/// this width in the layout tree.
const COLUMN_WIDTH: f32 = 100.0;
/// Layout height per panel.
const LAYOUT_HEIGHT: f32 = 200.0;
/// Scale: world units per layout unit.
const SCALE: f32 = 0.01;
/// Padding on the outer panel in layout units.
const PANEL_PADDING: f32 = 6.0;

// ── Scene constants ──────────────────────────────────────────────────────────

/// How many columns per panel.
const MAX_COLUMNS: usize = 8;
/// Max layout width — exactly fits `MAX_COLUMNS` with gaps and padding.
#[allow(clippy::cast_precision_loss)]
const MAX_LAYOUT_WIDTH: f32 =
    COLUMN_WIDTH * MAX_COLUMNS as f32 + COLUMN_GAP * (MAX_COLUMNS - 1) as f32 + PANEL_PADDING * 2.0;
/// Ground plane size — derived from panel width.
const GROUND_SIZE: f32 = MAX_LAYOUT_WIDTH * SCALE;
/// Distance between stacked panels along Z (on the ground plane).
const STACK_DEPTH: f32 = 1.25;

// ── Key repeat ───────────────────────────────────────────────────────────────

/// Rows added per frame during animated column fill.
const ROWS_PER_FRAME: usize = 20;
const FPS_UPDATE_INTERVAL: f32 = 1.0;
const PERF_PEAK_WINDOW_SECS: f32 = 5.0;

// ── Colors ───────────────────────────────────────────────────────────────────

const BORDER_COLOR: bevy::color::Color = bevy::color::Color::srgb(0.39, 0.43, 0.47);
const BG_COLOR: bevy::color::Color = bevy::color::Color::srgb(0.157, 0.173, 0.204);
const DIVIDER_COLOR: bevy::color::Color = bevy::color::Color::srgb(0.235, 0.51, 0.706);

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

#[derive(Component)]
struct FpsOverlay;

#[derive(Component)]
struct StatsOverlay;

#[derive(Resource, Default)]
#[allow(clippy::struct_field_names)]
struct StressPerfStats {
    last_panel_update_ms: f32,
    last_tree_build_ms:   f32,
    last_tree_builds:     usize,
    last_panel_count:     usize,
}

#[derive(Clone, Copy)]
struct PerfSnapshot {
    timestamp:     f32,
    fps:           f32,
    frame_ms:      f32,
    rows:          usize,
    panels:        usize,
    update_ms:     f32,
    tree_ms:       f32,
    tree_builds:   usize,
    layout_ms:     f32,
    layout_panels: usize,
    text_ms:       f32,
    text_panels:   usize,
}

// ── App ──────────────────────────────────────────────────────────────────────

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault))
        .add_plugins(DiegeticUiPlugin)
        .add_plugins(PanOrbitCameraPlugin)
        .init_resource::<StressControls>()
        .init_resource::<StressPerfStats>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                handle_input,
                animate_row_count,
                advance_color_rotation,
                update_fps_overlay,
                update_stats_overlay,
                update_panels,
                resize_ground_plane,
            )
                .chain(),
        )
        .run();
}

fn setup(
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mono_font = asset_server.load("fonts/JetBrainsMono-Regular.ttf");

    // FPS overlay.
    commands.spawn((
        FpsOverlay,
        Text::new("fps: --  ms: --  rows: 0"),
        TextFont {
            font: mono_font.clone(),
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(8.0),
            ..default()
        },
    ));

    // Ground plane.
    commands.spawn((
        GroundPlane,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE, STACK_DEPTH))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.3, 0.5, 0.3, 0.5),
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
    ));

    // Point light.
    commands.spawn((
        PointLight {
            intensity: 500_000.0,
            shadows_enabled: true,
            range: 30.0,
            ..default()
        },
        Transform::from_xyz(2.0, 5.0, 6.0),
    ));

    // Camera.
    commands.spawn((
        Camera3d::default(),
        AmbientLight {
            color: Color::WHITE,
            brightness: 1000.0,
            ..default()
        },
        PanOrbitCamera {
            focus: Vec3::new(0.0, 1.0, GROUND_SIZE * 0.25),
            radius: Some(8.0),
            yaw: Some(0.0),
            pitch: Some(0.35),
            trackpad_behavior: TrackpadBehavior::blender_default(),
            trackpad_sensitivity: 0.5,
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
    ));

    // Help text.
    commands.spawn((
        Text::new("'+' add  '-' remove  (hold to accelerate)"),
        TextFont {
            font: mono_font.clone(),
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));

    // Stats overlay (bottom right).
    commands.spawn((
        StatsOverlay,
        Text::new("rows: 0  panels: 0"),
        TextFont {
            font: mono_font,
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(12.0),
            right: Val::Px(12.0),
            ..default()
        },
    ));
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
#[allow(clippy::cast_precision_loss)]
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

// ── FPS overlay ──────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn update_fps_overlay(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    state: Res<StressControls>,
    stress_perf: Res<StressPerfStats>,
    diegetic_perf: Res<DiegeticPerfStats>,
    mut overlay: Query<&mut Text, With<FpsOverlay>>,
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
    #[allow(clippy::cast_possible_truncation)]
    let fps_value = fps.unwrap_or(0.0) as f32;
    #[allow(clippy::cast_possible_truncation)]
    let frame_ms_value = frame_ms.unwrap_or(0.0) as f32;
    let ms_str = frame_ms.map_or_else(|| "--".to_string(), |v| format!("{v:.1}"));

    history.push_back(PerfSnapshot {
        timestamp:     time.elapsed_secs(),
        fps:           fps_value,
        frame_ms:      frame_ms_value,
        rows:          state.row_count,
        panels:        stress_perf.last_panel_count,
        update_ms:     stress_perf.last_panel_update_ms,
        tree_ms:       stress_perf.last_tree_build_ms,
        tree_builds:   stress_perf.last_tree_builds,
        layout_ms:     diegetic_perf.last_compute_ms,
        layout_panels: diegetic_perf.last_compute_panels,
        text_ms:       diegetic_perf.last_text_extract_ms,
        text_panels:   diegetic_perf.last_text_extract_panels,
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
    let mut max_rows = 0_usize;
    let mut max_panels = 0_usize;
    let mut max_update_ms = 0.0_f32;
    let mut max_tree_ms = 0.0_f32;
    let mut max_tree_builds = 0_usize;
    let mut max_layout_ms = 0.0_f32;
    let mut max_layout_panels = 0_usize;
    let mut max_text_ms = 0.0_f32;
    let mut max_text_panels = 0_usize;
    for sample in &history {
        max_fps = max_fps.max(sample.fps);
        max_frame_ms = max_frame_ms.max(sample.frame_ms);
        max_rows = max_rows.max(sample.rows);
        max_panels = max_panels.max(sample.panels);
        max_update_ms = max_update_ms.max(sample.update_ms);
        max_tree_ms = max_tree_ms.max(sample.tree_ms);
        max_tree_builds = max_tree_builds.max(sample.tree_builds);
        max_layout_ms = max_layout_ms.max(sample.layout_ms);
        max_layout_panels = max_layout_panels.max(sample.layout_panels);
        max_text_ms = max_text_ms.max(sample.text_ms);
        max_text_panels = max_text_panels.max(sample.text_panels);
    }

    for mut text in &mut overlay {
        **text = format!(
            "{tag_now:<7} fps: {fps:>4}  ms: {frame:>5}  upd: {upd:>5}ms  tree: {tree:>5}ms  layout: {layout:>5}ms  text: {text_ms:>5}ms\n{tag_max:<7} fps: {max_fps:>4}  ms: {max_frame:>5}  upd: {max_upd:>5}ms  tree: {max_tree:>5}ms  layout: {max_layout:>5}ms  text: {max_text:>5}ms",
            tag_now = "now",
            tag_max = "5s max",
            fps = fps_str,
            frame = ms_str,
            upd = format!("{:.1}", stress_perf.last_panel_update_ms),
            tree = format!("{:.1}", stress_perf.last_tree_build_ms),
            layout = format!("{:.1}", diegetic_perf.last_compute_ms),
            text_ms = format!("{:.1}", diegetic_perf.last_text_extract_ms),
            max_fps = format!("{:.0}", max_fps),
            max_frame = format!("{:.1}", max_frame_ms),
            max_upd = format!("{:.1}", max_update_ms),
            max_tree = format!("{:.1}", max_tree_ms),
            max_layout = format!("{:.1}", max_layout_ms),
            max_text = format!("{:.1}", max_text_ms),
        );
    }
}

fn update_stats_overlay(
    state: Res<StressControls>,
    stress_perf: Res<StressPerfStats>,
    mut overlay: Query<&mut Text, With<StatsOverlay>>,
) {
    for mut text in &mut overlay {
        **text = format!(
            "panels: {}  rows: {}",
            stress_perf.last_panel_count, state.row_count,
        );
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = (available / ROW_HEIGHT) as usize;
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

    let ww = MAX_LAYOUT_WIDTH * SCALE;
    let wh = LAYOUT_HEIGHT * SCALE;

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
                layout_width: MAX_LAYOUT_WIDTH,
                layout_height: LAYOUT_HEIGHT,
                world_width: ww,
                world_height: wh,
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

    perf.last_panel_update_ms = update_start.elapsed().as_secs_f32() * 1000.0;
    perf.last_tree_build_ms = tree_build_ms;
    perf.last_tree_builds = tree_builds;
    perf.last_panel_count = needed;
}

/// Resizes and repositions the ground plane to cover all stacked panels.
#[allow(clippy::cast_precision_loss)]
fn resize_ground_plane(
    perf: Res<StressPerfStats>,
    mut ground: Query<(&mut Mesh3d, &mut Transform), With<GroundPlane>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut last_panel_count: Local<usize>,
) {
    if perf.last_panel_count == *last_panel_count {
        return;
    }
    *last_panel_count = perf.last_panel_count;

    // Shallow plane: just deep enough to reach one STACK_DEPTH behind the
    // last panel so shadows land on the ground.
    let depth = (perf.last_panel_count.max(1)) as f32 * STACK_DEPTH;
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
#[allow(clippy::cast_precision_loss)]
fn panel_transform(
    panel_idx: usize,
    total: usize,
    _world_width: f32,
    world_height: f32,
) -> Transform {
    let depth_from_front = (total - 1 - panel_idx) as f32;
    // Front panel at z=0 (forward edge of ground plane), older panels push back.
    let z = GROUND_SIZE.mul_add(0.5, -(depth_from_front * STACK_DEPTH));
    // Panel left edge aligns with plane left edge.
    let ww = MAX_LAYOUT_WIDTH * SCALE;
    let x = (-GROUND_SIZE).mul_add(0.5, ww * 0.5);
    // Panel bottom sits above the ground.
    let y = world_height.mul_add(0.5, 0.3);
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
            .padding(Padding::all(3.0))
            .background(BG_COLOR)
            .border(Border::all(1.0, BORDER_COLOR)),
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
                        .child_gap(1.0)
                        .padding(Padding::all(2.0))
                        .border(Border::all(1.0, BORDER_COLOR)),
                    |b| {
                        if is_first {
                            b.text(
                                "'+' add  '-' remove",
                                TextConfig::new(FONT_SIZE).with_shadow_mode(GlyphShadowMode::None),
                            );
                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .height(Sizing::fixed(1.0))
                                    .background(DIVIDER_COLOR),
                                |_| {},
                            );
                        }

                        #[allow(clippy::cast_precision_loss)]
                        for i in row_cursor..end {
                            let hue = if state.row_count > 0 {
                                360.0 * (i as f32 / state.row_count as f32)
                            } else {
                                0.0
                            };
                            let color = Color::hsl(hue, 1.0, 0.7);
                            let label = format!("item {i}:");
                            let value = words[i % words.len()];
                            let config = TextConfig::new(FONT_SIZE)
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
                                        El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
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
