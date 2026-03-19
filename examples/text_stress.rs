//! Text stress test — add/remove rows to measure per-element rendering cost.
//!
//! Press '+' to add rows, '-' to remove (hold for accelerating repeat).
//! FPS shown via 2D overlay.
//!
//! Rows fill columns left-to-right within a panel. When the panel reaches
//! screen width, it pushes backward and a new panel spawns in front.

use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
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
/// Max layout width — exactly fits MAX_COLUMNS with gaps and padding.
#[allow(clippy::cast_precision_loss)]
const MAX_LAYOUT_WIDTH: f32 =
    COLUMN_WIDTH * MAX_COLUMNS as f32 + COLUMN_GAP * (MAX_COLUMNS - 1) as f32 + PANEL_PADDING * 2.0;
/// Ground plane size — derived from panel width.
const GROUND_SIZE: f32 = MAX_LAYOUT_WIDTH * SCALE;
/// Distance between stacked panels along Z (on the ground plane).
const STACK_DEPTH: f32 = 1.25;

// ── Key repeat ───────────────────────────────────────────────────────────────

const REPEAT_START: f32 = 0.12;
const REPEAT_MIN: f32 = 0.01;
const REPEAT_ACCEL: f32 = 0.85;
const FPS_UPDATE_INTERVAL: f32 = 1.0;

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
    repeat_timer:    Timer,
    repeat_interval: f32,
    hold_duration:   f32,
    row_count:       usize,
}

impl Default for StressControls {
    fn default() -> Self {
        Self {
            repeat_timer:    Timer::from_seconds(REPEAT_START, TimerMode::Repeating),
            repeat_interval: REPEAT_START,
            hold_duration:   0.0,
            row_count:       0,
        }
    }
}

#[derive(Component)]
struct StressPanel(usize);

#[derive(Component)]
struct FpsOverlay;

// ── App ──────────────────────────────────────────────────────────────────────

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(BrpExtrasPlugin::default())
        .add_plugins(DiegeticUiPlugin)
        .add_plugins(PanOrbitCameraPlugin)
        .init_resource::<StressControls>()
        .add_systems(Startup, setup)
        .add_systems(Update, (handle_input, update_fps_overlay, update_panels))
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // FPS overlay.
    commands.spawn((
        FpsOverlay,
        Text::new("fps: --  ms: --  rows: 0"),
        TextFont {
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
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE, GROUND_SIZE))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.5, 0.3),
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
        PanOrbitCamera {
            focus: Vec3::new(0.0, 1.0, GROUND_SIZE * 0.25),
            radius: Some(8.0),
            yaw: Some(0.0),
            pitch: Some(0.35),
            trackpad_behavior: TrackpadBehavior::blender_default(),
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
    ));

    // Help text.
    commands.spawn((
        Text::new("'+' add  '-' remove  (hold to accelerate)"),
        TextFont {
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
}

// ── Input ────────────────────────────────────────────────────────────────────

fn handle_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut state: ResMut<StressControls>,
) {
    let adding = keyboard.pressed(KeyCode::Equal);
    let removing = keyboard.pressed(KeyCode::Minus);

    if !adding && !removing {
        if state.hold_duration != 0.0 || (state.repeat_interval - REPEAT_START).abs() > f32::EPSILON
        {
            state.hold_duration = 0.0;
            state.repeat_interval = REPEAT_START;
            state
                .repeat_timer
                .set_duration(std::time::Duration::from_secs_f32(REPEAT_START));
        }
        return;
    }

    if keyboard.just_pressed(KeyCode::Equal) {
        state.row_count += 1;
        state.hold_duration = 0.0;
        state.repeat_timer.reset();
        return;
    }
    if keyboard.just_pressed(KeyCode::Minus) && state.row_count > 0 {
        state.row_count -= 1;
        state.hold_duration = 0.0;
        state.repeat_timer.reset();
        return;
    }

    state.hold_duration += time.delta_secs();
    let new_interval = (REPEAT_START * REPEAT_ACCEL.powf(state.hold_duration)).max(REPEAT_MIN);
    if (new_interval - state.repeat_interval).abs() > 0.001 {
        state.repeat_interval = new_interval;
        state
            .repeat_timer
            .set_duration(std::time::Duration::from_secs_f32(new_interval));
    }

    state.repeat_timer.tick(time.delta());
    if state.repeat_timer.just_finished() {
        if adding {
            state.row_count += 1;
        } else if state.row_count > 0 {
            state.row_count -= 1;
        }
    }
}

// ── FPS overlay ──────────────────────────────────────────────────────────────

fn update_fps_overlay(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    state: Res<StressControls>,
    mut overlay: Query<&mut Text, With<FpsOverlay>>,
    mut timer: Local<Option<Timer>>,
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
    let ms_str = frame_ms.map_or_else(|| "--".to_string(), |v| format!("{v:.1}"));
    for mut text in &mut overlay {
        **text = format!("fps: {fps_str}  ms: {ms_str}  rows: {}", state.row_count);
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

/// Rows that fit in one full panel (all MAX_COLUMNS columns).
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
    mut last_panel_count: Local<usize>,
    mut last_row_count: Local<Option<usize>>,
) {
    if last_row_count.as_ref() == Some(&state.row_count) {
        return;
    }
    *last_row_count = Some(state.row_count);

    let rpp = rows_per_panel();
    let words: Vec<&str> = SOURCE_TEXT.split_whitespace().collect();

    let needed = if state.row_count == 0 {
        1
    } else {
        (state.row_count + rpp - 1) / rpp
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
        commands.spawn((
            StressPanel(idx),
            DiegeticPanel {
                tree:          build_panel_tree(&state, idx, rpp, &words),
                layout_width:  MAX_LAYOUT_WIDTH,
                layout_height: LAYOUT_HEIGHT,
                world_width:   ww,
                world_height:  wh,
            },
            panel_transform(idx, needed, ww, wh),
        ));
    }
    *last_panel_count = needed;

    // Update existing.
    for (entity, sp) in &existing {
        if sp.0 < needed {
            if let Ok((mut panel, mut transform)) = panels.get_mut(entity) {
                panel.tree = build_panel_tree(&state, sp.0, rpp, &words);
                // Only update transform for Z-depth repositioning.
                *transform = panel_transform(sp.0, needed, ww, wh);
            }
        }
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
    let z = GROUND_SIZE * 0.5 - depth_from_front * STACK_DEPTH;
    // Panel left edge aligns with plane left edge.
    let ww = MAX_LAYOUT_WIDTH * SCALE;
    let x = -GROUND_SIZE * 0.5 + ww * 0.5;
    // Panel bottom sits above the ground.
    let y = world_height * 0.5 + 0.3;
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
        1 + (remaining + other_col_rows - 1) / other_col_rows
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
                            b.text("'+' add  '-' remove", TextConfig::new(FONT_SIZE));
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
                            let color = Color::hsl(hue, 0.8, 0.6);
                            let label = format!("item {i}:");
                            let value = words[i % words.len()];
                            let config = TextConfig::new(FONT_SIZE).with_color(color);
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
